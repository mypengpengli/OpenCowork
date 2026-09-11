mod render;

use opencowork_agents::{
    AgentHandoffStore, AgentProfile, AgentTask, SchedulingPolicy, TaskPriority, TeamPlanner,
};
use opencowork_api::{default_openai_profile_for_model, OpenAiCompatClient, OpenAiCompatProfile};
use opencowork_app::AppRuntime;
use opencowork_commands::{handle_command, render_help, SlashCommand};
use opencowork_mcp::{
    mcp_tool_name, mcp_tool_prefix, merge_tool_definitions, McpAuthConfig, McpCatalog,
    McpCredentialRecord, McpCredentialStore, McpToolDefinition, TransportMcpExecutor,
};
use opencowork_plugins::{PluginHooks, PluginManager, PluginManagerConfig, PluginTool};
use opencowork_runtime::{
    calculate_context_thresholds, default_config_home, discover_instruction_sources,
    discover_nested_instruction_sources, discover_project_memory_source,
    discover_relevant_memory_sources, discover_relevant_memory_sources_with_provider,
    discover_team_memory_source, get_context_window_for_model, hydrate_current_session_memory,
    load_current_session_memory_source, maybe_schedule_session_memory_refresh_with_task,
    refresh_current_session_memory, refresh_current_session_memory_with_provider,
    should_report_instruction_file, skill_instruction_source_from_output,
    skill_instruction_sources_from_session, touched_paths_from_message, touched_paths_from_session,
    wait_for_session_memory_refresh, ConfigLoader, ContentBlock, ContextOptimizer,
    ContextOptimizerConfig, ConversationRuntime, HookRunner, InstructionSource, PermissionMode,
    PermissionPolicy, ProjectContext, PromptBundle, PromptComposer, ProviderBackedApiClient,
    RuntimeConfig, RuntimeHookEvent, RuntimePromptAugmenter, RuntimePromptUpdate, Session,
    SessionStore, ToolResultBudgetConfig, TurnSummary,
};
use opencowork_skills::{render_skill_listing, SkillCatalog, SkillDefinition};
use opencowork_tools::{
    force_pull_team_memory, force_push_team_memory, lsp_servers_from_runtime,
    start_team_memory_sync, stop_team_memory_sync, team_memory_sync_status, GlobalToolRegistry,
    ToolSearchSettings,
};
use render::CliTurnRenderer;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
struct McpRuntimeBundle {
    catalog: McpCatalog,
    tools: Vec<McpToolDefinition>,
    warnings: Vec<String>,
}

struct TurnExecution {
    session: Session,
    summary: TurnSummary,
    prompt: PromptBundle,
}

struct PreparedTurnContext {
    session: Session,
    prompt: PromptBundle,
}

struct ConditionalSkillPromptAugmenter {
    cwd: PathBuf,
    catalog: SkillCatalog,
    activated_skills: BTreeSet<String>,
    loaded_skills: BTreeSet<String>,
}

struct RuntimeHost<'a> {
    cwd: &'a Path,
    config: &'a RuntimeConfig,
    config_home: &'a Path,
    permission_mode: PermissionMode,
    plugin_tools: &'a [PluginTool],
    plugin_hooks: &'a PluginHooks,
    mcp_bundle: &'a McpRuntimeBundle,
}

impl ConditionalSkillPromptAugmenter {
    fn new(cwd: &Path, config_home: &Path, session: &Session) -> Self {
        let catalog = SkillCatalog::discover(&skill_roots(cwd, config_home));
        let activated_skills = catalog
            .conditional_matches(cwd, &touched_paths_from_session(cwd, session))
            .into_iter()
            .map(|skill| skill.name.clone())
            .collect();
        let loaded_skills = skill_instruction_sources_from_session(session)
            .iter()
            .filter_map(|source| source.label.strip_prefix("skill:"))
            .map(ToOwned::to_owned)
            .collect();
        Self {
            cwd: cwd.to_path_buf(),
            catalog,
            activated_skills,
            loaded_skills,
        }
    }
}

impl RuntimePromptAugmenter for ConditionalSkillPromptAugmenter {
    fn on_tool_result(
        &mut self,
        _session: &Session,
        tool_result: &opencowork_runtime::ConversationMessage,
    ) -> Vec<RuntimePromptUpdate> {
        let mut updates = lsp_context_prompt_updates(tool_result);
        updates.extend(explicit_skill_prompt_updates(
            tool_result,
            &mut self.loaded_skills,
        ));
        let touched_paths = touched_paths_from_message(&self.cwd, tool_result);
        if touched_paths.is_empty() {
            return updates;
        }

        for skill in self.catalog.conditional_matches(&self.cwd, &touched_paths) {
            if self.loaded_skills.contains(&skill.name)
                || !self.activated_skills.insert(skill.name.clone())
            {
                continue;
            }
            let Ok(definition) = self.catalog.load(&skill.name) else {
                continue;
            };
            let matched_paths =
                matched_skill_paths(&self.cwd, &touched_paths, &definition.summary.paths);
            updates.push(RuntimePromptUpdate {
                label: format!("conditional-skill:{}", definition.summary.name),
                content: render_conditional_skill_instruction(&definition, &matched_paths),
            });
        }
        updates
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WorkerServiceStatus {
    Starting,
    Running,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct WorkerServiceState {
    agent_id: String,
    pid: u32,
    poll_ms: u64,
    heartbeat_ms: u64,
    processed_jobs: usize,
    status: WorkerServiceStatus,
    stop_requested: bool,
    started_at_unix_ms: u128,
    last_heartbeat_unix_ms: u128,
    stopped_at_unix_ms: Option<u128>,
    last_error: Option<String>,
    log_path: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if env::args().nth(1).as_deref() == Some("--computer-capture") {
        return opencowork_tools::computer_capture_worker().map_err(Into::into);
    }
    let cwd = env::current_dir()?;
    let config = ConfigLoader::default_for(&cwd).load()?;
    let config_home = default_config_home();
    let _team_memory_sync_started = start_team_memory_sync(&cwd, &config_home, &config)?;
    let plugin_manager = PluginManager::new(PluginManagerConfig::new(&config_home));
    let plugin_tools = plugin_manager.aggregated_tools().unwrap_or_default();
    let plugin_hooks = plugin_manager.aggregated_hooks().unwrap_or_default();
    let mcp_bundle = resolve_mcp_bundle(&config, &config_home);
    let model = config.model().unwrap_or("gpt-5.4-mini").to_string();
    let registry = build_registry(
        &cwd,
        &config_home,
        &config,
        &model,
        plugin_tools.clone(),
        &mcp_bundle,
    )
    .unwrap_or_else(|_| GlobalToolRegistry::builtin());
    let args = env::args().skip(1).collect::<Vec<_>>();
    let permission_mode = config
        .permission_mode()
        .unwrap_or(PermissionMode::WorkspaceWrite);
    let runtime = RuntimeHost {
        cwd: &cwd,
        config: &config,
        config_home: &config_home,
        permission_mode,
        plugin_tools: &plugin_tools,
        plugin_hooks: &plugin_hooks,
        mcp_bundle: &mcp_bundle,
    };

    match args.first().map(String::as_str) {
        None | Some("help") | Some("--help") | Some("-h") => {
            println!("{}", render_root_help(&registry, &config_home));
        }
        Some("commands") => println!("{}", render_help()),
        Some("plugins") => {
            for plugin in plugin_manager.list()? {
                println!(
                    "{} {} [{}]",
                    plugin.name,
                    plugin.version,
                    if plugin.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    }
                );
            }
        }
        Some("skills") => {
            let roots = skill_roots(&cwd, &config_home);
            let catalog = SkillCatalog::discover(&roots);
            println!("Skills ({})", catalog.skills().len());
            for skill in catalog.skills() {
                let origin = match skill.origin {
                    opencowork_skills::SkillOrigin::SkillsDir => "skills",
                    opencowork_skills::SkillOrigin::LegacyCommandsDir => "commands",
                };
                let execution_context = match skill.execution_context {
                    Some(opencowork_skills::SkillExecutionContext::Current) => "current",
                    Some(opencowork_skills::SkillExecutionContext::Fork) => "fork",
                    None => "-",
                };
                println!(
                    "{} | {} | context={} | {}",
                    skill.name,
                    origin,
                    execution_context,
                    skill.path.display()
                );
                println!(
                    "  description {}",
                    skill.description.as_deref().unwrap_or("no description")
                );
                if let Some(when_to_use) = &skill.when_to_use {
                    println!("  when        {}", when_to_use);
                }
                if let Some(argument_hint) = &skill.argument_hint {
                    println!("  args        {}", argument_hint);
                }
                if !skill.allowed_tools.is_empty() {
                    println!("  tools       {}", skill.allowed_tools.join(", "));
                }
                if !skill.paths.is_empty() {
                    println!("  paths       {}", skill.paths.join(", "));
                }
                if let Some(agent) = &skill.agent {
                    println!("  agent       {}", agent);
                }
                if let Some(model) = &skill.model {
                    println!("  model       {}", model);
                }
                if let Some(effort) = &skill.effort {
                    println!("  effort      {}", effort);
                }
            }
        }
        Some("provider") => {
            let profile = resolve_provider_profile(&config, &model);
            let thresholds = calculate_context_thresholds(
                &model,
                config.context().context_window_tokens_override(),
                config.context().max_output_tokens(),
                config.context().auto_compact_buffer_tokens(),
                config.context().warning_buffer_tokens(),
                config.context().error_buffer_tokens(),
                config.context().manual_compact_buffer_tokens(),
            );
            println!("Provider");
            println!("  Model            {}", model);
            println!("  Name             {}", profile.provider_name);
            println!("  API key env      {}", profile.api_key_env);
            println!("  Base URL         {}", profile.resolved_base_url());
            println!(
                "  Base URL env     {}",
                profile.base_url_env.as_deref().unwrap_or("(none)")
            );
            println!("  Timeout          {}ms", profile.timeout_ms);
            println!("  Streaming        enabled");
            println!("  Context window   {}", thresholds.context_window);
            println!("  Effective window {}", thresholds.effective_context_window);
            println!("  Auto compact     {}", thresholds.auto_compact_threshold);
            println!("  Warning          {}", thresholds.warning_threshold);
            println!("  Blocking limit   {}", thresholds.blocking_limit);
        }
        Some("team-memory") => {
            handle_team_memory(&args)?;
        }
        Some("mcp") => {
            handle_mcp(&args, &config_home, &config, &mcp_bundle)?;
        }
        Some("agents") => {
            let planner = TeamPlanner::new(SchedulingPolicy {
                max_parallel_agents: 3,
            });
            for assignment in planner.assign(&default_agents(), &default_tasks()) {
                println!(
                    "{} -> {} | {}",
                    assignment.task_id, assignment.agent_id, assignment.reason
                );
            }
        }
        Some("handoffs") => {
            handle_handoffs(&args, &runtime)?;
        }
        Some("sessions") => {
            let store = SessionStore::new(config_home.join("sessions"));
            let sessions = store.list()?;
            println!("Sessions ({})", sessions.len());
            for session in sessions {
                let collapsed = store
                    .load(&session.id)
                    .ok()
                    .and_then(|loaded| loaded.context_collapse_snapshot)
                    .map(|snapshot| {
                        format!(
                            " | collapsed_spans={} | collapsed_messages={}",
                            snapshot.collapsed_spans, snapshot.collapsed_messages
                        )
                    })
                    .unwrap_or_default();
                println!(
                    "{} | messages={}{} | {}",
                    session.id,
                    session.message_count,
                    collapsed,
                    session.path.display()
                );
            }
        }
        Some("prompt") => {
            let user_input = join_args(&args[1..])?;
            let app = AppRuntime::load(&cwd)?;
            let execution = run_app_turn(&app, &model, Session::new(), &user_input, true)?;
            let store = SessionStore::new(config_home.join("sessions"));
            let descriptor = store.save(&execution.session)?;
            print_turn_report("Prompt", Some(&descriptor.id), &execution);
        }
        Some("resume") => {
            let session_id = args
                .get(1)
                .ok_or_else(|| "missing session id for resume".to_string())?;
            let user_input = join_args(&args[2..])?;
            let store = SessionStore::new(config_home.join("sessions"));
            let session = store.load(session_id)?;
            let app = AppRuntime::load(&cwd)?;
            let execution = run_app_turn(&app, &model, session, &user_input, true)?;
            let descriptor = store.save_named(session_id, &execution.session)?;
            print_turn_report("Resume", Some(&descriptor.id), &execution);
        }
        Some("prompt-plan") => {
            let store = SessionStore::new(config_home.join("sessions"));
            let session = latest_session(&store)?;
            let app = AppRuntime::load(&cwd)?;
            let bundle = app.prepare_prompt_plan(&model, &session)?;
            println!("Prompt plan");
            println!("  Layers            {}", bundle.layers.len());
            println!("  Estimated tokens  {}", bundle.estimated_tokens);
            println!("  Compacted         {}", bundle.compacted);
            for layer in bundle.layers {
                println!("\n[{}]\n{}", layer.name, layer.content);
            }
        }
        Some("slash") => {
            let raw = args.get(1).map_or("", String::as_str);
            let session = Session::new();
            let command =
                SlashCommand::parse(raw).unwrap_or(SlashCommand::Unknown(raw.to_string()));
            if let Some(response) = handle_command(&command, &session, permission_mode) {
                println!("{response}");
            } else {
                println!("Unknown slash command: {raw}");
            }
        }
        Some("tool-manifest") => {
            let diagnostics = registry.tool_search_diagnostics();
            println!(
                "ToolSearch | mode={} | deferred_inline={} | deferred_chars={} | auto_threshold_chars={}",
                match diagnostics.mode {
                    opencowork_tools::ToolSearchMode::DeferredAlways => "tst",
                    opencowork_tools::ToolSearchMode::DeferredAuto => "tst-auto",
                    opencowork_tools::ToolSearchMode::Standard => "standard",
                },
                diagnostics.deferred_tools_inline,
                diagnostics.deferred_description_chars,
                diagnostics.auto_threshold_chars,
            );
            for (name, description, permission, _) in registry.definitions() {
                println!("{name} | {:?} | {description}", permission);
            }
        }
        Some(other) => {
            println!(
                "Unknown command: {other}\n\n{}",
                render_root_help(&registry, &config_home)
            );
        }
    }

    stop_team_memory_sync()?;
    Ok(())
}

fn handle_team_memory(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    match args.get(1).map(String::as_str) {
        None | Some("status") => {
            let status = team_memory_sync_status();
            println!("Team memory sync");
            println!("  Running          {}", status.running);
            println!(
                "  Repo             {}",
                status.repo_slug.as_deref().unwrap_or("(none)")
            );
            println!(
                "  Endpoint         {}",
                status.endpoint.as_deref().unwrap_or("(none)")
            );
            println!("  Pending changes  {}", status.pending_changes);
            println!(
                "  Last checksum    {}",
                status.last_known_checksum.as_deref().unwrap_or("(none)")
            );
            println!("  Files pulled     {}", status.files_pulled);
            println!("  Files pushed     {}", status.files_pushed);
            println!(
                "  Last pull        {}",
                status
                    .last_pull_unix_ms
                    .map_or_else(|| "(none)".to_string(), |value| value.to_string())
            );
            println!(
                "  Last push        {}",
                status
                    .last_push_unix_ms
                    .map_or_else(|| "(none)".to_string(), |value| value.to_string())
            );
            println!(
                "  Last error       {}",
                status.last_error.as_deref().unwrap_or("(none)")
            );
        }
        Some("pull") => {
            force_pull_team_memory()?;
            println!("team memory pull scheduled");
        }
        Some("push") => {
            force_push_team_memory()?;
            println!("team memory push scheduled");
        }
        Some(other) => {
            return Err(format!("unknown team-memory subcommand `{other}`").into());
        }
    }
    Ok(())
}

fn handle_handoffs(
    args: &[String],
    runtime: &RuntimeHost<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = AgentHandoffStore::new(runtime.config_home.join("handoffs"));
    let planner = TeamPlanner::new(SchedulingPolicy {
        max_parallel_agents: 3,
    });
    let assignments = planner.assign(&default_agents(), &default_tasks());

    match args.get(1).map(String::as_str) {
        Some("create") => {
            for assignment in &assignments {
                let agent_model = default_agents()
                    .into_iter()
                    .find(|agent| agent.id == assignment.agent_id)
                    .map(|agent| agent.model);
                let handoff = store.create(
                    assignment,
                    agent_model,
                    format!("Continue task `{}` for OpenCoWork", assignment.task_id),
                    vec![
                        "crates/runtime/src/conversation.rs".to_string(),
                        "crates/runtime/src/prompt.rs".to_string(),
                        "crates/tools/src/lib.rs".to_string(),
                    ],
                )?;
                println!(
                    "created {} -> {} ({})",
                    handoff.task_id, handoff.agent_id, handoff.handoff_id
                );
            }
        }
        Some("consume") => {
            let agent_id = args
                .get(2)
                .ok_or_else(|| "missing agent id for handoffs consume".to_string())?;
            if !consume_one_handoff(runtime, agent_id)? {
                println!("No pending handoffs for {}", agent_id);
            }
        }
        Some("worker") => {
            let agent_id = args
                .get(2)
                .ok_or_else(|| "missing agent id for handoffs worker".to_string())?;
            let poll_ms = parse_u64_flag(args, "--poll-ms").unwrap_or(1500);
            let max_jobs = parse_usize_flag(args, "--max-jobs").unwrap_or(0);
            let processed = run_handoff_worker(runtime, agent_id, poll_ms, max_jobs)?;
            println!("Worker finished for {} | processed={}", agent_id, processed);
        }
        Some("service") => {
            handle_handoff_service(args, runtime)?;
        }
        _ => {
            let handoffs = store.list()?;
            println!("Handoffs ({})", handoffs.len());
            for handoff in handoffs {
                println!(
                    "{} | {} | {:?} | {}",
                    handoff.handoff_id, handoff.agent_id, handoff.status, handoff.summary
                );
            }
        }
    }

    Ok(())
}

fn consume_one_handoff(
    runtime: &RuntimeHost<'_>,
    agent_id: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let store = AgentHandoffStore::new(runtime.config_home.join("handoffs"));
    let Some(mut handoff) = store.claim_next(agent_id)? else {
        return Ok(false);
    };
    let session_id = format!("agent-{}", handoff.handoff_id);
    handoff = store.mark_running(&handoff.handoff_id, Some(session_id.clone()))?;
    let session_store = SessionStore::new(runtime.config_home.join("sessions"));
    let starting_session = load_session_or_new(&session_store, &session_id);
    let model = handoff
        .agent_model
        .clone()
        .unwrap_or_else(|| resolve_agent_model(agent_id));

    match run_live_turn(
        runtime,
        &model,
        starting_session,
        &render_handoff_prompt(&handoff),
        true,
    ) {
        Ok(execution) => {
            session_store.save_named(&session_id, &execution.session)?;
            let result = assistant_text(&execution.summary)
                .map(|text| preview_text(text, 200))
                .unwrap_or_else(|| "No assistant text produced.".to_string());
            let completed =
                store.mark_completed(&handoff.handoff_id, Some(session_id.clone()), result)?;
            println!(
                "Completed {} for {} | session={} | status={:?}",
                completed.handoff_id, completed.agent_id, session_id, completed.status
            );
            if let Some(text) = assistant_text(&execution.summary) {
                println!("\n{}", text);
            }
            Ok(true)
        }
        Err(error) => {
            let _ = store.mark_failed(&handoff.handoff_id, Some(session_id), error.to_string());
            Err(error)
        }
    }
}

fn run_handoff_worker(
    runtime: &RuntimeHost<'_>,
    agent_id: &str,
    poll_ms: u64,
    max_jobs: usize,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut processed = 0;
    loop {
        if max_jobs > 0 && processed >= max_jobs {
            break;
        }

        if consume_one_handoff(runtime, agent_id)? {
            processed += 1;
            continue;
        }

        if max_jobs > 0 {
            break;
        }

        println!("Worker idle for {} | poll={}ms", agent_id, poll_ms);
        thread::sleep(Duration::from_millis(poll_ms));
    }

    Ok(processed)
}

fn handle_handoff_service(
    args: &[String],
    runtime: &RuntimeHost<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.get(2).map(String::as_str) {
        Some("start") => {
            let agent_id = args
                .get(3)
                .ok_or_else(|| "missing agent id for handoffs service start".to_string())?;
            let poll_ms = parse_u64_flag(args, "--poll-ms").unwrap_or(1500);
            let heartbeat_ms = parse_u64_flag(args, "--heartbeat-ms").unwrap_or(5000);
            let state =
                start_handoff_service(runtime.config_home, agent_id, poll_ms, heartbeat_ms)?;
            println!(
                "Worker service started for {} | pid={} | log={}",
                state.agent_id, state.pid, state.log_path
            );
            Ok(())
        }
        Some("run") => {
            let agent_id = args
                .get(3)
                .ok_or_else(|| "missing agent id for handoffs service run".to_string())?;
            let poll_ms = parse_u64_flag(args, "--poll-ms").unwrap_or(1500);
            let heartbeat_ms = parse_u64_flag(args, "--heartbeat-ms").unwrap_or(5000);
            run_handoff_service(runtime, agent_id, poll_ms, heartbeat_ms)
        }
        Some("status") => {
            print_handoff_service_status(runtime.config_home, args.get(3).map(String::as_str))?;
            Ok(())
        }
        Some("stop") => {
            let agent_id = args
                .get(3)
                .ok_or_else(|| "missing agent id for handoffs service stop".to_string())?;
            request_handoff_service_stop(runtime.config_home, agent_id)?;
            println!("stop requested for {}", agent_id);
            Ok(())
        }
        other => Err(format!(
            "unknown handoffs service subcommand `{}`",
            other.unwrap_or("(none)")
        )
        .into()),
    }
}

fn start_handoff_service(
    config_home: &Path,
    agent_id: &str,
    poll_ms: u64,
    heartbeat_ms: u64,
) -> Result<WorkerServiceState, Box<dyn std::error::Error>> {
    let state_path = handoff_service_state_path(config_home, agent_id);
    if let Ok(state) = load_worker_service_state(&state_path) {
        if matches!(
            state.status,
            WorkerServiceStatus::Starting | WorkerServiceStatus::Running
        ) && !is_service_state_stale(&state, heartbeat_ms.saturating_mul(3))
        {
            return Err(format!("worker service for `{agent_id}` is already running").into());
        }
    }

    fs::create_dir_all(handoff_service_root(config_home))?;
    let log_path = handoff_service_log_path(config_home, agent_id);
    let stdout = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let stderr = stdout.try_clone()?;
    let current_exe = env::current_exe()?;
    let mut command = Command::new(current_exe);
    command
        .args([
            "handoffs",
            "service",
            "run",
            agent_id,
            "--poll-ms",
            &poll_ms.to_string(),
            "--heartbeat-ms",
            &heartbeat_ms.to_string(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let child = command.spawn()?;
    let state = WorkerServiceState {
        agent_id: agent_id.to_string(),
        pid: child.id(),
        poll_ms,
        heartbeat_ms,
        processed_jobs: 0,
        status: WorkerServiceStatus::Starting,
        stop_requested: false,
        started_at_unix_ms: now_ms(),
        last_heartbeat_unix_ms: now_ms(),
        stopped_at_unix_ms: None,
        last_error: None,
        log_path: log_path.display().to_string(),
    };
    save_worker_service_state(&state_path, &state)?;
    Ok(state)
}

fn run_handoff_service(
    runtime: &RuntimeHost<'_>,
    agent_id: &str,
    poll_ms: u64,
    heartbeat_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let state_path = handoff_service_state_path(runtime.config_home, agent_id);
    let log_path = handoff_service_log_path(runtime.config_home, agent_id);
    let pid = std::process::id();
    let mut state = WorkerServiceState {
        agent_id: agent_id.to_string(),
        pid,
        poll_ms,
        heartbeat_ms,
        processed_jobs: 0,
        status: WorkerServiceStatus::Running,
        stop_requested: false,
        started_at_unix_ms: now_ms(),
        last_heartbeat_unix_ms: now_ms(),
        stopped_at_unix_ms: None,
        last_error: None,
        log_path: log_path.display().to_string(),
    };
    save_worker_service_state(&state_path, &state)?;
    loop {
        let latest = load_worker_service_state(&state_path).unwrap_or_else(|_| state.clone());
        if latest.stop_requested {
            state.stop_requested = true;
            state.status = WorkerServiceStatus::Stopped;
            state.stopped_at_unix_ms = Some(now_ms());
            save_worker_service_state(&state_path, &state)?;
            break;
        }

        state.status = WorkerServiceStatus::Running;
        let now = now_ms();
        if heartbeat_ms == 0
            || now.saturating_sub(state.last_heartbeat_unix_ms) >= u128::from(heartbeat_ms)
        {
            state.last_heartbeat_unix_ms = now;
            save_worker_service_state(&state_path, &state)?;
        }

        match consume_one_handoff(runtime, agent_id) {
            Ok(true) => {
                state.processed_jobs += 1;
                state.last_error = None;
                state.last_heartbeat_unix_ms = now_ms();
                save_worker_service_state(&state_path, &state)?;
            }
            Ok(false) => {
                thread::sleep(Duration::from_millis(poll_ms));
            }
            Err(error) => {
                state.status = WorkerServiceStatus::Failed;
                state.last_error = Some(error.to_string());
                state.stopped_at_unix_ms = Some(now_ms());
                save_worker_service_state(&state_path, &state)?;
                return Err(error);
            }
        }
    }
    Ok(())
}

fn print_handoff_service_status(
    config_home: &Path,
    agent_filter: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = handoff_service_root(config_home);
    if !root.exists() {
        println!("Worker services (0)");
        return Ok(());
    }
    let mut states = Vec::new();
    for entry in fs::read_dir(&root)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let state = load_worker_service_state(&path)?;
        if agent_filter.is_none_or(|agent| state.agent_id == agent) {
            states.push(state);
        }
    }
    states.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
    println!("Worker services ({})", states.len());
    for state in states {
        let stale = is_service_state_stale(&state, state.heartbeat_ms.saturating_mul(3));
        println!(
            "{} | pid={} | {:?}{} | processed={} | log={}",
            state.agent_id,
            state.pid,
            state.status,
            if stale { " | stale" } else { "" },
            state.processed_jobs,
            state.log_path
        );
        if let Some(error) = &state.last_error {
            println!("  error {}", error);
        }
    }
    Ok(())
}

fn request_handoff_service_stop(
    config_home: &Path,
    agent_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = handoff_service_state_path(config_home, agent_id);
    let mut state = load_worker_service_state(&path)?;
    state.stop_requested = true;
    save_worker_service_state(&path, &state)?;
    Ok(())
}

fn handoff_service_root(config_home: &Path) -> PathBuf {
    config_home.join("worker-service")
}

fn handoff_service_state_path(config_home: &Path, agent_id: &str) -> PathBuf {
    handoff_service_root(config_home).join(format!("{agent_id}.json"))
}

fn handoff_service_log_path(config_home: &Path, agent_id: &str) -> PathBuf {
    handoff_service_root(config_home).join(format!("{agent_id}.log"))
}

fn load_worker_service_state(
    path: &Path,
) -> Result<WorkerServiceState, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn save_worker_service_state(
    path: &Path,
    state: &WorkerServiceState,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(state)?)?;
    Ok(())
}

fn is_service_state_stale(state: &WorkerServiceState, stale_after_ms: u64) -> bool {
    if matches!(
        state.status,
        WorkerServiceStatus::Stopped | WorkerServiceStatus::Failed
    ) {
        return false;
    }
    now_ms().saturating_sub(state.last_heartbeat_unix_ms) > u128::from(stale_after_ms)
}

fn resolve_mcp_bundle(config: &RuntimeConfig, config_home: &Path) -> McpRuntimeBundle {
    let catalog = McpCatalog::new(config.mcp_servers().clone());
    let configured = catalog.tool_definitions();
    let mut warnings = Vec::new();
    let tools = if catalog.servers().is_empty() {
        configured
    } else {
        let executor = TransportMcpExecutor::with_credential_store(
            catalog.clone(),
            Some(mcp_auth_root(config_home)),
        );
        match executor.discover_tools() {
            Ok(discovered) => merge_tool_definitions(configured, discovered),
            Err(error) => {
                warnings.push(error);
                configured
            }
        }
    };

    McpRuntimeBundle {
        catalog,
        tools,
        warnings,
    }
}

fn build_registry(
    cwd: &Path,
    config_home: &Path,
    config: &RuntimeConfig,
    model: &str,
    plugin_tools: Vec<PluginTool>,
    mcp_bundle: &McpRuntimeBundle,
) -> Result<GlobalToolRegistry, String> {
    let skill_catalog = SkillCatalog::discover(&skill_roots(cwd, config_home));
    GlobalToolRegistry::with_extensions(
        plugin_tools,
        mcp_bundle.tools.clone(),
        Some(Box::new(TransportMcpExecutor::with_credential_store(
            mcp_bundle.catalog.clone(),
            Some(mcp_auth_root(config_home)),
        ))),
        skill_catalog,
        lsp_servers_from_runtime(cwd, config.lsp().servers().values().cloned()),
    )
    .map(|registry| {
        registry.with_tool_search_settings(ToolSearchSettings::from_env(
            model,
            config.context().context_window_tokens_override(),
        ))
    })
}

fn mcp_auth_root(config_home: &Path) -> PathBuf {
    config_home.join("mcp-auth")
}

fn mcp_executor(config_home: &Path, catalog: &McpCatalog) -> TransportMcpExecutor {
    TransportMcpExecutor::with_credential_store(catalog.clone(), Some(mcp_auth_root(config_home)))
}

fn handle_mcp(
    args: &[String],
    config_home: &Path,
    config: &RuntimeConfig,
    mcp_bundle: &McpRuntimeBundle,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.get(1).map(String::as_str) {
        None => print_mcp_summary(config_home, config, mcp_bundle),
        Some("resources") => {
            let executor = mcp_executor(config_home, &mcp_bundle.catalog);
            let resources = executor.discover_resources()?;
            println!("MCP resources ({})", resources.len());
            for resource in resources {
                println!(
                    "{} | {} | {} | {}",
                    resource.server_name,
                    resource.name,
                    resource.uri,
                    resource.mime_type.as_deref().unwrap_or("text/plain")
                );
            }
            Ok(())
        }
        Some("resource") => {
            if args.get(2).map(String::as_str) != Some("read") {
                return Err("usage: mcp resource read <server> <uri>".into());
            }
            let server_name = args
                .get(3)
                .ok_or_else(|| "missing server name for mcp resource read".to_string())?;
            let uri = args
                .get(4)
                .ok_or_else(|| "missing uri for mcp resource read".to_string())?;
            let executor = mcp_executor(config_home, &mcp_bundle.catalog);
            let contents = executor.read_resource(server_name, uri)?;
            println!("Resource {}", uri);
            for content in contents {
                println!(
                    "  {} | {}",
                    content.uri,
                    content.mime_type.as_deref().unwrap_or("text/plain")
                );
                if let Some(text) = content.text {
                    println!("{}", text);
                } else if let Some(blob) = content.blob {
                    println!("{}", preview_text(&blob, 400));
                }
            }
            Ok(())
        }
        Some("auth") => handle_mcp_auth(args, config_home, config),
        Some(other) => Err(format!("unknown mcp subcommand `{other}`").into()),
    }
}

fn print_mcp_summary(
    config_home: &Path,
    config: &RuntimeConfig,
    mcp_bundle: &McpRuntimeBundle,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("MCP servers ({})", config.mcp_servers().len());
    if !mcp_bundle.warnings.is_empty() {
        for warning in &mcp_bundle.warnings {
            println!("  warning {warning}");
        }
    }
    let credential_store = McpCredentialStore::new(mcp_auth_root(config_home));
    for (name, server) in config.mcp_servers() {
        let tool_count = mcp_bundle
            .tools
            .iter()
            .filter(|tool| tool.server_name == *name)
            .count();
        println!(
            "{} | {:?} | {} | tools={} | auth={} | timeout={}ms",
            name,
            server.transport,
            server
                .endpoint
                .as_deref()
                .or(server.command.as_deref())
                .unwrap_or("(none)"),
            tool_count,
            server.auth.label(),
            server.timeout_ms.unwrap_or(30_000)
        );
        println!("  prefix {}", mcp_tool_prefix(name));
        println!(
            "  credentials {}",
            render_mcp_auth_presence(&credential_store, name, &server.auth)?
        );
        for tool in mcp_bundle
            .tools
            .iter()
            .filter(|tool| tool.server_name == *name)
        {
            println!("  tool   {}", mcp_tool_name(name, &tool.tool_name));
        }
    }
    Ok(())
}

fn handle_mcp_auth(
    args: &[String],
    config_home: &Path,
    config: &RuntimeConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = McpCredentialStore::new(mcp_auth_root(config_home));
    match args.get(2).map(String::as_str) {
        None => {
            println!("MCP auth");
            for (name, server) in config.mcp_servers() {
                println!(
                    "{} | {} | {}",
                    name,
                    server.auth.label(),
                    render_mcp_auth_presence(&store, name, &server.auth)?
                );
            }
            Ok(())
        }
        Some("save") => {
            let server_name = args
                .get(3)
                .ok_or_else(|| "missing server name for mcp auth save".to_string())?;
            let token = args
                .get(4)
                .ok_or_else(|| "missing access token for mcp auth save".to_string())?;
            store.save(
                server_name,
                &McpCredentialRecord {
                    access_token: token.to_string(),
                    refresh_token: None,
                    expires_at_unix_ms: None,
                    scopes: Vec::new(),
                },
            )?;
            println!("saved auth for {}", server_name);
            Ok(())
        }
        Some("clear") => {
            let server_name = args
                .get(3)
                .ok_or_else(|| "missing server name for mcp auth clear".to_string())?;
            store.clear(server_name)?;
            println!("cleared auth for {}", server_name);
            Ok(())
        }
        Some(other) => Err(format!("unknown mcp auth subcommand `{other}`").into()),
    }
}

fn render_mcp_auth_presence(
    store: &McpCredentialStore,
    server_name: &str,
    auth: &McpAuthConfig,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(match auth {
        McpAuthConfig::None => "not-required".to_string(),
        McpAuthConfig::BearerEnv { token_env } => {
            if env::var(token_env)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .is_some()
            {
                format!("ready via env:{token_env}")
            } else {
                format!("missing env:{token_env}")
            }
        }
        McpAuthConfig::BearerFile { token_path } => {
            if Path::new(token_path).exists() {
                format!("ready via file:{token_path}")
            } else {
                format!("missing file:{token_path}")
            }
        }
        McpAuthConfig::OAuth { .. } => {
            if store.load(server_name)?.is_some() {
                "ready via local oauth token".to_string()
            } else {
                "missing local oauth token".to_string()
            }
        }
    })
}

fn build_hook_runner(config: &RuntimeConfig, plugin_hooks: &PluginHooks) -> HookRunner {
    let mut hooks = config.hooks().clone();
    hooks.extend_command_strings(
        RuntimeHookEvent::PreToolUse,
        plugin_hooks.pre_tool_use.clone(),
    );
    hooks.extend_command_strings(
        RuntimeHookEvent::PostToolUse,
        plugin_hooks.post_tool_use.clone(),
    );
    HookRunner::from_hook_config(&hooks)
}

fn build_runtime_registry(runtime: &RuntimeHost<'_>) -> Result<GlobalToolRegistry, String> {
    build_registry(
        runtime.cwd,
        runtime.config_home,
        runtime.config,
        runtime.config.model().unwrap_or("gpt-5.4-mini"),
        runtime.plugin_tools.to_vec(),
        runtime.mcp_bundle,
    )
}

fn build_runtime_hooks(runtime: &RuntimeHost<'_>) -> HookRunner {
    build_hook_runner(runtime.config, runtime.plugin_hooks)
}

fn resolve_provider_profile(config: &RuntimeConfig, model: &str) -> OpenAiCompatProfile {
    if let Some(provider) = config.provider() {
        return OpenAiCompatProfile::custom(
            provider.name(),
            provider.api_key_env(),
            provider.base_url(),
            provider.base_url_env().map(ToOwned::to_owned),
        )
        .with_timeout_ms(provider.timeout_ms());
    }
    default_openai_profile_for_model(model)
}

fn schedule_provider_backed_session_memory_refresh(
    config: &RuntimeConfig,
    model: &str,
    session: &mut Session,
    cwd: &Path,
    config_home: &Path,
) -> Result<bool, std::io::Error> {
    let profile = resolve_provider_profile(config, model);
    let model = model.to_string();
    maybe_schedule_session_memory_refresh_with_task(
        session,
        cwd,
        config_home,
        move |mut background_session, cwd, config_home| match OpenAiCompatClient::from_profile(
            profile.clone(),
        ) {
            Ok(mut provider) => {
                if refresh_current_session_memory_with_provider(
                    &mut background_session,
                    &cwd,
                    &config_home,
                    &mut provider,
                    &model,
                )
                .is_err()
                {
                    let _ =
                        refresh_current_session_memory(&mut background_session, &cwd, &config_home);
                }
            }
            Err(_) => {
                let _ = refresh_current_session_memory(&mut background_session, &cwd, &config_home);
            }
        },
    )
}

fn run_live_turn(
    host: &RuntimeHost<'_>,
    model: &str,
    session: Session,
    user_input: &str,
    emit_live_output: bool,
) -> Result<TurnExecution, Box<dyn std::error::Error>> {
    let profile = resolve_provider_profile(host.config, model);
    let provider = OpenAiCompatClient::from_profile(profile)?;
    let prepared = prepare_turn_context(
        host.cwd,
        host.config_home,
        host.config,
        host.permission_mode,
        model,
        &session,
        Some(user_input),
    )?;
    let mut hooks = build_runtime_hooks(host);
    let instruction_files = prepared
        .prompt
        .layers
        .iter()
        .filter_map(|layer| layer.name.strip_prefix("instruction:"))
        .filter(|label| should_report_instruction_file(label, host.cwd, host.config_home, &session))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let instructions_loaded = hooks.run_instructions_loaded(&instruction_files);
    if instructions_loaded.is_denied() {
        return Err(io::Error::other(format!(
            "instructions-loaded hook denied execution: {}",
            instructions_loaded.messages().join(" | ")
        ))
        .into());
    }
    let touched_files = touched_paths_from_session(host.cwd, &prepared.session)
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    if !touched_files.is_empty() {
        let file_changed = hooks.run_file_changed(&touched_files);
        if file_changed.is_denied() {
            return Err(io::Error::other(format!(
                "file-changed hook denied execution: {}",
                file_changed.messages().join(" | ")
            ))
            .into());
        }
    }
    let mut conversation = ConversationRuntime::new(
        prepared.session,
        ProviderBackedApiClient::new(provider, model),
        build_runtime_registry(host).map_err(io::Error::other)?,
        PermissionPolicy::new(host.permission_mode),
        prepared.prompt.system_prompt.clone(),
        hooks,
    )
    .with_tool_result_budget(tool_result_budget_config(
        host.config,
        host.config_home.join("tool-results"),
    ))
    .with_prompt_augmenter(Box::new(ConditionalSkillPromptAugmenter::new(
        host.cwd,
        host.config_home,
        &session,
    )));
    let summary = if emit_live_output {
        let mut stdout = io::stdout();
        let mut renderer = CliTurnRenderer::new(&mut stdout);
        let result = conversation.run_turn_with_observer(user_input, None, Some(&mut renderer));
        renderer.finish()?;
        result?
    } else {
        conversation.run_turn(user_input, None)?
    };
    let mut updated_session = conversation.session().clone();
    let _ = schedule_provider_backed_session_memory_refresh(
        host.config,
        model,
        &mut updated_session,
        host.cwd,
        host.config_home,
    )?;
    Ok(TurnExecution {
        session: updated_session,
        summary,
        prompt: prepared.prompt,
    })
}

fn run_app_turn(
    app: &AppRuntime,
    model: &str,
    session: Session,
    user_input: &str,
    emit_live_output: bool,
) -> Result<TurnExecution, Box<dyn std::error::Error>> {
    let execution = if emit_live_output {
        let mut stdout = io::stdout();
        let mut renderer = CliTurnRenderer::new(&mut stdout);
        let mut sink = |event| renderer.on_app_event(&event);
        let execution = app.run_turn(model, session, user_input, Some(&mut sink))?;
        renderer.finish()?;
        execution
    } else {
        app.run_turn(model, session, user_input, None)?
    };

    Ok(TurnExecution {
        session: execution.session,
        summary: execution.summary,
        prompt: execution.prompt,
    })
}

fn prepare_turn_context(
    cwd: &Path,
    config_home: &Path,
    config: &RuntimeConfig,
    permission_mode: PermissionMode,
    model: &str,
    session: &Session,
    pending_user_input: Option<&str>,
) -> Result<PreparedTurnContext, Box<dyn std::error::Error>> {
    let mut hydrated_session = session.clone();
    wait_for_session_memory_refresh(&mut hydrated_session, cwd, config_home)?;
    let hydrated_session = hydrate_current_session_memory(&hydrated_session, cwd, config_home)?;
    let instructions = collect_instruction_sources(
        cwd,
        config_home,
        config,
        &hydrated_session,
        model,
        pending_user_input,
    )?;
    let optimizer = ContextOptimizer::new(context_optimizer_config(config, model));
    let preparation = optimizer.prepare_session(&hydrated_session);
    let selection = optimizer.select(&hydrated_session, &instructions, &preparation);
    let composer = PromptComposer;
    let compact_result =
        preparation
            .summary
            .as_ref()
            .map(|summary| opencowork_runtime::CompactResult {
                summary: summary.clone(),
                formatted_summary: summary.clone(),
                compacted_session: preparation.session.clone(),
                removed_message_count: usize::from(preparation.compacted),
            });
    let prompt = composer.compose(
        &ProjectContext {
            cwd: cwd.to_path_buf(),
            current_date: "2026-04-02".to_string(),
            model: Some(model.to_string()),
            permission_mode,
            instruction_files: instructions
                .iter()
                .filter(|instruction| !instruction.label.starts_with("skill:"))
                .map(|instruction| PathBuf::from(&instruction.label))
                .collect(),
        },
        &selection.instructions,
        compact_result.as_ref(),
        if compact_result.is_some() {
            None
        } else {
            selection.summary.as_deref()
        },
    );

    Ok(PreparedTurnContext {
        session: preparation.session,
        prompt,
    })
}

fn render_root_help(registry: &GlobalToolRegistry, config_home: &Path) -> String {
    format!(
        "OpenCoWork CLI\n\nCommands\n  help               Show this help\n  commands           Show slash commands\n  plugins            List installed plugins\n  skills             Discover available skills\n  provider           Show resolved model/provider settings\n  team-memory [status|pull|push]    Inspect or drive team memory sync\n  mcp                List configured MCP servers, tools, auth, and timeout state\n  mcp resources      List discovered MCP resources\n  mcp resource read <server> <uri>  Read an MCP resource\n  mcp auth [save|clear] ...         Inspect or manage saved MCP auth tokens\n  agents             Show default multi-agent assignment plan\n  handoffs [create|consume <agent>|worker <agent>]  Manage persisted agent handoffs\n  handoffs service [start|run|status|stop] ...      Manage persistent worker services\n  sessions           List saved local sessions\n  prompt <text>      Run a live provider-backed turn and save a session\n  resume <id> <text> Continue a saved session\n  prompt-plan        Render the composed system prompt plan\n  slash <command>    Run a local slash command helper\n  tool-manifest      List builtin, plugin, and MCP tools\n\nConfig home         {}\nAvailable tools     {}",
        config_home.display(),
        registry.definitions().len()
    )
}

fn collect_instruction_sources(
    cwd: &Path,
    config_home: &Path,
    config: &RuntimeConfig,
    session: &Session,
    model: &str,
    pending_user_input: Option<&str>,
) -> Result<Vec<InstructionSource>, std::io::Error> {
    let mut sources = discover_instruction_sources(
        cwd,
        config.context().instruction_files(),
        config.context().max_instruction_tokens(),
    )?;
    sources.extend(discover_nested_instruction_sources(
        cwd,
        &touched_paths_from_session(cwd, session),
        config.context().max_instruction_tokens(),
    )?);
    if let Some(source) =
        discover_project_memory_source(cwd, config_home, config.context().max_instruction_tokens())?
    {
        sources.push(source);
    }
    if let Some(source) =
        discover_team_memory_source(cwd, config_home, config.context().max_instruction_tokens())?
    {
        sources.push(source);
    }
    if let Some(source) = load_current_session_memory_source(
        session,
        cwd,
        config_home,
        config.context().max_instruction_tokens(),
    )? {
        sources.push(source);
    }
    sources.extend(discover_relevant_memory_sources(
        cwd,
        config_home,
        session,
        pending_user_input,
        config.context().max_instruction_tokens(),
    )?);
    if let Some(user_query) = pending_user_input.filter(|value| !value.trim().is_empty()) {
        if let Ok(mut provider) =
            OpenAiCompatClient::from_profile(resolve_provider_profile(config, model))
        {
            let provider_sources = discover_relevant_memory_sources_with_provider(
                cwd,
                config_home,
                session,
                Some(user_query),
                config.context().max_instruction_tokens(),
                &mut provider,
                model,
            )?;
            if !provider_sources.is_empty() {
                sources.retain(|source| !source.label.starts_with("relevant-memory:"));
                sources.extend(provider_sources);
            }
        }
    }
    if let Some(skill_listing) = available_skill_listing_source(cwd, config_home, config, model) {
        sources.push(skill_listing);
    }
    let explicit_skill_sources = skill_instruction_sources_from_session(session);
    let loaded_skills = explicit_skill_sources
        .iter()
        .filter_map(|source| source.label.strip_prefix("skill:"))
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();
    sources.extend(conditional_skill_instruction_sources(
        cwd,
        config_home,
        session,
        &loaded_skills,
    ));
    sources.extend(explicit_skill_sources);
    Ok(sources)
}

fn available_skill_listing_source(
    cwd: &Path,
    config_home: &Path,
    config: &RuntimeConfig,
    model: &str,
) -> Option<InstructionSource> {
    let catalog = SkillCatalog::discover(&skill_roots(cwd, config_home));
    let content = render_skill_listing(
        catalog.skills(),
        Some(get_context_window_for_model(
            model,
            config.context().context_window_tokens_override(),
        )),
    );
    (!content.is_empty()).then_some(InstructionSource {
        label: "available-skills".to_string(),
        content,
        priority: 5_000,
    })
}

fn conditional_skill_instruction_sources(
    cwd: &Path,
    config_home: &Path,
    session: &Session,
    loaded_skills: &BTreeSet<String>,
) -> Vec<InstructionSource> {
    let touched_paths = touched_paths_from_session(cwd, session);
    if touched_paths.is_empty() {
        return Vec::new();
    }

    let catalog = SkillCatalog::discover(&skill_roots(cwd, config_home));
    let mut sources = Vec::new();
    let mut seen = BTreeSet::new();
    for skill in catalog.conditional_matches(cwd, &touched_paths) {
        if loaded_skills.contains(&skill.name) || !seen.insert(skill.name.clone()) {
            continue;
        }
        let Ok(definition) = catalog.load(&skill.name) else {
            continue;
        };
        sources.push(InstructionSource {
            label: format!("conditional-skill:{}", definition.summary.name),
            content: render_conditional_skill_instruction(
                &definition,
                &matched_skill_paths(cwd, &touched_paths, &definition.summary.paths),
            ),
            priority: 25_000,
        });
    }

    sources
}

fn render_conditional_skill_instruction(
    definition: &SkillDefinition,
    matched_paths: &[String],
) -> String {
    let mut details = Vec::new();
    if let Some(description) = &definition.summary.description {
        details.push(format!("Description: {description}"));
    }
    if let Some(when_to_use) = &definition.summary.when_to_use {
        details.push(format!("When to use: {when_to_use}"));
    }
    if let Some(argument_hint) = &definition.summary.argument_hint {
        details.push(format!("Argument hint: {argument_hint}"));
    }
    if !definition.summary.allowed_tools.is_empty() {
        details.push(format!(
            "Allowed tools: {}",
            definition.summary.allowed_tools.join(", ")
        ));
    }
    if !definition.summary.paths.is_empty() {
        details.push(format!(
            "Activation paths: {}",
            definition.summary.paths.join(", ")
        ));
    }
    if let Some(agent) = &definition.summary.agent {
        details.push(format!("Suggested agent: {agent}"));
    }
    if let Some(model) = &definition.summary.model {
        details.push(format!("Preferred model: {model}"));
    }
    if let Some(effort) = &definition.summary.effort {
        details.push(format!("Suggested effort: {effort}"));
    }

    let mut content = format!(
        "Conditionally activated skill `{}` because file work matched this skill.",
        definition.summary.name
    );
    if !matched_paths.is_empty() {
        content.push_str("\nMatched files: ");
        content.push_str(&matched_paths.join(", "));
    }
    if !details.is_empty() {
        content.push('\n');
        content.push_str(&details.join("\n"));
    }
    content.push_str("\nApply these instructions when relevant.\n\n");
    content.push_str(&definition.content);
    content
}

fn matched_skill_paths(cwd: &Path, touched_paths: &[PathBuf], patterns: &[String]) -> Vec<String> {
    touched_paths
        .iter()
        .filter_map(|path| path.strip_prefix(cwd).ok())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .filter(|path| {
            patterns.iter().any(|pattern| {
                path == pattern
                    || path
                        .strip_prefix(pattern)
                        .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with('/'))
            })
        })
        .collect()
}

fn explicit_skill_prompt_updates(
    message: &opencowork_runtime::ConversationMessage,
    loaded_skills: &mut BTreeSet<String>,
) -> Vec<RuntimePromptUpdate> {
    let mut updates = Vec::new();
    for block in &message.blocks {
        let ContentBlock::ToolResult {
            tool_name,
            output,
            is_error,
            ..
        } = block
        else {
            continue;
        };
        if *is_error || tool_name != "Skill" {
            continue;
        }
        let Some(source) = skill_instruction_source_from_output(output, 0) else {
            continue;
        };
        let Some(skill_name) = source.label.strip_prefix("skill:") else {
            continue;
        };
        if !loaded_skills.insert(skill_name.to_string()) {
            continue;
        }
        updates.push(RuntimePromptUpdate {
            label: source.label,
            content: source.content,
        });
    }
    updates
}

fn lsp_context_prompt_updates(
    message: &opencowork_runtime::ConversationMessage,
) -> Vec<RuntimePromptUpdate> {
    let mut updates = Vec::new();
    for block in &message.blocks {
        let ContentBlock::ToolResult {
            tool_name,
            output,
            is_error,
            ..
        } = block
        else {
            continue;
        };
        if *is_error || tool_name != "LspContext" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(output) else {
            continue;
        };
        let Some(prompt_section) = value
            .get("prompt_section")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let label = value
            .get("path")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(|path| format!("lsp:{}", path.replace('\\', "/")))
            .unwrap_or_else(|| "lsp:context".to_string());
        updates.push(RuntimePromptUpdate {
            label,
            content: prompt_section.to_string(),
        });
    }
    updates
}

fn context_optimizer_config(config: &RuntimeConfig, model: &str) -> ContextOptimizerConfig {
    ContextOptimizerConfig {
        model: model.to_string(),
        preserve_recent_messages: config.context().preserve_recent_messages(),
        context_window_tokens_override: config.context().context_window_tokens_override(),
        max_instruction_tokens: config.context().max_instruction_tokens(),
        max_output_tokens: config.context().max_output_tokens(),
        auto_compact_buffer_tokens: config.context().auto_compact_buffer_tokens(),
        message_collapse_chars: config.context().message_collapse_chars(),
        max_tool_result_chars: config.context().max_tool_result_chars(),
        max_tool_results_per_message_chars: config.context().max_tool_results_per_message_chars(),
    }
}

fn tool_result_budget_config(
    config: &RuntimeConfig,
    storage_root: PathBuf,
) -> ToolResultBudgetConfig {
    ToolResultBudgetConfig {
        max_result_chars: config.context().max_tool_result_chars(),
        max_message_chars: config.context().max_tool_results_per_message_chars(),
        preview_chars: opencowork_runtime::DEFAULT_TOOL_RESULT_PREVIEW_CHARS,
        storage_root: Some(storage_root),
    }
}

fn latest_session(store: &SessionStore) -> Result<Session, Box<dyn std::error::Error>> {
    Ok(store
        .list()?
        .first()
        .map(|descriptor| store.load(&descriptor.id))
        .transpose()?
        .unwrap_or_else(Session::new))
}

fn load_session_or_new(store: &SessionStore, session_id: &str) -> Session {
    store.load(session_id).unwrap_or_else(|_| Session::new())
}

fn print_turn_report(label: &str, session_id: Option<&str>, execution: &TurnExecution) {
    println!("{label}");
    if let Some(session_id) = session_id {
        println!("  Session          {}", session_id);
    }
    println!("  Iterations       {}", execution.summary.iterations);
    println!(
        "  Usage total      {}",
        execution.summary.usage.total_tokens()
    );
    println!("  Prompt tokens    {}", execution.prompt.estimated_tokens);
    println!("  Compacted        {}", execution.prompt.compacted);
    if let Some(snapshot) = &execution.session.context_collapse_snapshot {
        println!("  Collapsed spans  {}", snapshot.collapsed_spans);
        println!("  Collapsed msgs   {}", snapshot.collapsed_messages);
    }
    println!("  Assistant        streamed");
}

fn assistant_text(summary: &TurnSummary) -> Option<&str> {
    summary
        .assistant_messages
        .last()
        .and_then(opencowork_runtime::ConversationMessage::first_text)
}

fn join_args(args: &[String]) -> Result<String, Box<dyn std::error::Error>> {
    let joined = args.join(" ").trim().to_string();
    if joined.is_empty() {
        Err("missing prompt text".into())
    } else {
        Ok(joined)
    }
}

fn render_handoff_prompt(handoff: &opencowork_agents::AgentHandoff) -> String {
    let mut prompt = format!("Delegated task: {}\n", handoff.summary);
    if !handoff.context_files.is_empty() {
        prompt.push_str("\nRelevant files:\n");
        for path in &handoff.context_files {
            prompt.push_str("- ");
            prompt.push_str(path);
            prompt.push('\n');
        }
    }
    prompt.push_str(
        "\nWork only on this delegated scope. Use tools when needed and finish with a concise result.",
    );
    prompt
}

fn preview_text(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    value.chars().take(limit).collect::<String>() + "..."
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_millis()
}

fn parse_u64_flag(args: &[String], flag: &str) -> Option<u64> {
    parse_flag_value(args, flag).and_then(|value| value.parse::<u64>().ok())
}

fn parse_usize_flag(args: &[String], flag: &str) -> Option<usize> {
    parse_flag_value(args, flag).and_then(|value| value.parse::<usize>().ok())
}

fn parse_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == flag)
        .map(|window| window[1].as_str())
        .or_else(|| {
            args.iter().find_map(|arg| {
                arg.strip_prefix(&format!("{flag}="))
                    .filter(|value| !value.is_empty())
            })
        })
}

fn resolve_agent_model(agent_id: &str) -> String {
    default_agents()
        .into_iter()
        .find(|agent| agent.id == agent_id)
        .map(|agent| agent.model)
        .unwrap_or_else(|| "gpt-5.4-mini".to_string())
}

fn skill_roots(cwd: &Path, config_home: &Path) -> Vec<PathBuf> {
    vec![
        cwd.join(".claude").join("skills"),
        cwd.join(".claude").join("commands"),
        cwd.join(".opencowork").join("skills"),
        cwd.join(".opencowork").join("commands"),
        cwd.join(".codex").join("skills"),
        cwd.join(".codex").join("commands"),
        config_home.join("skills"),
        config_home.join("commands"),
    ]
}

fn default_agents() -> Vec<AgentProfile> {
    vec![
        AgentProfile {
            id: "architect".to_string(),
            role: "architect".to_string(),
            model: "gpt-5.4".to_string(),
            capabilities: BTreeSet::from([
                "design".to_string(),
                "prompt".to_string(),
                "planning".to_string(),
                "runtime".to_string(),
            ]),
            max_parallel_tasks: 2,
        },
        AgentProfile {
            id: "systems-worker".to_string(),
            role: "systems-worker".to_string(),
            model: "gpt-5.4-mini".to_string(),
            capabilities: BTreeSet::from([
                "implementation".to_string(),
                "runtime".to_string(),
                "tools".to_string(),
                "mcp".to_string(),
            ]),
            max_parallel_tasks: 3,
        },
        AgentProfile {
            id: "ecosystem-worker".to_string(),
            role: "ecosystem-worker".to_string(),
            model: "gpt-5.4-mini".to_string(),
            capabilities: BTreeSet::from([
                "skills".to_string(),
                "plugins".to_string(),
                "docs".to_string(),
                "testing".to_string(),
            ]),
            max_parallel_tasks: 3,
        },
    ]
}

fn default_tasks() -> Vec<AgentTask> {
    vec![
        AgentTask {
            id: "prompt-engine".to_string(),
            summary: "design prompt orchestration".to_string(),
            required_capabilities: BTreeSet::from(["design".to_string(), "prompt".to_string()]),
            priority: TaskPriority::Critical,
        },
        AgentTask {
            id: "tool-runtime".to_string(),
            summary: "expand tool execution runtime".to_string(),
            required_capabilities: BTreeSet::from([
                "implementation".to_string(),
                "tools".to_string(),
            ]),
            priority: TaskPriority::High,
        },
        AgentTask {
            id: "ecosystem".to_string(),
            summary: "shape skill/plugin/mcp ecosystem".to_string(),
            required_capabilities: BTreeSet::from(["skills".to_string(), "plugins".to_string()]),
            priority: TaskPriority::High,
        },
    ]
}

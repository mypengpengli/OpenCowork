use opencowork_api::{default_openai_profile_for_model, OpenAiCompatClient, OpenAiCompatProfile};
use opencowork_mcp::{merge_tool_definitions, McpCatalog, McpToolDefinition, TransportMcpExecutor};
use opencowork_plugins::{PluginHooks, PluginManager, PluginManagerConfig, PluginTool};
use opencowork_runtime::{
    default_config_home, discover_instruction_sources, discover_nested_instruction_sources,
    discover_project_memory_source, discover_relevant_memory_sources,
    discover_relevant_memory_sources_with_provider, discover_team_memory_source,
    get_context_window_for_model, hydrate_current_session_memory,
    load_current_session_memory_source, maybe_schedule_session_memory_refresh_with_task,
    refresh_current_session_memory, refresh_current_session_memory_with_provider,
    should_report_instruction_file, skill_instruction_source_from_output,
    skill_instruction_sources_from_session, touched_paths_from_message, touched_paths_from_session,
    wait_for_session_memory_refresh, AssistantEvent, ConfigLoader, ContentBlock, ContextOptimizer,
    ContextOptimizerConfig, ConversationMessage, ConversationRuntime, HookRunner,
    InstructionSource, PermissionMode, PermissionPolicy, ProjectContext, PromptBundle,
    PromptComposer, ProviderBackedApiClient, RuntimeConfig, RuntimeHookEvent, RuntimeObserver,
    RuntimePromptAugmenter, RuntimePromptUpdate, Session, TokenUsage, ToolResultBudgetConfig,
    TurnSummary,
};
use opencowork_skills::{render_skill_listing, SkillCatalog, SkillDefinition};
use opencowork_tools::{
    lsp_servers_from_runtime, start_team_memory_sync, team_memory_sync_status, GlobalToolRegistry,
    TeamMemorySyncStatus, ToolSearchSettings,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
struct McpRuntimeBundle {
    catalog: McpCatalog,
    tools: Vec<McpToolDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    AssistantTextDelta {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        input: String,
        required_permission: String,
    },
    ToolResult {
        tool_use_id: String,
        tool_name: String,
        output: String,
        is_error: bool,
    },
    Usage {
        usage: TokenUsage,
    },
    MessageStop,
}

#[derive(Debug, Clone)]
pub struct AppTurnExecution {
    pub session: Session,
    pub summary: TurnSummary,
    pub prompt: PromptBundle,
}

#[derive(Debug, Clone)]
pub struct AppRuntime {
    cwd: PathBuf,
    config: RuntimeConfig,
    config_home: PathBuf,
    permission_mode: PermissionMode,
    plugin_tools: Vec<PluginTool>,
    plugin_hooks: PluginHooks,
    mcp_bundle: McpRuntimeBundle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedTurnContext {
    session: Session,
    prompt: PromptBundle,
}

struct ConditionalPromptAugmenter {
    cwd: PathBuf,
    catalog: SkillCatalog,
    activated_skills: BTreeSet<String>,
    loaded_skills: BTreeSet<String>,
}

struct EventObserver<'a> {
    sink: &'a mut dyn FnMut(AppEvent),
}

impl AppRuntime {
    pub fn load(cwd: impl Into<PathBuf>) -> Result<Self, Box<dyn std::error::Error>> {
        let cwd = cwd.into();
        let config = ConfigLoader::default_for(&cwd).load()?;
        let config_home = default_config_home();
        let _ = start_team_memory_sync(&cwd, &config_home, &config);
        let plugin_manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        let plugin_tools = plugin_manager.aggregated_tools().unwrap_or_default();
        let plugin_hooks = plugin_manager.aggregated_hooks().unwrap_or_default();
        let permission_mode = config
            .permission_mode()
            .unwrap_or(PermissionMode::WorkspaceWrite);
        let mcp_bundle = resolve_mcp_bundle(&config, &config_home);

        Ok(Self {
            cwd,
            config,
            config_home,
            permission_mode,
            plugin_tools,
            plugin_hooks,
            mcp_bundle,
        })
    }

    #[must_use]
    pub fn team_memory_sync_status(&self) -> TeamMemorySyncStatus {
        team_memory_sync_status()
    }

    /// Read local sync state without constructing a runtime or contacting MCP servers.
    pub fn current_team_memory_sync_status() -> TeamMemorySyncStatus {
        team_memory_sync_status()
    }

    /// Initialize team memory independently of model and tool discovery.
    pub fn initialize_team_memory_sync(cwd: &Path) -> Result<TeamMemorySyncStatus, String> {
        let config = ConfigLoader::default_for(cwd)
            .load()
            .map_err(|error| error.to_string())?;
        start_team_memory_sync(cwd, &default_config_home(), &config)?;
        Ok(team_memory_sync_status())
    }

    #[must_use]
    pub fn model(&self) -> &str {
        self.config.model().unwrap_or("gpt-5.4-mini")
    }

    #[must_use]
    pub fn permission_mode(&self) -> PermissionMode {
        self.permission_mode
    }

    #[must_use]
    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    pub fn tool_manifest(
        &self,
    ) -> Result<Vec<(String, String, PermissionMode, serde_json::Value)>, String> {
        Ok(self.build_registry()?.definitions())
    }

    pub fn prepare_prompt_plan(
        &self,
        model: &str,
        session: &Session,
    ) -> Result<PromptBundle, Box<dyn std::error::Error>> {
        Ok(self.prepare_turn_context(model, session, None)?.prompt)
    }

    pub fn run_turn(
        &self,
        model: &str,
        session: Session,
        user_input: &str,
        mut sink: Option<&mut dyn FnMut(AppEvent)>,
    ) -> Result<AppTurnExecution, Box<dyn std::error::Error>> {
        let profile = resolve_provider_profile(&self.config, model);
        let provider = OpenAiCompatClient::from_profile(profile)?;
        let prepared = self.prepare_turn_context(model, &session, Some(user_input))?;
        let mut hooks = build_hook_runner(&self.config, &self.plugin_hooks);
        let instruction_files = prepared
            .prompt
            .layers
            .iter()
            .filter_map(|layer| layer.name.strip_prefix("instruction:"))
            .filter(|label| {
                should_report_instruction_file(label, &self.cwd, &self.config_home, &session)
            })
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let instructions_loaded = hooks.run_instructions_loaded(&instruction_files);
        if instructions_loaded.is_denied() {
            return Err(std::io::Error::other(format!(
                "instructions-loaded hook denied execution: {}",
                instructions_loaded.messages().join(" | ")
            ))
            .into());
        }

        let touched_files = touched_paths_from_session(&self.cwd, &prepared.session)
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        if !touched_files.is_empty() {
            let file_changed = hooks.run_file_changed(&touched_files);
            if file_changed.is_denied() {
                return Err(std::io::Error::other(format!(
                    "file-changed hook denied execution: {}",
                    file_changed.messages().join(" | ")
                ))
                .into());
            }
        }

        let mut conversation = ConversationRuntime::new(
            prepared.session,
            ProviderBackedApiClient::new(provider, model),
            self.build_registry().map_err(std::io::Error::other)?,
            PermissionPolicy::new(self.permission_mode),
            prepared.prompt.system_prompt.clone(),
            hooks,
        )
        .with_tool_result_budget(tool_result_budget_config(
            &self.config,
            self.config_home.join("tool-results"),
        ))
        .with_prompt_augmenter(Box::new(ConditionalPromptAugmenter::new(
            &self.cwd,
            &self.config_home,
            &session,
        )));

        let summary = if let Some(ref mut sink) = sink {
            let mut observer = EventObserver { sink: *sink };
            conversation.run_turn_with_observer(user_input, None, Some(&mut observer))?
        } else {
            conversation.run_turn(user_input, None)?
        };

        let mut updated_session = conversation.session().clone();
        let _ = schedule_provider_backed_session_memory_refresh(
            &self.config,
            model,
            &mut updated_session,
            &self.cwd,
            &self.config_home,
        )?;

        Ok(AppTurnExecution {
            session: updated_session,
            summary,
            prompt: prepared.prompt,
        })
    }

    fn build_registry(&self) -> Result<GlobalToolRegistry, String> {
        let skill_catalog = SkillCatalog::discover(&skill_roots(&self.cwd, &self.config_home));
        GlobalToolRegistry::with_extensions(
            self.plugin_tools.clone(),
            self.mcp_bundle.tools.clone(),
            Some(Box::new(TransportMcpExecutor::with_credential_store(
                self.mcp_bundle.catalog.clone(),
                Some(mcp_auth_root(&self.config_home)),
            ))),
            skill_catalog,
            lsp_servers_from_runtime(&self.cwd, self.config.lsp().servers().values().cloned()),
        )
        .map(|registry| {
            registry.with_tool_search_settings(ToolSearchSettings::from_env(
                self.model(),
                self.config.context().context_window_tokens_override(),
            ))
        })
    }

    fn prepare_turn_context(
        &self,
        model: &str,
        session: &Session,
        pending_user_input: Option<&str>,
    ) -> Result<PreparedTurnContext, Box<dyn std::error::Error>> {
        let mut hydrated_session = session.clone();
        wait_for_session_memory_refresh(&mut hydrated_session, &self.cwd, &self.config_home)?;
        let hydrated_session =
            hydrate_current_session_memory(&hydrated_session, &self.cwd, &self.config_home)?;
        let instructions = collect_instruction_sources(
            &self.cwd,
            &self.config_home,
            &self.config,
            &hydrated_session,
            model,
            pending_user_input,
        )?;
        let optimizer = ContextOptimizer::new(context_optimizer_config(&self.config, model));
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
                cwd: self.cwd.clone(),
                current_date: "2026-04-02".to_string(),
                model: Some(model.to_string()),
                permission_mode: self.permission_mode,
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
}

impl ConditionalPromptAugmenter {
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

impl RuntimePromptAugmenter for ConditionalPromptAugmenter {
    fn on_tool_result(
        &mut self,
        _session: &Session,
        tool_result: &ConversationMessage,
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

impl RuntimeObserver for EventObserver<'_> {
    fn on_assistant_event(&mut self, event: &AssistantEvent) {
        let event = match event {
            AssistantEvent::TextDelta(text) => AppEvent::AssistantTextDelta { text: text.clone() },
            AssistantEvent::ToolUse {
                id,
                name,
                input,
                required_permission,
            } => AppEvent::ToolCall {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
                required_permission: format!("{required_permission:?}"),
            },
            AssistantEvent::Usage(usage) => AppEvent::Usage { usage: *usage },
            AssistantEvent::MessageStop => AppEvent::MessageStop,
        };
        (self.sink)(event);
    }

    fn on_tool_result(&mut self, message: &ConversationMessage) {
        let Some(ContentBlock::ToolResult {
            tool_use_id,
            tool_name,
            output,
            is_error,
        }) = message.blocks.first()
        else {
            return;
        };
        (self.sink)(AppEvent::ToolResult {
            tool_use_id: tool_use_id.clone(),
            tool_name: tool_name.clone(),
            output: output.clone(),
            is_error: *is_error,
        });
    }
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

fn resolve_mcp_bundle(config: &RuntimeConfig, config_home: &Path) -> McpRuntimeBundle {
    let catalog = McpCatalog::new(config.mcp_servers().clone());
    let configured = catalog.tool_definitions();
    let tools = if catalog.servers().is_empty() {
        configured
    } else {
        let executor = TransportMcpExecutor::with_credential_store(
            catalog.clone(),
            Some(mcp_auth_root(config_home)),
        );
        executor
            .discover_tools()
            .map(|discovered| merge_tool_definitions(configured.clone(), discovered))
            .unwrap_or(configured)
    };

    McpRuntimeBundle { catalog, tools }
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

fn mcp_auth_root(config_home: &Path) -> PathBuf {
    config_home.join("mcp-auth")
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

fn lsp_context_prompt_updates(message: &ConversationMessage) -> Vec<RuntimePromptUpdate> {
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

fn explicit_skill_prompt_updates(
    message: &ConversationMessage,
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
    storage_root: std::path::PathBuf,
) -> ToolResultBudgetConfig {
    ToolResultBudgetConfig {
        max_result_chars: config.context().max_tool_result_chars(),
        max_message_chars: config.context().max_tool_results_per_message_chars(),
        preview_chars: opencowork_runtime::DEFAULT_TOOL_RESULT_PREVIEW_CHARS,
        storage_root: Some(storage_root),
    }
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

#[cfg(test)]
mod tests {
    use super::{AppEvent, EventObserver};
    use opencowork_runtime::{
        AssistantEvent, ConversationMessage, PermissionMode, RuntimeObserver, TokenUsage,
    };

    #[test]
    fn event_observer_maps_runtime_events_to_app_events() {
        let mut events = Vec::new();
        let mut sink = |event| events.push(event);
        let mut observer = EventObserver { sink: &mut sink };

        observer.on_assistant_event(&AssistantEvent::TextDelta("hello".to_string()));
        observer.on_assistant_event(&AssistantEvent::ToolUse {
            id: "tool-1".to_string(),
            name: "read_file".to_string(),
            input: "{\"path\":\"README.md\"}".to_string(),
            required_permission: PermissionMode::ReadOnly,
        });
        observer.on_assistant_event(&AssistantEvent::Usage(TokenUsage {
            input_tokens: 1,
            output_tokens: 2,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        }));
        observer.on_tool_result(&ConversationMessage::tool_result(
            "tool-1",
            "read_file",
            "{\"content\":\"ok\"}",
            false,
        ));

        assert!(matches!(
            &events[0],
            AppEvent::AssistantTextDelta { text } if text == "hello"
        ));
        assert!(matches!(
            &events[1],
            AppEvent::ToolCall { name, .. } if name == "read_file"
        ));
        assert!(matches!(
            &events[2],
            AppEvent::Usage { usage } if usage.output_tokens == 2
        ));
        assert!(matches!(
            &events[3],
            AppEvent::ToolResult { tool_name, .. } if tool_name == "read_file"
        ));
    }
}

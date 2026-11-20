use super::*;
use opencowork_agents::jobs::{now_ms, ChildTask, TaskGroup, TaskStore};
use opencowork_runtime::{ExclusiveLease, ProcessTree, Session, SessionStore};
use std::collections::BTreeMap;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct Execution {
    child: Child,
    tree: ProcessTree,
    _effects: Option<ExclusiveLease>,
}

pub(super) fn start(state: ShellState) {
    std::thread::spawn(move || {
        let store = TaskStore::new(&state.config_home);
        use notify::{RecursiveMode, Watcher};
        let event_root = state.config_home.join("subagents");
        let _ = std::fs::create_dir_all(&event_root);
        let (wake, events) = std::sync::mpsc::sync_channel(32);
        let mut watcher = notify::recommended_watcher(move |_: notify::Result<notify::Event>| {
            let _ = wake.try_send(());
        })
        .ok();
        if let Some(watcher) = watcher.as_mut() {
            let _ = watcher.watch(&event_root, RecursiveMode::Recursive);
        }
        let lease_path = |host| {
            state
                .config_home
                .join("subagents")
                .join("hosts")
                .join(format!("{host}.lock"))
        };
        let Ok(_host_lease) = ExclusiveLease::acquire(&lease_path(std::process::id())) else {
            return;
        };
        // A stopped host releases this OS lease even after an abrupt crash.
        // Leave its checkpoints intact and require an explicit new turn to resume.
        for group in store.groups() {
            if group.host != std::process::id() && !group.cancelled {
                if let Ok(_stale_host) = ExclusiveLease::acquire(&lease_path(group.host)) {
                    for task in store.tasks(&group.id) {
                        let _ = store.update_task(&task.id, |t| {
                            if matches!(t.status.as_str(), "queued" | "running" | "waiting" | "ready") {
                                t.status = "interrupted".into();
                                t.error = Some("Host restarted; inspect the saved child session before delegating a continuation".into());
                            }
                        });
                    }
                    let _ = store.update_group(&group.id, |g| g.cancelled = true);
                }
            }
        }
        let mut running = BTreeMap::<String, Execution>::new();
        loop {
            for group in store
                .groups()
                .into_iter()
                .filter(|g| g.host == std::process::id())
            {
                if group.cancelled
                    && !running
                        .keys()
                        .any(|id| id.starts_with(&format!("{}-", group.id)))
                {
                    continue;
                }
                let tasks = store.tasks(&group.id);
                let active = group.detached
                    || state
                        .turns
                        .lock()
                        .map(|t| t.contains_key(&group.id))
                        .unwrap_or(false);
                let spent = tasks
                    .iter()
                    .map(|t| t.usage.total_tokens() as u64)
                    .sum::<u64>()
                    + group.root_tokens;
                let stop = !active
                    || group.cancelled
                    || now_ms() >= group.deadline
                    || spent >= group.max_tokens;
                if active
                    && !tasks.is_empty()
                    && (spent >= group.max_tokens || now_ms() >= group.deadline)
                {
                    if let Ok(turns) = state.turns.lock() {
                        if let Some(turn) = turns.get(&group.id) {
                            let _ = turn.cancel.send(true);
                        }
                    }
                }
                for task in tasks.iter().filter(|t| {
                    matches!(
                        t.status.as_str(),
                        "running" | "queued" | "waiting" | "ready"
                    )
                }) {
                    let parent_stopped = tasks.iter().any(|parent| {
                        parent.id == task.parent
                            && matches!(
                                parent.status.as_str(),
                                "cancelled" | "failed" | "interrupted"
                            )
                    });
                    if stop || parent_stopped {
                        let _ = store.cancel_tree(&task.id);
                    }
                }
                if stop {
                    let _ = store.update_group(&group.id, |g| g.cancelled = true);
                }
                for task in &tasks {
                    if let Some(mut execution) = running.remove(&task.id) {
                        let current = store.task(&task.id).unwrap_or_else(|_| task.clone());
                        if current.status == "cancelled" || stop {
                            let _ = execution.tree.terminate();
                            let _ = execution.child.wait();
                        } else if execution.child.try_wait().ok().flatten().is_some() {
                            let _ = store.update_task(&task.id, |t| {
                                if matches!(t.status.as_str(), "running" | "waiting" | "ready") {
                                    t.status = "interrupted".into();
                                    t.error =
                                        Some("Worker exited before saving its final result".into());
                                }
                            });
                        } else {
                            running.insert(task.id.clone(), execution);
                        }
                    }
                }
                if stop {
                    continue;
                }
                let mut live = tasks
                    .iter()
                    .filter(|t| t.status == "running" && running.contains_key(&t.id))
                    .count();
                let mut reserved = tasks
                    .iter()
                    .filter(|t| t.status == "running" && running.contains_key(&t.id))
                    .map(|t| t.token_limit.saturating_sub(t.usage.total_tokens() as u64))
                    .sum::<u64>();
                for task in tasks
                    .iter()
                    .filter(|t| t.status == "ready" && running.contains_key(&t.id))
                {
                    if live >= group.concurrency {
                        break;
                    }
                    let needed = task
                        .token_limit
                        .saturating_sub(task.usage.total_tokens() as u64);
                    if spent + reserved + needed > group.max_tokens && live > 0 {
                        continue;
                    }
                    let available = group.max_tokens.saturating_sub(spent + reserved);
                    let _ = store.update_task(&task.id, |t| {
                        if t.status == "ready" {
                            t.token_limit = t.usage.total_tokens() as u64 + needed.min(available);
                            t.status = "running".into();
                        }
                    });
                    reserved += needed.min(available);
                    live += 1;
                }
                for task in tasks.iter().filter(|t| t.status == "queued") {
                    if live >= group.concurrency {
                        break;
                    }
                    if running.contains_key(&task.id) {
                        continue;
                    }
                    let available = group.max_tokens.saturating_sub(spent + reserved);
                    if available == 0 || (task.token_limit > available && live > 0) {
                        continue;
                    }
                    let mut task = task.clone();
                    task.token_limit =
                        (task.usage.total_tokens() as u64 + available).min(task.token_limit);
                    let effects = if task.writable {
                        match ExclusiveLease::acquire(
                            &state.config_home.join("subagent-effects.lock"),
                        ) {
                            Ok(lease) => Some(lease),
                            Err(_) => continue,
                        }
                    } else {
                        None
                    };
                    let result = launch(&state, &group, &task, effects);
                    match result {
                        Ok(execution) => {
                            reserved += task
                                .token_limit
                                .saturating_sub(task.usage.total_tokens() as u64);
                            live += 1;
                            running.insert(task.id.clone(), execution);
                        }
                        Err(error) => {
                            let _ = store.update_task(&task.id, |t| {
                                if t.status == "queued" || t.status == "running" {
                                    t.status = "failed".into();
                                    t.error = Some(error);
                                }
                            });
                        }
                    }
                }
            }
            while events.try_recv().is_ok() {}
            let _ = events.recv_timeout(if running.is_empty() {
                Duration::from_secs(2)
            } else {
                Duration::from_millis(250)
            });
        }
    });
}

fn launch(
    state: &ShellState,
    group: &TaskGroup,
    task: &ChildTask,
    effects: Option<ExclusiveLease>,
) -> Result<Execution, String> {
    let store = TaskStore::new(&state.config_home);
    let admitted = store.update_task(&task.id, |t| {
        if t.status == "queued" {
            t.token_limit = task.token_limit;
            t.status = "running".into();
        }
    })?;
    if admitted.status != "running" || admitted.attempt != task.attempt {
        return Err("Task cancelled before launch".into());
    }
    let mut command = Command::new(env::current_exe().map_err(|e| e.to_string())?);
    command
        .arg("--subagent-worker")
        .arg(&task.id)
        .arg(task.attempt.to_string())
        .current_dir(&group.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("OPENCOWORK_CONFIG_HOME", &state.config_home)
        .env("OPENCOWORK_ROOT_TURN_ID", &group.id)
        .env("OPENCOWORK_SESSION_ID", &task.id)
        .env("OPENCOWORK_SUBAGENT_DEPTH", task.depth.to_string())
        .env(
            "OPENCOWORK_STEERING_FILE",
            state
                .config_home
                .join("turn-control")
                .join(format!("{}.json", task.id)),
        )
        .env(
            "OPENCOWORK_TASK_TOKEN_LIMIT",
            task.token_limit
                .saturating_sub(task.usage.total_tokens() as u64)
                .to_string(),
        )
        .env("OPENCOWORK_TASK_DEADLINE", group.deadline.to_string())
        .env(
            "OPENCOWORK_EFFECTS_LEASE_HELD",
            if task.writable { "1" } else { "0" },
        )
        .env_remove("OPENCOWORK_CHECKPOINT_ACK");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    match ProcessTree::attach(child.id()) {
        Ok(tree) => {
            use std::io::Write;
            child
                .stdin
                .take()
                .ok_or("Missing worker start gate")?
                .write_all(b"start\n")
                .map_err(|e| e.to_string())?;
            Ok(Execution {
                child,
                tree,
                _effects: effects,
            })
        }
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(error.to_string())
        }
    }
}

pub(super) fn worker() -> Result<(), String> {
    let result = worker_inner();
    if let (Err(error), Some(id)) = (&result, env::args().nth(2)) {
        let _ = TaskStore::new(&default_config_home()).update_task(&id, |t| {
            if t.status == "running" {
                t.status = "failed".into();
                t.error = Some(error.clone());
            }
        });
    }
    result
}

fn worker_inner() -> Result<(), String> {
    let mut gate = String::new();
    std::io::stdin()
        .read_line(&mut gate)
        .map_err(|e| e.to_string())?;
    if gate != "start\n" {
        return Err("Worker was not attached to its process tree".into());
    }
    let id = env::args().nth(2).ok_or("Missing child task id")?;
    let attempt = env::args()
        .nth(3)
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or("Missing child attempt")?;
    let home = default_config_home();
    let store = TaskStore::new(&home);
    let task = store.task(&id)?;
    let group = store.group(&task.group)?;
    if task.attempt != attempt || task.status != "running" {
        return Ok(());
    }
    apply_shell_api_key_override(&group.cwd)?;
    let sessions = SessionStore::new(home.join("sessions"));
    let mut previous = sessions
        .load(&id)
        .unwrap_or_else(|_| Session::new().with_id(&id));
    if let Some(effort) = &task.reasoning_effort {
        std::env::set_var("OPENCOWORK_REASONING_EFFORT", effort);
    }
    previous.workspace = Some(group.cwd.to_string_lossy().into_owned());
    previous.title = Some(task.name.clone());
    let input = if attempt == 0 {
        task.prompt.clone()
    } else {
        format!("Continue the delegated task from saved state. Inspect prior actions and unknown outcomes before repeating any operation. Original task: {}", task.prompt)
    };
    let mut partial = super::chat::PartialTurn::new(previous.clone(), &input);
    let mut usage = task.usage;
    let mut last_save = std::time::Instant::now();
    let runtime = opencowork_app::AppRuntime::load(&group.cwd)
        .map_err(|e| e.to_string())?
        .with_permission_mode(if task.writable {
            ConfigLoader::default_for(&group.cwd)
                .load()
                .map_err(|e| e.to_string())?
                .permission_mode()
                .unwrap_or(opencowork_runtime::PermissionMode::ReadOnly)
        } else {
            opencowork_runtime::PermissionMode::ReadOnly
        });
    let config = ConfigLoader::default_for(&group.cwd)
        .load()
        .map_err(|e| e.to_string())?;
    let model = task
        .model
        .as_deref()
        .or(config.model())
        .unwrap_or("gpt-5.4-mini");
    let result = runtime.run_turn_with_context(model, previous, &input, "You are a delegated worker. Complete only your assigned task and return evidence, affected paths and unresolved questions to the parent. Use Subagents only within the configured delegation limits.", Some(&mut |event| {
        partial.apply(event.clone());
        if let opencowork_app::AppEvent::Usage { usage: next } = event { usage.accumulate(next); }
        let critical = matches!(event, opencowork_app::AppEvent::ToolCall { .. } | opencowork_app::AppEvent::ToolResult { .. } | opencowork_app::AppEvent::UserMessage { .. } | opencowork_app::AppEvent::ContextCompacted { .. });
        if last_save.elapsed() >= Duration::from_millis(250) || critical || matches!(event, opencowork_app::AppEvent::MessageStop) {
            let saved = sessions.save_named(&id, &partial.snapshot());
            let ledger = store.update_task(&id, |t| { if t.attempt == attempt { t.usage = usage; } });
            if critical && (saved.is_err() || ledger.is_err()) {
                // The scheduler marks this worker interrupted. Never execute a
                // tool without first saving its intent and preceding results.
                std::process::exit(74);
            }
            last_save = std::time::Instant::now();
        }
    }));
    let saved = result
        .as_ref()
        .map(|execution| execution.session.clone())
        .unwrap_or_else(|_| partial.snapshot());
    sessions
        .save_named(&id, &saved)
        .map_err(|e| e.to_string())?;
    store.update_task(&id, |t| {
        if t.attempt != attempt || t.status != "running" {
            return;
        }
        t.usage = usage;
        match result {
            Ok(execution) => {
                t.affected_paths =
                    opencowork_runtime::touched_paths_from_session(&group.cwd, &execution.session)
                        .iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect();
                t.evidence = execution
                    .session
                    .messages
                    .iter()
                    .flat_map(|m| m.blocks.iter())
                    .filter_map(|b| match b {
                        opencowork_runtime::ContentBlock::ToolResult {
                            tool_name,
                            is_error: false,
                            ..
                        } => Some(format!("Completed tool: {tool_name}")),
                        _ => None,
                    })
                    .collect();
                t.status = "completed".into();
                t.result = execution
                    .summary
                    .assistant_messages
                    .last()
                    .map(|message| {
                        message
                            .blocks
                            .iter()
                            .filter_map(|block| match block {
                                opencowork_runtime::ContentBlock::Text { text } => {
                                    Some(text.as_str())
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
            }
            Err(error) => {
                t.status = "failed".into();
                t.error = Some(error.to_string());
            }
        }
    })?;
    Ok(())
}

pub(super) async fn overview(State(state): State<ShellState>) -> Result<Json<Value>, ApiError> {
    let store = TaskStore::new(&state.config_home);
    let groups: Vec<_> = store
        .groups()
        .into_iter()
        .filter(|g| g.cwd == state.cwd)
        .collect();
    let tasks: Vec<_> = store
        .all_tasks()
        .into_iter()
        .filter(|t| groups.iter().any(|g| g.id == t.group))
        .collect();
    let settings = read_settings(&state.cwd)?;
    Ok(Json(
        serde_json::json!({"settings":settings.get("subagents").cloned().unwrap_or_else(||serde_json::json!({"maxConcurrent":6,"maxDepth":1})),"tasks":tasks}),
    ))
}

pub(super) async fn cancel(
    State(state): State<ShellState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let store = TaskStore::new(&state.config_home);
    let task = store.task(&id).map_err(ApiError::not_found)?;
    let group = store.group(&task.group).map_err(ApiError::not_found)?;
    if group.cwd != state.cwd {
        return Err(ApiError::bad_request("Task belongs to another workspace"));
    }
    let task = store.cancel_tree(&id).map_err(internal_error)?;
    Ok(Json(serde_json::json!(task)))
}

pub(super) async fn configure(
    State(state): State<ShellState>,
    Json(input): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let concurrency = input["maxConcurrent"]
        .as_u64()
        .filter(|v| *v > 0 && *v <= u32::MAX as u64)
        .ok_or_else(|| ApiError::bad_request("Concurrency must be a positive integer"))?;
    let depth = input["maxDepth"]
        .as_u64()
        .filter(|v| *v > 0 && *v <= 16)
        .ok_or_else(|| ApiError::bad_request("Depth must be between 1 and 16"))?;
    let mut settings = read_settings(&state.cwd)?;
    settings["subagents"] = serde_json::json!({"maxConcurrent":concurrency,"maxDepth":depth});
    write_settings(&state.cwd, &settings)?;
    let store = TaskStore::new(&state.config_home);
    for group in store
        .groups()
        .into_iter()
        .filter(|g| g.cwd == state.cwd && g.host == std::process::id())
    {
        store
            .update_group(&group.id, |g| {
                g.concurrency = concurrency as usize;
                g.max_depth = depth as usize;
            })
            .map_err(internal_error)?;
    }
    overview(State(state)).await
}

fn detached_group(state: &ShellState) -> Result<TaskGroup, ApiError> {
    let config = ConfigLoader::default_for(&state.cwd)
        .load()
        .map_err(internal_error)?;
    let limits = opencowork_runtime::TurnLimits::from_settings(config.merged());
    let id = format!("manual-{}", uuid::Uuid::new_v4());
    let group = TaskGroup {
        detached: true,
        id: id.clone(),
        session: id,
        cwd: state.cwd.clone(),
        host: std::process::id(),
        concurrency: config
            .merged()
            .pointer("/subagents/maxConcurrent")
            .and_then(Value::as_u64)
            .unwrap_or(6)
            .max(1) as usize,
        max_depth: config
            .merged()
            .pointer("/subagents/maxDepth")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .clamp(1, 16) as usize,
        max_tokens: limits.max_tokens as u64,
        root_tokens: 0,
        deadline: now_ms() + limits.max_seconds * 1000,
        cancelled: false,
    };
    TaskStore::new(&state.config_home)
        .create_group(&group)
        .map_err(internal_error)?;
    Ok(group)
}
pub(super) async fn create(
    State(state): State<ShellState>,
    Json(input): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let name = input["name"]
        .as_str()
        .filter(|s| s.len() <= 32 && TaskStore::valid(s))
        .ok_or_else(|| {
            ApiError::bad_request(
                "Use a task name with 1-32 letters, numbers, dashes or underscores",
            )
        })?;
    let prompt = input["task"]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 64000)
        .ok_or_else(|| ApiError::bad_request("Task must contain 1-64000 bytes"))?;
    let writable = input["writable"].as_bool().unwrap_or(false);
    if writable
        && ConfigLoader::default_for(&state.cwd)
            .load()
            .map_err(internal_error)?
            .permission_mode()
            .unwrap_or(opencowork_runtime::PermissionMode::ReadOnly)
            < opencowork_runtime::PermissionMode::WorkspaceWrite
    {
        return Err(ApiError::bad_request(
            "Current workspace does not permit writable child tasks",
        ));
    }
    let group = detached_group(&state)?;
    let task = ChildTask {
        id: format!("{}-0-{name}", group.id),
        group: group.id.clone(),
        parent: group.session.clone(),
        depth: 1,
        name: name.into(),
        prompt: prompt.into(),
        model: input["model"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        reasoning_effort: input["reasoningEffort"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        fork_from: input["forkFrom"].as_str().map(str::to_owned),
        resumed_from: None,
        started_at: None,
        finished_at: None,
        affected_paths: vec![],
        evidence: vec![],
        writable,
        status: "queued".into(),
        result: String::new(),
        error: None,
        usage: Default::default(),
        token_limit: group.max_tokens / (group.concurrency as u64 + 1),
        attempt: 0,
        created_at: now_ms(),
    };
    if let Some(source) = &task.fork_from {
        TaskStore::new(&state.config_home)
            .fork_session(source, &task.id, &state.cwd)
            .map_err(ApiError::bad_request)?;
    }
    Ok(Json(serde_json::json!(TaskStore::new(&state.config_home)
        .create(&task)
        .map_err(internal_error)?)))
}
pub(super) async fn resume(
    State(state): State<ShellState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let store = TaskStore::new(&state.config_home);
    let task = store.task(&id).map_err(ApiError::not_found)?;
    let old = store.group(&task.group).map_err(internal_error)?;
    if old.cwd != state.cwd {
        return Err(ApiError::bad_request("Task belongs to another workspace"));
    }
    let group = if old.host == std::process::id() && !old.cancelled {
        old
    } else {
        detached_group(&state)?
    };
    Ok(Json(serde_json::json!(store
        .resume_into(&id, &group, &group.session)
        .map_err(|e| ApiError::bad_request(e.to_string()))?)))
}
pub(super) async fn message(
    State(state): State<ShellState>,
    AxumPath(id): AxumPath<String>,
    Json(input): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let store = TaskStore::new(&state.config_home);
    let task = store.task(&id).map_err(ApiError::not_found)?;
    if store.group(&task.group).map_err(internal_error)?.cwd != state.cwd {
        return Err(ApiError::bad_request("Task belongs to another workspace"));
    }
    let text = input["text"]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 16000)
        .ok_or_else(|| ApiError::bad_request("Message must contain 1-16000 bytes"))?;
    let path = state
        .config_home
        .join("turn-control")
        .join(format!("{id}.json"));
    let _lease = ExclusiveLease::acquire(&path.with_extension("lock")).map_err(internal_error)?;
    let mut messages: Vec<Value> = fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if messages.len() >= 20 {
        return Err(ApiError::bad_request("Child steering queue is full"));
    }
    messages.push(serde_json::json!({"id":uuid::Uuid::new_v4().to_string(),"mode":"steer","status":"pending","text":text}));
    opencowork_runtime::write_json_atomic(&path, &messages).map_err(internal_error)?;
    Ok(Json(serde_json::json!({"accepted":true})))
}
pub(super) async fn processes(State(state): State<ShellState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!(
        opencowork_runtime::process_sessions::ProcessStore::new(&state.config_home)
            .list()
            .into_iter()
            .filter(|p| p.cwd == state.cwd)
            .collect::<Vec<_>>()
    )))
}
pub(super) async fn process_output(
    State(state): State<ShellState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let store = opencowork_runtime::process_sessions::ProcessStore::new(&state.config_home);
    if store.get(&id).map_err(ApiError::not_found)?.cwd != state.cwd {
        return Err(ApiError::bad_request(
            "Process belongs to another workspace",
        ));
    }
    Ok(Json(store.read(&id, 0, 65536, 0).map_err(internal_error)?))
}
pub(super) async fn cancel_process(
    State(state): State<ShellState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    let store = opencowork_runtime::process_sessions::ProcessStore::new(&state.config_home);
    if store.get(&id).map_err(ApiError::not_found)?.cwd != state.cwd {
        return Err(ApiError::bad_request(
            "Process belongs to another workspace",
        ));
    }
    store
        .submit(&serde_json::json!({"action":"cancel","id":id}))
        .map_err(internal_error)?;
    Ok(Json(serde_json::json!({"accepted":true})))
}

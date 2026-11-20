use super::*;
use opencowork_agents::jobs::{now_ms, ChildTask, TaskStore};

pub(super) fn spec() -> ToolSpec {
    ToolSpec {
        name: "Subagents", aliases: &[], exposure: ToolExposure::Core,
        required_permission: PermissionMode::ReadOnly,
        description: "Delegate independent tasks to queued child agents. create needs a stable unique name and a complete task with user constraints and acceptance checks. Children start with fresh context and are read-only unless writable=true is explicitly requested. A writable child takes the workspace mutation lease. Default concurrency is 6, extra tasks queue. inspect returns the full result. wait blocks for at most 30 seconds. Use message to steer a running child, cancel to stop it, resume only after inspecting failed/interrupted effects. Depth and shared token/time limits apply.",
        input_schema: json!({"type":"object","additionalProperties":false,"properties":{
            "action":{"enum":["create","list","inspect","wait","message","cancel","resume"]},
            "name":{"type":"string"},"id":{"type":"string"},"task":{"type":"string"},
            "model":{"type":"string"},"reasoningEffort":{"enum":["none","minimal","low","medium","high","xhigh"]},"fork":{"type":"boolean"},"writable":{"type":"boolean"},"timeoutMs":{"type":"integer","minimum":0,"maximum":30000}
        },"required":["action"]}),
    }
}

pub(super) fn execute(input: &Value) -> Result<String, ToolError> {
    run(input).map(|v| v.to_string()).map_err(ToolError::new)
}
fn run(input: &Value) -> Result<Value, String> {
    let group_id = std::env::var("OPENCOWORK_ROOT_TURN_ID")
        .map_err(|_| "Subagents require an active desktop/web turn")?;
    let caller = std::env::var("OPENCOWORK_SESSION_ID").map_err(|_| "Missing caller session")?;
    let store = TaskStore::new(&opencowork_runtime::default_config_home());
    let group = store.group(&group_id)?;
    let depth = std::env::var("OPENCOWORK_SUBAGENT_DEPTH")
        .ok()
        .and_then(|x| x.parse::<usize>().ok())
        .unwrap_or(0);
    let action = input["action"].as_str().ok_or("Missing action")?;
    if action == "create" {
        if depth >= group.max_depth {
            return Err("Subagent depth limit reached; perform the delegated task directly".into());
        }
        if group.cancelled || now_ms() >= group.deadline {
            return Err("Parent task is stopped or out of time".into());
        }
        let name = input["name"]
            .as_str()
            .filter(|n| n.len() <= 32 && TaskStore::valid(n))
            .ok_or("Use a unique 1-32 character task name")?;
        let prompt = input["task"]
            .as_str()
            .filter(|p| !p.trim().is_empty() && p.len() <= 64000)
            .ok_or("Task must contain 1-64000 bytes")?;
        let writable = input["writable"].as_bool().unwrap_or(false);
        let config = opencowork_runtime::ConfigLoader::default_for(&group.cwd)
            .load()
            .map_err(|e| e.to_string())?;
        if writable
            && config.permission_mode().unwrap_or(PermissionMode::ReadOnly)
                < PermissionMode::WorkspaceWrite
        {
            return Err("Parent permission does not allow writable children".into());
        }
        if writable && depth > 0 {
            return Err("Nested writable tasks must be delegated by the root to avoid workspace lease deadlocks".into());
        }
        let task = ChildTask {
            reasoning_effort: input["reasoningEffort"].as_str().map(str::to_owned),
            fork_from: input["fork"]
                .as_bool()
                .filter(|v| *v)
                .map(|_| caller.clone()),
            resumed_from: None,
            started_at: None,
            finished_at: None,
            affected_paths: vec![],
            evidence: vec![],
            id: format!("{}-{}-{name}", group.id, depth),
            group: group.id.clone(),
            parent: caller.clone(),
            depth: depth + 1,
            name: name.into(),
            prompt: prompt.into(),
            model: input["model"].as_str().map(str::to_owned),
            writable,
            status: "queued".into(),
            result: String::new(),
            error: None,
            usage: Default::default(),
            token_limit: (group.max_tokens / (group.concurrency as u64 + 1)).max(1),
            attempt: 0,
            created_at: now_ms(),
        };
        if task.fork_from.is_some() {
            store.fork_session(&caller, &task.id, &group.cwd)?;
        }
        return serde_json::to_value(store.create(&task)?).map_err(|e| e.to_string());
    }
    if action == "list" {
        return Ok(json!(store.tasks(&group.id)));
    }
    let id = input["id"].as_str().ok_or("Missing task id")?;
    let task = store.task(id)?;
    if task.group != group.id {
        if store.group(&task.group)?.cwd == group.cwd && caller == group.session {
            if action == "inspect" {
                return Ok(json!(task));
            }
            if action == "resume" {
                return Ok(json!(store.resume_into(id, &group, &caller)?));
            }
        }
        return Err("Task belongs to a different root turn".into());
    }
    let may_control = caller == group.session || caller == task.parent;
    match action {
        "inspect" => Ok(json!(task)),
        "wait" => {
            if id == caller {
                return Err("An agent cannot wait for itself".into());
            }
            let mut ancestor = caller.clone();
            while let Ok(parent) = store.task(&ancestor) {
                if parent.parent == id {
                    return Err("An agent cannot wait for an ancestor".into());
                }
                if parent.parent == ancestor {
                    break;
                }
                ancestor = parent.parent;
            }
            let released = if depth > 0 {
                store.update_task(&caller, |t| {
                    if t.status == "running" {
                        t.status = "waiting".into();
                    }
                })?;
                true
            } else {
                false
            };
            let timeout = input["timeoutMs"].as_u64().unwrap_or(10000).min(30000);
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout);
            loop {
                let task = store.task(id)?;
                if !matches!(
                    task.status.as_str(),
                    "queued" | "running" | "waiting" | "ready"
                ) || std::time::Instant::now() >= deadline
                {
                    if released {
                        store.update_task(&caller, |t| {
                            if t.status == "waiting" {
                                t.status = "ready".into();
                            }
                        })?;
                        loop {
                            let own = store.task(&caller)?;
                            if own.status == "running" {
                                break;
                            }
                            if own.status != "ready" || now_ms() >= group.deadline {
                                return Err(
                                    "Child cancelled while waiting for execution capacity".into()
                                );
                            }
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                    }
                    return Ok(json!(task));
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        "cancel" if may_control => Ok(json!(store.cancel_tree(id)?)),
        "resume" if may_control => {
            if group.cancelled || now_ms() >= group.deadline {
                return Err("Parent task is stopped or out of time".into());
            }
            Ok(json!(store.update_task(id, |t| {
                if matches!(t.status.as_str(), "failed" | "interrupted" | "cancelled") {
                    t.status = "queued".into();
                    t.attempt += 1;
                    t.error = None;
                }
            })?))
        }
        "message" if may_control => {
            let text = input["task"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 16000)
                .ok_or("Missing bounded message")?;
            let path = opencowork_runtime::default_config_home()
                .join("turn-control")
                .join(format!("{id}.json"));
            let _lease = opencowork_runtime::ExclusiveLease::acquire(&path.with_extension("lock"))
                .map_err(|e| e.to_string())?;
            let mut messages: Vec<Value> = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            if messages.len() >= 20 {
                return Err("Child steering queue is full".into());
            }
            messages.push(json!({"id":format!("steer-{}-{}",now_ms(),messages.len()),"mode":"steer","status":"pending","text":text}));
            opencowork_runtime::write_json_atomic(&path, &messages).map_err(|e| e.to_string())?;
            Ok(json!({"accepted":true}))
        }
        _ => Err("Unsupported action or caller cannot control this task".into()),
    }
}

use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct TaskInput {
    pub conversation_id: String,
    pub message: String,
    #[serde(default)]
    pub history: Vec<ChatHistoryMessage>,
    #[serde(default)]
    pub attachments: Vec<AttachmentInput>,
    pub skill_name: Option<String>,
    pub skill_args: Option<String>,
    pub resume_from: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TaskStep {
    pub title: String,
    pub status: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub conversation_id: String,
    pub objective: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub input: TaskInput,
    pub trace: Vec<ToolContextMessage>,
    pub steps: Vec<TaskStep>,
    pub artifacts: Vec<String>,
    pub verification: Vec<String>,
    pub response: Option<String>,
    pub error: Option<String>,
}

fn records() -> &'static Mutex<HashMap<String, TaskRecord>> {
    static RECORDS: OnceLock<Mutex<HashMap<String, TaskRecord>>> = OnceLock::new();
    RECORDS.get_or_init(|| {
        let mut records = HashMap::new();
        if let Ok(entries) = fs::read_dir(directory()) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(bytes) = fs::read(entry.path()) {
                    if let Ok(mut task) = serde_json::from_slice::<TaskRecord>(&bytes) {
                        if mark_interrupted(&mut task) {
                            let _ = persist(&task);
                        }
                        records.insert(task.id.clone(), task);
                    }
                }
            }
        }
        Mutex::new(records)
    })
}
fn mark_interrupted(task: &mut TaskRecord) -> bool {
    if task.status != "running" {
        return false;
    }
    task.status = "interrupted".into();
    task.updated_at = Local::now().to_rfc3339();
    task.error = Some("Application restarted. Inspect previous operations before resuming.".into());
    true
}

fn resumed_history(previous: &TaskRecord) -> Vec<ChatHistoryMessage> {
    let mut history = previous.input.history.clone();
    history.push(history_message("user", previous.input.message.clone()));
    history.extend(previous.trace.iter().map(|m| ChatHistoryMessage {
        role: m.role.clone(),
        content: m.content.clone().unwrap_or_default(),
        tool_calls: m.tool_calls.clone(),
        tool_call_id: m.tool_call_id.clone(),
    }));
    if let Some(response) = &previous.response {
        let text = serde_json::from_str::<serde_json::Value>(response)
            .ok()
            .and_then(|v| v["response"].as_str().map(String::from))
            .unwrap_or(response.clone());
        history.push(history_message("assistant", text));
    }
    normalize_history(history)
}
#[cfg(not(test))]
fn directory() -> PathBuf {
    StorageManager::new().get_data_dir().join("agent-tasks")
}
#[cfg(test)]
fn directory() -> PathBuf {
    static TEST_DIR: OnceLock<PathBuf> = OnceLock::new();
    TEST_DIR
        .get_or_init(|| {
            std::env::temp_dir().join(format!(
                "opencowork-task-tests-{}",
                next_background_task_id()
            ))
        })
        .clone()
}
fn persist(task: &TaskRecord) -> Result<(), String> {
    let dir = directory();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", task.id));
    let temp = path.with_extension("tmp");
    fs::write(&temp, serde_json::to_vec(task).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
}
fn update(id: &str, f: impl FnOnce(&mut TaskRecord)) -> Result<(), String> {
    let mut guard = records().lock().map_err(|e| e.to_string())?;
    if let Some(record) = guard.get_mut(id) {
        let mut next = record.clone();
        f(&mut next);
        next.updated_at = Local::now().to_rfc3339();
        persist(&next)?;
        *record = next;
    }
    Ok(())
}
pub(super) fn trace(id: &str, message: ToolContextMessage) -> Result<(), String> {
    update(id, |record| record.trace.push(message))
}
pub(super) fn artifact(id: &str, path: &str) -> Result<(), String> {
    update(id, |r| {
        if !r.artifacts.contains(&path.to_string()) {
            r.artifacts.push(path.into());
        }
    })
}
pub(super) fn plan(id: &str, steps: Vec<TaskStep>) -> Result<(), String> {
    if steps
        .iter()
        .any(|s| !["pending", "running", "completed"].contains(&s.status.as_str()))
    {
        return Err("Invalid step status".into());
    }
    update(id, |r| r.steps = steps)
}
pub(super) fn verification(id: &str, evidence: String) -> Result<(), String> {
    update(id, |r| r.verification.push(evidence))
}

#[tauri::command]
pub async fn list_agent_tasks(conversation_id: Option<String>) -> Result<Vec<TaskRecord>, String> {
    let mut result: Vec<_> = records()
        .lock()
        .map_err(|e| e.to_string())?
        .values()
        .filter(|r| {
            conversation_id
                .as_ref()
                .is_none_or(|id| &r.conversation_id == id)
        })
        .cloned()
        .collect();
    result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    // Keep the sidebar payload bounded. Full history is retrieved only when needed.
    for task in &mut result {
        task.input.history.clear();
        task.trace.clear();
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_agent_task(id: String) -> Result<TaskRecord, String> {
    records()
        .lock()
        .map_err(|e| e.to_string())?
        .get(&id)
        .cloned()
        .ok_or("Task not found".into())
}

#[tauri::command]
pub async fn start_agent_task(
    mut input: TaskInput,
    app_handle: AppHandle,
) -> Result<String, String> {
    let objective;
    if let Some(id) = &input.resume_from {
        let previous = get_agent_task(id.clone()).await?;
        if previous.status == "running" {
            return Err("Task is still running".into());
        }
        if previous.conversation_id != input.conversation_id {
            return Err("Task belongs to another conversation".into());
        }
        objective = previous.objective.clone();
        input.history = resumed_history(&previous);
        input.message = format!("Resume the original objective: {}\nPrevious status: {}. Inspect files/processes and previous evidence before retrying any write, command or external action; an interrupted operation may already have completed. Continue remaining work and verify results.\nAdditional instruction: {}", objective, previous.status, input.message);
        input.skill_name = previous.input.skill_name;
        input.skill_args = Some(input.message.clone());
        input.attachments = previous.input.attachments;
    } else {
        objective = input.message.clone();
    }
    let id = format!(
        "task-{}-{}",
        Local::now().timestamp_micros(),
        BACKGROUND_TASK_COUNTER.fetch_add(1, Ordering::SeqCst)
    );
    let now = Local::now().to_rfc3339();
    let task = TaskRecord {
        id: id.clone(),
        conversation_id: input.conversation_id.clone(),
        objective,
        status: "running".into(),
        created_at: now.clone(),
        updated_at: now,
        input: input.clone(),
        trace: vec![],
        steps: vec![],
        artifacts: vec![],
        verification: vec![],
        response: None,
        error: None,
    };
    {
        let mut guard = records().lock().map_err(|e| e.to_string())?;
        if guard
            .values()
            .any(|r| r.conversation_id == input.conversation_id && r.status == "running")
        {
            return Err("This conversation already has a running task".into());
        }
        persist(&task)?;
        guard.insert(id.clone(), task);
    }
    // Register before returning the id: an immediate Stop must not be lost.
    register_cancel_token(&app_handle.state::<AppState>(), &id).await;
    let task_id = id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app_handle.state::<AppState>();
        let result = if let Some(skill) = input.skill_name {
            invoke_skill(
                skill,
                input.skill_args,
                Some(input.history),
                Some(input.attachments),
                Some(task_id.clone()),
                app_handle.clone(),
                state,
            )
            .await
        } else {
            chat_with_assistant(
                input.message,
                Some(input.history),
                Some(input.attachments),
                Some(task_id.clone()),
                app_handle.clone(),
                state,
            )
            .await
        };
        // Background commands and MCP servers belong to this task, including on failure.
        let cleanup = process::stop_owner(&task_id).await;
        mcp::shutdown(&task_id).await;
        clear_cancel_token(&app_handle.state::<AppState>(), &task_id).await;
        let result = result.and_then(|response| cleanup.map(|_| response));
        let persisted = update(&task_id, |r| match result {
            Ok(response) => {
                r.status = "completed".into();
                r.response = Some(response);
            }
            Err(error) => {
                r.status = if error.contains(REQUEST_CANCELLED_ERROR) {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
                r.error = Some(error.clone());
                let mut context = r.trace.clone();
                if context.iter().any(|m| {
                    m.content
                        .as_ref()
                        .is_some_and(|s| s.chars().count() > MAX_PERSISTED_TOOL_CONTEXT_CHARS)
                }) {
                    let history: Vec<_> = context
                        .iter()
                        .map(|m| ChatHistoryMessage {
                            role: m.role.clone(),
                            content: m.content.clone().unwrap_or_default(),
                            tool_calls: m.tool_calls.clone(),
                            tool_call_id: m.tool_call_id.clone(),
                        })
                        .collect();
                    if let Ok(archive) = archive_context(&history) {
                        for message in &mut context {
                            if let Some(content) = &mut message.content {
                                let chars: Vec<_> = content.chars().collect();
                                if chars.len() > MAX_PERSISTED_TOOL_CONTEXT_CHARS {
                                    *content = format!(
                                        "{}\n[Full result in {}]\n{}",
                                        chars[..1500].iter().collect::<String>(),
                                        archive.display(),
                                        chars[chars.len() - 1500..].iter().collect::<String>()
                                    );
                                }
                            }
                        }
                    }
                }
                r.response = serde_json::to_string(&ChatResponse {
                    response: format!("Task stopped: {error}\nPrevious tool operations are recorded. Inspect their outcomes before resuming."),
                    tool_context: context, active_skill: r.input.skill_name.clone(),
                }).ok();
            }
        });
        if let Err(error) = persisted {
            eprintln!("Unable to persist task completion: {error}");
        }
        let _ = app_handle.emit("agent-task-changed", &task_id);
    });
    Ok(id)
}

#[tauri::command]
pub async fn preview_task_artifact(
    task_id: String,
    path: String,
) -> Result<serde_json::Value, String> {
    let task = get_agent_task(task_id).await?;
    if !task.artifacts.contains(&path) {
        return Err("File is not an artifact of this task".into());
    }
    let storage = StorageManager::new();
    let access = build_tool_access(&storage.load_config()?, &storage, None);
    let path = ensure_path_allowed(&access, &path)?;
    let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 {
        return Err("Preview supports files up to 8 MiB".into());
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    let mime = match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        _ => None,
    };
    if let Some(mime) = mime {
        use base64::Engine;
        return Ok(
            serde_json::json!({"kind":"image","content":format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))}),
        );
    }
    let text =
        String::from_utf8(bytes).map_err(|_| "This binary file cannot be previewed as text")?;
    Ok(
        serde_json::json!({"kind":"text","content":text.chars().take(100_000).collect::<String>(),"truncated":text.chars().count()>100_000}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_task_journal_restart_and_resume_preserve_unknown_outcomes() {
        let id = next_background_task_id();
        let input = TaskInput {
            conversation_id: id.clone(),
            message: "create exactly one file".into(),
            history: vec![],
            attachments: vec![],
            skill_name: None,
            skill_args: None,
            resume_from: None,
        };
        let task = TaskRecord {
            id: id.clone(),
            conversation_id: id.clone(),
            objective: input.message.clone(),
            status: "running".into(),
            created_at: "now".into(),
            updated_at: "now".into(),
            input,
            trace: vec![],
            steps: vec![],
            artifacts: vec![],
            verification: vec![],
            response: None,
            error: None,
        };
        persist(&task).unwrap();
        records().lock().unwrap().insert(id.clone(), task);
        trace(
            &id,
            ToolContextMessage {
                role: "assistant".into(),
                content: None,
                tool_call_id: None,
                tool_calls: Some(vec![ToolCallInfo {
                    id: "write".into(),
                    name: "Write".into(),
                    arguments: "{}".into(),
                }]),
            },
        )
        .unwrap();
        plan(
            &id,
            vec![TaskStep {
                title: "create file".into(),
                status: "running".into(),
            }],
        )
        .unwrap();
        // Simulate a crash after the intent was saved, before a tool result was saved.
        let mut restored: TaskRecord =
            serde_json::from_slice(&fs::read(directory().join(format!("{id}.json"))).unwrap())
                .unwrap();
        assert!(mark_interrupted(&mut restored));
        assert_eq!(restored.status, "interrupted");
        assert!(!mark_interrupted(&mut restored));
        restored.response = Some(r#"{"response":"partial response","tool_context":[]}"#.into());
        let history = resumed_history(&restored);
        assert_eq!(history[0].content, "create exactly one file");
        assert_eq!(history[2].tool_call_id.as_deref(), Some("write"));
        assert!(history[2].content.contains("outcome unknown"));
        assert_eq!(history.last().unwrap().content, "partial response");
        assert_eq!(restored.steps[0].title, "create file");
        let other = list_agent_tasks(Some("other-conversation".into()))
            .await
            .unwrap();
        assert!(other.is_empty());
    }
}

use opencowork_runtime::{write_json_atomic, ExclusiveLease, TokenUsage};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskGroup {
    #[serde(default)]
    pub detached: bool,
    pub id: String,
    pub session: String,
    pub cwd: PathBuf,
    pub host: u32,
    pub concurrency: usize,
    pub max_depth: usize,
    pub max_tokens: u64,
    pub root_tokens: u64,
    pub deadline: u64,
    pub cancelled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildTask {
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub fork_from: Option<String>,
    #[serde(default)]
    pub resumed_from: Option<String>,
    #[serde(default)]
    pub started_at: Option<u64>,
    #[serde(default)]
    pub finished_at: Option<u64>,
    #[serde(default)]
    pub affected_paths: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
    pub id: String,
    pub group: String,
    pub parent: String,
    pub depth: usize,
    pub name: String,
    pub prompt: String,
    pub model: Option<String>,
    pub writable: bool,
    pub status: String,
    pub result: String,
    pub error: Option<String>,
    pub usage: TokenUsage,
    pub token_limit: u64,
    pub attempt: u32,
    pub created_at: u64,
}

#[derive(Clone)]
pub struct TaskStore {
    root: PathBuf,
}

impl TaskStore {
    pub fn new(home: &Path) -> Self {
        Self {
            root: home.join("subagents"),
        }
    }
    pub fn valid(id: &str) -> bool {
        !id.is_empty()
            && id.len() <= 160
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    }
    fn path(&self, kind: &str, id: &str) -> Result<PathBuf, String> {
        if !Self::valid(id) {
            return Err("Invalid subagent ID".into());
        }
        Ok(self.root.join(kind).join(format!("{id}.json")))
    }
    pub fn group(&self, id: &str) -> Result<TaskGroup, String> {
        read(&self.path("groups", id)?)
    }
    pub fn task(&self, id: &str) -> Result<ChildTask, String> {
        read(&self.path("tasks", id)?)
    }
    pub fn groups(&self) -> Vec<TaskGroup> {
        list(&self.root.join("groups"))
    }
    pub fn tasks(&self, group: &str) -> Vec<ChildTask> {
        let mut tasks = self.all_tasks();
        tasks.retain(|t| t.group == group);
        tasks.sort_by_key(|t| t.created_at);
        tasks
    }
    pub fn all_tasks(&self) -> Vec<ChildTask> {
        list(&self.root.join("tasks"))
    }
    pub fn fork_session(&self, source: &str, target: &str, cwd: &Path) -> Result<(), String> {
        if !Self::valid(source) || !Self::valid(target) {
            return Err("Invalid fork session ID".into());
        }
        let home = self.root.parent().unwrap();
        let sessions = opencowork_runtime::SessionStore::new(home.join("sessions"));
        if sessions.load(target).is_ok() {
            return Ok(());
        }
        let journal = std::fs::read(home.join("turn-journals").join(format!("{source}.json")))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
        let mut session = if let Some(journal) = journal {
            let mut session: opencowork_runtime::Session =
                serde_json::from_value(if journal.get("session").is_some() {
                    journal["session"].clone()
                } else {
                    journal.clone()
                })
                .map_err(|e| e.to_string())?;
            let pending: Vec<opencowork_runtime::ContentBlock> =
                serde_json::from_value(journal["pending"].clone()).unwrap_or_default();
            if !pending.is_empty() {
                session
                    .messages
                    .push(opencowork_runtime::ConversationMessage::assistant(
                        pending, None,
                    ));
            }
            session
        } else {
            sessions.load(source).map_err(|e| e.to_string())?
        };
        if session.workspace.as_deref() != Some(cwd.to_string_lossy().as_ref()) {
            return Err("Fork source belongs to another workspace".into());
        }
        let completed: std::collections::BTreeSet<_> = session
            .messages
            .iter()
            .flat_map(|m| m.blocks.iter())
            .filter_map(|b| match b {
                opencowork_runtime::ContentBlock::ToolResult { tool_use_id, .. } => {
                    Some(tool_use_id.clone())
                }
                _ => None,
            })
            .collect();
        let unresolved: Vec<_> = session
            .messages
            .iter()
            .flat_map(|m| m.blocks.iter())
            .filter_map(|b| match b {
                opencowork_runtime::ContentBlock::ToolUse { id, name, .. }
                    if !completed.contains(id) =>
                {
                    Some((id.clone(), name.clone()))
                }
                _ => None,
            })
            .collect();
        for (id, name) in unresolved {
            session.messages.push(opencowork_runtime::ConversationMessage::tool_result(id,name,"Fork boundary: parent action is pending or its outcome is unknown. Inspect state before repeating it.",true));
        }
        sessions
            .save_named(target, &session.with_id(target))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    pub fn resume_into(
        &self,
        id: &str,
        group: &TaskGroup,
        parent: &str,
    ) -> Result<ChildTask, String> {
        let previous = self.task(id)?;
        let old_group = self.group(&previous.group)?;
        if old_group.cwd != group.cwd {
            return Err("Task belongs to another workspace".into());
        }
        if !matches!(
            previous.status.as_str(),
            "failed" | "interrupted" | "cancelled"
        ) {
            return Err("Only interrupted, failed or cancelled tasks can be resumed".into());
        }
        if previous.group == group.id {
            return self.update_task(id, |t| {
                t.status = "queued".into();
                t.attempt += 1;
                t.error = None;
                t.finished_at = None;
            });
        }
        if !old_group.cancelled {
            return Err("Stop the former root task before moving its child".into());
        }
        let mut task = previous.clone();
        task.id = format!("{}-resume-{}", group.id, previous.name);
        task.parent = parent.into();
        task.group = group.id.clone();
        task.depth = 1;
        task.status = "queued".into();
        task.attempt += 1;
        task.resumed_from = Some(previous.id.clone());
        task.error = None;
        task.usage = Default::default();
        task.started_at = None;
        task.finished_at = None;
        task.created_at = now_ms();
        task.token_limit = (group.max_tokens / (group.concurrency as u64 + 1)).max(1);
        let sessions =
            opencowork_runtime::SessionStore::new(self.root.parent().unwrap().join("sessions"));
        if let Ok(session) = sessions.load(id) {
            sessions
                .save_named(&task.id, &session.with_id(&task.id))
                .map_err(|e| e.to_string())?;
        }
        self.create(&task)
    }
    pub fn cancel_tree(&self, id: &str) -> Result<ChildTask, String> {
        let root = self.task(id)?;
        let tasks = self.tasks(&root.group);
        let mut descendants = vec![id.to_owned()];
        let mut index = 0;
        while index < descendants.len() {
            for task in &tasks {
                if task.parent == descendants[index] && !descendants.contains(&task.id) {
                    descendants.push(task.id.clone());
                }
            }
            index += 1;
        }
        for child in descendants.into_iter().rev() {
            self.update_task(&child, |task| {
                if matches!(
                    task.status.as_str(),
                    "queued" | "running" | "waiting" | "ready"
                ) {
                    task.status = "cancelled".into();
                }
            })?;
        }
        self.task(id)
    }
    pub fn create_group(&self, group: &TaskGroup) -> Result<(), String> {
        let path = self.path("groups", &group.id)?;
        let _lease = lock(&path)?;
        if path.exists() {
            return Err("Subagent task group already exists".into());
        }
        write_json_atomic(&path, group).map_err(|e| e.to_string())
    }
    pub fn create(&self, task: &ChildTask) -> Result<ChildTask, String> {
        let path = self.path("tasks", &task.id)?;
        let _lease = lock(&path)?;
        if path.exists() {
            let previous: ChildTask = read(&path)?;
            if previous.prompt == task.prompt
                && previous.parent == task.parent
                && previous.model == task.model
                && previous.writable == task.writable
            {
                return Ok(previous);
            }
            return Err("This task name is already in use; inspect the existing task or choose another name".into());
        }
        write_json_atomic(&path, task).map_err(|e| e.to_string())?;
        Ok(task.clone())
    }
    pub fn update_task(
        &self,
        id: &str,
        f: impl FnOnce(&mut ChildTask),
    ) -> Result<ChildTask, String> {
        let path = self.path("tasks", id)?;
        let _lease = lock(&path)?;
        let mut task: ChildTask = read(&path)?;
        let previous_status = task.status.clone();
        f(&mut task);
        if task.status != previous_status {
            if task.status == "running" && task.started_at.is_none() {
                task.started_at = Some(now_ms());
            }
            if matches!(
                task.status.as_str(),
                "completed" | "failed" | "cancelled" | "interrupted"
            ) {
                task.finished_at = Some(now_ms());
            }
        }
        write_json_atomic(&path, &task).map_err(|e| e.to_string())?;
        Ok(task)
    }
    pub fn update_group(&self, id: &str, f: impl FnOnce(&mut TaskGroup)) -> Result<(), String> {
        let path = self.path("groups", id)?;
        let _lease = lock(&path)?;
        let mut group = read(&path)?;
        f(&mut group);
        write_json_atomic(&path, &group).map_err(|e| e.to_string())
    }
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn list<T: serde::de::DeserializeOwned>(root: &Path) -> Vec<T> {
    std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| read(&e.path()).ok())
        .collect()
}
fn lock(path: &Path) -> Result<ExclusiveLease, String> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        match ExclusiveLease::acquire(&path.with_extension("lock")) {
            Ok(lease) => return Ok(lease),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::PermissionDenied
                    || matches!(error.raw_os_error(), Some(5 | 32 | 33)) =>
            {
                if std::time::Instant::now() >= deadline {
                    return Err("Subagent state is busy; retry".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

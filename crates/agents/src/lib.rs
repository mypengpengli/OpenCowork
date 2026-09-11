use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: String,
    pub role: String,
    pub model: String,
    #[serde(default)]
    pub capabilities: BTreeSet<String>,
    pub max_parallel_tasks: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub summary: String,
    #[serde(default)]
    pub required_capabilities: BTreeSet<String>,
    pub priority: TaskPriority,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    pub task_id: String,
    pub agent_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SchedulingPolicy {
    pub max_parallel_agents: usize,
}

pub struct TeamPlanner {
    policy: SchedulingPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHandoff {
    pub handoff_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub agent_model: Option<String>,
    pub summary: String,
    #[serde(default)]
    pub context_files: Vec<String>,
    #[serde(default = "default_handoff_status")]
    pub status: HandoffStatus,
    pub session_id: Option<String>,
    pub result_summary: Option<String>,
    pub error: Option<String>,
    pub created_at_unix_ms: u128,
    pub started_at_unix_ms: Option<u128>,
    pub completed_at_unix_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentHandoffStore {
    root: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

fn default_handoff_status() -> HandoffStatus {
    HandoffStatus::Pending
}

#[derive(Debug, thiserror::Error)]
pub enum AgentStoreError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

impl TeamPlanner {
    #[must_use]
    pub fn new(policy: SchedulingPolicy) -> Self {
        Self { policy }
    }

    #[must_use]
    pub fn assign(&self, agents: &[AgentProfile], tasks: &[AgentTask]) -> Vec<Assignment> {
        let mut load = BTreeMap::<String, usize>::new();
        let mut assignments = Vec::new();
        let mut ordered_tasks = tasks.to_vec();
        ordered_tasks.sort_by(|left, right| right.priority.cmp(&left.priority));

        for task in ordered_tasks {
            let Some(agent) = agents
                .iter()
                .filter(|agent| {
                    task.required_capabilities
                        .iter()
                        .all(|capability| agent.capabilities.contains(capability))
                })
                .filter(|agent| {
                    load.get(&agent.id).copied().unwrap_or(0) < agent.max_parallel_tasks
                })
                .min_by_key(|agent| load.get(&agent.id).copied().unwrap_or(0))
            else {
                continue;
            };

            if self.policy.max_parallel_agents > 0
                && load.len() >= self.policy.max_parallel_agents
                && !load.contains_key(&agent.id)
            {
                continue;
            }

            *load.entry(agent.id.clone()).or_insert(0) += 1;
            assignments.push(Assignment {
                task_id: task.id,
                agent_id: agent.id.clone(),
                reason: format!("matched role `{}` with required capabilities", agent.role),
            });
        }

        assignments
    }
}

impl AgentHandoffStore {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn save(&self, handoff: &AgentHandoff) -> Result<(), AgentStoreError> {
        fs::create_dir_all(&self.root)?;
        opencowork_runtime::write_json_atomic(
            &self.root.join(format!("{}.json", handoff.handoff_id)),
            &serde_json::to_value(handoff)?,
        )?;
        Ok(())
    }

    pub fn create(
        &self,
        assignment: &Assignment,
        agent_model: Option<String>,
        summary: impl Into<String>,
        context_files: Vec<String>,
    ) -> Result<AgentHandoff, AgentStoreError> {
        let handoff = AgentHandoff {
            handoff_id: format!(
                "handoff-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
            task_id: assignment.task_id.clone(),
            agent_id: assignment.agent_id.clone(),
            agent_model,
            summary: summary.into(),
            context_files,
            status: HandoffStatus::Pending,
            session_id: None,
            result_summary: None,
            error: None,
            created_at_unix_ms: now_ms(),
            started_at_unix_ms: None,
            completed_at_unix_ms: None,
        };
        self.save(&handoff)?;
        Ok(handoff)
    }

    pub fn load(&self, handoff_id: &str) -> Result<AgentHandoff, AgentStoreError> {
        Ok(serde_json::from_str(&fs::read_to_string(
            self.root.join(format!("{handoff_id}.json")),
        )?)?)
    }

    pub fn list(&self) -> Result<Vec<AgentHandoff>, AgentStoreError> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }

        let mut handoffs: Vec<AgentHandoff> = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            handoffs.push(serde_json::from_str(&fs::read_to_string(path)?)?);
        }
        handoffs.sort_by(|left, right| right.created_at_unix_ms.cmp(&left.created_at_unix_ms));
        Ok(handoffs)
    }

    pub fn claim_next(&self, agent_id: &str) -> Result<Option<AgentHandoff>, AgentStoreError> {
        let _claim =
            match opencowork_runtime::ExclusiveLease::acquire(&self.root.join("claim.lock")) {
                Ok(lease) => lease,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::PermissionDenied
                    ) =>
                {
                    return Ok(None)
                }
                Err(e) => return Err(e.into()),
            };
        let mut handoffs = self.list()?;
        handoffs.sort_by(|left, right| left.created_at_unix_ms.cmp(&right.created_at_unix_ms));
        let Some(next) = handoffs.into_iter().find(|handoff| {
            handoff.agent_id == agent_id && handoff.status == HandoffStatus::Pending
        }) else {
            return Ok(None);
        };

        self.mark_running(&next.handoff_id, next.session_id.clone())
            .map(Some)
    }

    pub fn mark_running(
        &self,
        handoff_id: &str,
        session_id: Option<String>,
    ) -> Result<AgentHandoff, AgentStoreError> {
        let session_id = session_id.clone();
        self.update(handoff_id, |handoff| {
            handoff.status = HandoffStatus::Running;
            handoff.session_id = session_id.clone();
            handoff.started_at_unix_ms = Some(now_ms());
            handoff.completed_at_unix_ms = None;
            handoff.error = None;
            handoff.result_summary = None;
        })
    }

    pub fn mark_completed(
        &self,
        handoff_id: &str,
        session_id: Option<String>,
        result_summary: impl Into<String>,
    ) -> Result<AgentHandoff, AgentStoreError> {
        let session_id = session_id.clone();
        let result_summary = result_summary.into();
        self.update(handoff_id, |handoff| {
            handoff.status = HandoffStatus::Completed;
            handoff.session_id = session_id.clone();
            handoff.result_summary = Some(result_summary.clone());
            handoff.error = None;
            handoff.completed_at_unix_ms = Some(now_ms());
        })
    }

    pub fn mark_failed(
        &self,
        handoff_id: &str,
        session_id: Option<String>,
        error: impl Into<String>,
    ) -> Result<AgentHandoff, AgentStoreError> {
        let session_id = session_id.clone();
        let error = error.into();
        self.update(handoff_id, |handoff| {
            handoff.status = HandoffStatus::Failed;
            handoff.session_id = session_id.clone();
            handoff.error = Some(error.clone());
            handoff.completed_at_unix_ms = Some(now_ms());
        })
    }

    fn update(
        &self,
        handoff_id: &str,
        mut updater: impl FnMut(&mut AgentHandoff),
    ) -> Result<AgentHandoff, AgentStoreError> {
        let mut handoff = self.load(handoff_id)?;
        updater(&mut handoff);
        self.save(&handoff)?;
        Ok(handoff)
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::{
        AgentHandoffStore, AgentProfile, AgentTask, HandoffStatus, SchedulingPolicy, TaskPriority,
        TeamPlanner,
    };
    use std::collections::BTreeSet;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("opencowork-{label}-{stamp}"))
    }

    #[test]
    fn concurrent_workers_claim_only_once() {
        let root = temp_dir("atomic-claim");
        let store = AgentHandoffStore::new(&root);
        store
            .create(
                &super::Assignment {
                    task_id: "task".into(),
                    agent_id: "worker".into(),
                    reason: "test".into(),
                },
                None,
                "single claim",
                vec![],
            )
            .unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
        let threads = (0..4)
            .map(|_| {
                let store = store.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    store.claim_next("worker")
                })
            })
            .collect::<Vec<_>>();
        let claims = threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter_map(|r| r.ok().flatten())
            .count();
        assert_eq!(claims, 1);
        assert!(store.claim_next("worker").unwrap().is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn assigns_tasks_to_capable_agents() {
        let agents = vec![
            AgentProfile {
                id: "architect".to_string(),
                role: "architect".to_string(),
                model: "gpt-5.4".to_string(),
                capabilities: BTreeSet::from([
                    "design".to_string(),
                    "runtime".to_string(),
                    "planning".to_string(),
                ]),
                max_parallel_tasks: 2,
            },
            AgentProfile {
                id: "worker".to_string(),
                role: "worker".to_string(),
                model: "gpt-5.4-mini".to_string(),
                capabilities: BTreeSet::from(["implementation".to_string(), "testing".to_string()]),
                max_parallel_tasks: 2,
            },
        ];
        let tasks = vec![
            AgentTask {
                id: "design-loop".to_string(),
                summary: "design main runtime loop".to_string(),
                required_capabilities: BTreeSet::from([
                    "design".to_string(),
                    "runtime".to_string(),
                ]),
                priority: TaskPriority::Critical,
            },
            AgentTask {
                id: "tests".to_string(),
                summary: "write tests".to_string(),
                required_capabilities: BTreeSet::from(["testing".to_string()]),
                priority: TaskPriority::Normal,
            },
        ];

        let planner = TeamPlanner::new(SchedulingPolicy {
            max_parallel_agents: 2,
        });
        let assignments = planner.assign(&agents, &tasks);
        assert_eq!(assignments.len(), 2);
        assert_eq!(assignments[0].agent_id, "architect");
        assert_eq!(assignments[1].agent_id, "worker");
    }

    #[test]
    fn persists_agent_handoffs() {
        let root = temp_dir("handoffs");
        let store = AgentHandoffStore::new(&root);
        let handoff = store
            .create(
                &super::Assignment {
                    task_id: "prompt-engine".to_string(),
                    agent_id: "architect".to_string(),
                    reason: "matched".to_string(),
                },
                Some("gpt-5.4".to_string()),
                "Review prompt engine redesign",
                vec!["crates/runtime/src/prompt.rs".to_string()],
            )
            .expect("create handoff");

        assert!(handoff.handoff_id.starts_with("handoff-"));
        assert_eq!(handoff.status, HandoffStatus::Pending);
        let running = store
            .claim_next("architect")
            .expect("claim")
            .expect("pending handoff");
        assert_eq!(running.status, HandoffStatus::Running);
        let completed = store
            .mark_completed(&running.handoff_id, Some("session-1".to_string()), "done")
            .expect("complete");
        assert_eq!(completed.status, HandoffStatus::Completed);
        assert_eq!(completed.session_id.as_deref(), Some("session-1"));
        assert_eq!(store.list().expect("list").len(), 1);

        let _ = fs::remove_dir_all(root);
    }
}

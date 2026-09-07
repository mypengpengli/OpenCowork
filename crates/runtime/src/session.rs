use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::Path;

use crate::usage::TokenUsage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: String,
    },
    ToolResult {
        tool_use_id: String,
        tool_name: String,
        output: String,
        is_error: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: MessageRole,
    pub blocks: Vec<ContentBlock>,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextCollapseCommit {
    pub summary: String,
    pub removed_message_count: usize,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextCollapseSnapshot {
    pub summary: String,
    pub collapsed_spans: usize,
    pub collapsed_messages: usize,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SessionMemoryState {
    #[serde(default)]
    pub initialized: bool,
    #[serde(default)]
    pub last_triggered_message_count: usize,
    #[serde(default)]
    pub last_summarized_message_count: usize,
    #[serde(default)]
    pub tokens_at_last_extraction: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_started_at_unix_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub messages: Vec<ConversationMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_session_memory: Option<String>,
    #[serde(default, skip_serializing_if = "SessionMemoryState::is_empty")]
    pub session_memory_state: SessionMemoryState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_collapse_archive: Vec<ConversationMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_collapse_commits: Vec<ContextCollapseCommit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_collapse_snapshot: Option<ContextCollapseSnapshot>,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

impl Session {
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: 1,
            id: None,
            messages: Vec::new(),
            current_session_memory: None,
            session_memory_state: SessionMemoryState::default(),
            context_collapse_archive: Vec::new(),
            context_collapse_commits: Vec::new(),
            context_collapse_snapshot: None,
        }
    }

    #[must_use]
    pub fn from_messages(messages: Vec<ConversationMessage>) -> Self {
        Self {
            version: 1,
            id: None,
            messages,
            current_session_memory: None,
            session_memory_state: SessionMemoryState::default(),
            context_collapse_archive: Vec::new(),
            context_collapse_commits: Vec::new(),
            context_collapse_snapshot: None,
        }
    }

    #[must_use]
    pub fn with_messages(&self, messages: Vec<ConversationMessage>) -> Self {
        Self {
            version: self.version,
            id: self.id.clone(),
            messages,
            current_session_memory: self.current_session_memory.clone(),
            session_memory_state: self.session_memory_state.clone(),
            context_collapse_archive: self.context_collapse_archive.clone(),
            context_collapse_commits: self.context_collapse_commits.clone(),
            context_collapse_snapshot: self.context_collapse_snapshot.clone(),
        }
    }

    #[must_use]
    pub fn with_id(&self, id: impl Into<String>) -> Self {
        let mut session = self.clone();
        session.id = Some(id.into());
        session
    }

    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    #[must_use]
    pub fn active_context_summary(&self) -> Option<&str> {
        self.context_collapse_snapshot
            .as_ref()
            .map(|snapshot| snapshot.summary.as_str())
    }

    pub fn save_to_path(&self, path: impl AsRef<Path>) -> Result<(), SessionError> {
        let path = path.as_ref();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = path.with_extension(format!("pending-{}-{stamp}", std::process::id()));
        let result = (|| {
            fs::write(&temporary, serde_json::to_string_pretty(self)?)?;
            fs::rename(&temporary, path)?;
            Ok::<_, SessionError>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        Ok(())
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, SessionError> {
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionMemoryState {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl ConversationMessage {
    #[must_use]
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            blocks: vec![ContentBlock::Text { text: text.into() }],
            usage: None,
        }
    }

    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            blocks: vec![ContentBlock::Text { text: text.into() }],
            usage: None,
        }
    }

    #[must_use]
    pub fn assistant(blocks: Vec<ContentBlock>, usage: Option<TokenUsage>) -> Self {
        Self {
            role: MessageRole::Assistant,
            blocks,
            usage,
        }
    }

    #[must_use]
    pub fn tool_result(
        tool_use_id: impl Into<String>,
        tool_name: impl Into<String>,
        output: impl Into<String>,
        is_error: bool,
    ) -> Self {
        Self {
            role: MessageRole::Tool,
            blocks: vec![ContentBlock::ToolResult {
                tool_use_id: tool_use_id.into(),
                tool_name: tool_name.into(),
                output: output.into(),
                is_error,
            }],
            usage: None,
        }
    }

    #[must_use]
    pub fn first_text(&self) -> Option<&str> {
        self.blocks.iter().find_map(|block| match block {
            ContentBlock::Text { text } if !text.trim().is_empty() => Some(text.as_str()),
            _ => None,
        })
    }
}

impl Display for MessageRole {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::System => "system",
                Self::User => "user",
                Self::Assistant => "assistant",
                Self::Tool => "tool",
            }
        )
    }
}

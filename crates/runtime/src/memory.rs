use crate::prompt::skill_instruction_sources_from_session;
use crate::{
    estimate_session_tokens, ContentBlock, ConversationMessage, ConversationRuntime, HookRunner,
    InstructionSource, PermissionMode, PermissionPolicy, ProviderBackedApiClient,
    RuntimeToolDefinition, Session, ToolError, ToolExecutor,
};
use opencowork_api::{InputMessage, ProviderClient, ProviderRequest};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const MEMORY_ENTRYPOINT_NAME: &str = "MEMORY.md";
const SESSION_MEMORY_FILE_NAME: &str = "summary.md";
const SESSION_MEMORY_STATE_FILE_NAME: &str = "state.json";
const MAX_ENTRYPOINT_LINES: usize = 200;
const MAX_ENTRYPOINT_BYTES: usize = 25_000;
const FRONTMATTER_MAX_LINES: usize = 30;
const MAX_MEMORY_FILES: usize = 200;
const MAX_MEMORY_LINES: usize = 200;
const MAX_MEMORY_BYTES: usize = 4_096;
const MAX_TOTAL_SESSION_MEMORY_CHARS: usize = 48_000;
const MAX_SESSION_MEMORY_SECTION_CHARS: usize = 8_000;
const MAX_SESSION_MEMORY_SECTION_TOKENS: usize = 2_000;
const MAX_TOTAL_SESSION_MEMORY_TOKENS: usize = 12_000;
const MAX_RELEVANT_MEMORIES: usize = 5;
const MEMORY_SELECTOR_SYSTEM_PROMPT: &str = r#"You are selecting memory files that will be useful as the agent processes the user's request.

Return a JSON object with exactly one key:
{"selected_memories":["file-a.md","file-b.md"]}

Rules:
- Select at most 5 files.
- Only include files that are clearly useful for the user's request.
- If none are clearly useful, return an empty array.
- Prefer warnings, constraints, historical decisions, and implementation notes.
- Do not include MEMORY.md. It is already loaded separately.
"#;
const SESSION_MEMORY_EXTRACTION_SYSTEM_PROMPT: &str = r#"You update session memory notes for an agent coding session.

Use the available edit tool to update the session-memory file in place.
- Preserve every existing section header line exactly.
- Preserve every italic section-description line exactly.
- Do not add new sections.
- Do not mention note-taking, extraction, or these instructions.
- Stop after the required edits are complete.
"#;
const DEFAULT_SESSION_MEMORY_TEMPLATE: &str = r#"
# Session Title
_A short and distinctive 5-10 word descriptive title for the session. Super info dense, no filler_

# Current State
_What is actively being worked on right now? Pending tasks not yet completed. Immediate next steps._

# Task specification
_What did the user ask to build? Any design decisions or other explanatory context_

# Files and Functions
_What are the important files? In short, what do they contain and why are they relevant?_

# Workflow
_What bash commands are usually run and in what order? How to interpret their output if not obvious?_

# Errors & Corrections
_Errors encountered and how they were fixed. What did the user correct? What approaches failed and should not be tried again?_

# Codebase and System Documentation
_What are the important system components? How do they work/fit together?_

# Learnings
_What has worked well? What has not? What to avoid? Do not duplicate items from other sections_

# Key results
_If the user asked a specific output such as an answer to a question, a table, or other document, repeat the exact result here_

# Worklog
_Step by step, what was attempted, done? Very terse summary for each step_
"#;
const DEFAULT_SESSION_MEMORY_UPDATE_PROMPT: &str = r#"IMPORTANT: This message and these instructions are NOT part of the actual user conversation. Do NOT include any references to "note-taking", "session notes extraction", or these update instructions in the notes content.

Based on the user conversation above (EXCLUDING this note-taking instruction message as well as system prompt, claude.md entries, or any past session summaries), update the session notes file.

The file {{notesPath}} has already been read for you. Here are its current contents:
<current_notes_content>
{{currentNotes}}
</current_notes_content>

Your ONLY task is to use the Edit tool to update the notes file, then stop. You may make multiple edits as needed. Do not call any other tools.

CRITICAL RULES FOR EDITING:
- The file must maintain its exact structure with all sections, headers, and italic descriptions intact
- NEVER modify, delete, or add section headers (the lines starting with `#`)
- NEVER modify or delete the italic _section description_ lines
- The italic _section description_ lines are template instructions and must be preserved exactly
- ONLY update the actual content that appears BELOW the italic _section descriptions_ within each existing section
-- Do NOT add any new sections, summaries, or information outside the existing structure
- Do NOT reference this note-taking process or instructions anywhere in the notes
- It's OK to skip updating a section if there are no substantial new insights to add
- Write DETAILED, INFO-DENSE content for each section
- For "Key results", include the complete, exact output the user requested
- Do not include information that's already in the CLAUDE.md files included in the context
- Keep each section under ~{{sectionTokenLimit}} tokens/words
- Focus on actionable, specific information that would help someone understand or recreate the work discussed in the conversation
- IMPORTANT: Always update "Current State" to reflect the most recent work

Use the Edit tool with file_path: {{notesPath}}

REMEMBER: Use the Edit tool and stop. Do not continue after the edits. Do not add code fences.
"#;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PersistedSessionMemoryState {
    initialized: bool,
    last_triggered_message_count: usize,
    last_summarized_message_count: usize,
    tokens_at_last_extraction: usize,
}

#[derive(Debug, serde::Deserialize)]
struct SessionMemoryEditToolInput {
    file_path: String,
    old_string: String,
    new_string: String,
}

struct SessionMemoryEditToolExecutor {
    memory_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryCandidate {
    filename: String,
    path: PathBuf,
    relative_path: String,
    title: Option<String>,
    description: Option<String>,
    memory_type: Option<String>,
    score: usize,
    modified_ms: u128,
}

#[must_use]
pub fn project_memory_root(config_home: &Path, cwd: &Path) -> PathBuf {
    config_home
        .join("projects")
        .join(project_slug(cwd))
        .join("memory")
}

#[must_use]
pub fn project_memory_entrypoint(config_home: &Path, cwd: &Path) -> PathBuf {
    project_memory_root(config_home, cwd).join(MEMORY_ENTRYPOINT_NAME)
}

#[must_use]
pub fn project_team_memory_root(config_home: &Path, cwd: &Path) -> PathBuf {
    project_memory_root(config_home, cwd).join("team")
}

#[must_use]
pub fn project_team_memory_entrypoint(config_home: &Path, cwd: &Path) -> PathBuf {
    project_team_memory_root(config_home, cwd).join(MEMORY_ENTRYPOINT_NAME)
}

#[must_use]
pub fn project_session_memory_path(config_home: &Path, cwd: &Path, session: &Session) -> PathBuf {
    config_home
        .join("projects")
        .join(project_slug(cwd))
        .join(session.id().unwrap_or("current"))
        .join("session-memory")
        .join(SESSION_MEMORY_FILE_NAME)
}

#[must_use]
pub fn project_session_memory_state_path(
    config_home: &Path,
    cwd: &Path,
    session: &Session,
) -> PathBuf {
    config_home
        .join("projects")
        .join(project_slug(cwd))
        .join(session.id().unwrap_or("current"))
        .join("session-memory")
        .join(SESSION_MEMORY_STATE_FILE_NAME)
}

pub fn discover_project_memory_source(
    cwd: &Path,
    config_home: &Path,
    max_instruction_tokens: usize,
) -> Result<Option<InstructionSource>, std::io::Error> {
    if max_instruction_tokens == 0 {
        return Ok(None);
    }
    let path = project_memory_entrypoint(config_home, cwd);
    let Some(content) = read_optional_text(&path)? else {
        return Ok(None);
    };
    let content = truncate_entrypoint_content(&content);
    let content = truncate_chars(
        &format!(
            "Persistent project memory entrypoint. Use this as durable background knowledge for the workspace.\n\n{}",
            normalize_block(&content)
        ),
        max_instruction_tokens
            .saturating_mul(4)
            .min(MAX_ENTRYPOINT_BYTES.saturating_add(512)),
    );
    Ok(Some(InstructionSource {
        label: path.display().to_string(),
        content,
        priority: 18_500,
    }))
}

pub fn discover_team_memory_source(
    cwd: &Path,
    config_home: &Path,
    max_instruction_tokens: usize,
) -> Result<Option<InstructionSource>, std::io::Error> {
    if max_instruction_tokens == 0 {
        return Ok(None);
    }
    let path = project_team_memory_entrypoint(config_home, cwd);
    let Some(content) = read_optional_text(&path)? else {
        return Ok(None);
    };
    let content = truncate_entrypoint_content(&content);
    let content = truncate_chars(
        &format!(
            "Persistent team memory entrypoint. Use this as shared team context for the workspace.\n\n{}",
            normalize_block(&content)
        ),
        max_instruction_tokens
            .saturating_mul(4)
            .min(MAX_ENTRYPOINT_BYTES.saturating_add(512)),
    );
    Ok(Some(InstructionSource {
        label: path.display().to_string(),
        content,
        priority: 18_250,
    }))
}

pub fn load_current_session_memory_source(
    session: &Session,
    cwd: &Path,
    config_home: &Path,
    max_instruction_tokens: usize,
) -> Result<Option<InstructionSource>, std::io::Error> {
    if max_instruction_tokens == 0 {
        return Ok(None);
    }
    let content = match session.current_session_memory.as_deref() {
        Some(content) if !content.trim().is_empty() => Some(content.to_string()),
        _ => read_optional_text(&project_session_memory_path(config_home, cwd, session))?,
    };
    let Some(content) = content.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let content = truncate_chars(
        &format!(
            "Current session memory. Treat these as working notes for this conversation and prefer the actual transcript if there is any conflict.\n\n{}",
            normalize_block(&content)
        ),
        max_instruction_tokens
            .saturating_mul(4)
            .min(MAX_TOTAL_SESSION_MEMORY_CHARS),
    );
    Ok(Some(InstructionSource {
        label: "current-session-memory".to_string(),
        content,
        priority: 18_000,
    }))
}

pub fn hydrate_current_session_memory(
    session: &Session,
    cwd: &Path,
    config_home: &Path,
) -> Result<Session, std::io::Error> {
    let mut hydrated = session.clone();
    let memory_path = project_session_memory_path(config_home, cwd, session);
    if let Some(content) =
        read_optional_text(&memory_path)?.filter(|value| !value.trim().is_empty())
    {
        hydrated.current_session_memory = Some(content);
    }
    if let Some(persisted_state) = read_persisted_session_memory_state(
        &project_session_memory_state_path(config_home, cwd, session),
    )? {
        hydrated.session_memory_state.initialized = persisted_state.initialized;
        hydrated.session_memory_state.last_triggered_message_count =
            persisted_state.last_triggered_message_count;
        hydrated.session_memory_state.last_summarized_message_count =
            persisted_state.last_summarized_message_count;
        hydrated.session_memory_state.tokens_at_last_extraction =
            persisted_state.tokens_at_last_extraction;
    }

    if let (Some(started), Ok(metadata)) = (
        hydrated.session_memory_state.extraction_started_at_unix_ms,
        fs::metadata(&memory_path),
    ) {
        let completed_after_start = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_millis() >= started)
            .unwrap_or(false);
        if completed_after_start {
            hydrated.session_memory_state.extraction_started_at_unix_ms = None;
        }
    }

    Ok(hydrated)
}

pub fn refresh_current_session_memory(
    session: &mut Session,
    cwd: &Path,
    config_home: &Path,
) -> Result<Option<PathBuf>, std::io::Error> {
    let Some(content) = render_current_session_memory(session, cwd) else {
        session.current_session_memory = None;
        let _ = fs::remove_file(project_session_memory_path(config_home, cwd, session));
        let _ = fs::remove_file(project_session_memory_state_path(config_home, cwd, session));
        return Ok(None);
    };
    let path = project_session_memory_path(config_home, cwd, session);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, &content)?;
    session.current_session_memory = Some(content);
    persist_session_memory_state(session, cwd, config_home)?;
    Ok(Some(path))
}

pub fn refresh_current_session_memory_with_provider<P: ProviderClient>(
    session: &mut Session,
    cwd: &Path,
    config_home: &Path,
    provider: &mut P,
    model: &str,
) -> Result<Option<PathBuf>, std::io::Error> {
    let path = project_session_memory_path(config_home, cwd, session);
    let current_notes = ensure_session_memory_file(session, cwd, config_home)?;
    let prompt = build_session_memory_update_prompt(&current_notes, &path, cwd);
    let transcript_session = session.clone();
    let mut extractor = ConversationRuntime::new(
        transcript_session,
        ProviderBackedApiClient::new(provider, model),
        SessionMemoryEditToolExecutor {
            memory_path: path.clone(),
        },
        PermissionPolicy::new(PermissionMode::ReadOnly),
        vec![SESSION_MEMORY_EXTRACTION_SYSTEM_PROMPT.to_string()],
        HookRunner::default(),
    )
    .with_max_iterations(8);
    let extraction = extractor.run_turn(prompt, None);

    let fallback_content = extraction
        .ok()
        .and_then(|summary| {
            summary
                .assistant_messages
                .iter()
                .filter_map(ConversationMessage::first_text)
                .map(strip_code_fences)
                .map(|value| normalize_block(&value))
                .find(|value| !value.trim().is_empty())
        })
        .or_else(|| render_current_session_memory(session, cwd))
        .unwrap_or(current_notes);
    let content = read_optional_text(&path)?
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback_content);
    session.current_session_memory = Some(content);
    persist_session_memory_state(session, cwd, config_home)?;
    Ok(Some(path))
}

fn ensure_session_memory_file(
    session: &Session,
    cwd: &Path,
    config_home: &Path,
) -> Result<String, std::io::Error> {
    let path = project_session_memory_path(config_home, cwd, session);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = session
        .current_session_memory
        .clone()
        .or_else(|| read_optional_text(&path).ok().flatten())
        .unwrap_or_else(|| load_session_memory_template(cwd));
    if read_optional_text(&path)?.is_none() {
        fs::write(&path, &content)?;
    }
    Ok(content)
}

fn persist_session_memory_state(
    session: &mut Session,
    cwd: &Path,
    config_home: &Path,
) -> Result<(), std::io::Error> {
    session.session_memory_state.initialized = true;
    session.session_memory_state.tokens_at_last_extraction = estimate_session_tokens(session);
    if !has_tool_calls_in_last_assistant_turn(session) {
        session.session_memory_state.last_summarized_message_count = session.messages.len();
    }

    let path = project_session_memory_state_path(config_home, cwd, session);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let persisted = PersistedSessionMemoryState {
        initialized: session.session_memory_state.initialized,
        last_triggered_message_count: session.session_memory_state.last_triggered_message_count,
        last_summarized_message_count: session.session_memory_state.last_summarized_message_count,
        tokens_at_last_extraction: session.session_memory_state.tokens_at_last_extraction,
    };
    let serialized = serde_json::to_string_pretty(&persisted).map_err(std::io::Error::other)?;
    fs::write(path, serialized)?;
    Ok(())
}

fn read_persisted_session_memory_state(
    path: &Path,
) -> Result<Option<PersistedSessionMemoryState>, std::io::Error> {
    let Some(content) = read_optional_text(path)? else {
        return Ok(None);
    };
    serde_json::from_str(&content)
        .map(Some)
        .map_err(std::io::Error::other)
}

impl ToolExecutor for SessionMemoryEditToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        if tool_name != "edit_file" {
            return Err(ToolError::new(format!(
                "unsupported session-memory tool `{tool_name}`"
            )));
        }
        let input = serde_json::from_str::<SessionMemoryEditToolInput>(input)
            .map_err(|error| ToolError::new(format!("invalid edit_file input: {error}")))?;
        let requested_path = PathBuf::from(&input.file_path);
        if requested_path != self.memory_path {
            return Err(ToolError::new(format!(
                "only edit_file on `{}` is allowed",
                self.memory_path.display()
            )));
        }

        let content = fs::read_to_string(&self.memory_path)
            .map_err(|error| ToolError::new(error.to_string()))?;
        if input.old_string.is_empty() {
            return Err(ToolError::new(
                "edit_file requires a non-empty old_string for session memory extraction",
            ));
        }
        if !content.contains(&input.old_string) {
            return Err(ToolError::new("old_string not found"));
        }
        let updated = content.replacen(&input.old_string, &input.new_string, 1);
        fs::write(&self.memory_path, &updated)
            .map_err(|error| ToolError::new(error.to_string()))?;
        Ok(json!({
            "file_path": self.memory_path.display().to_string(),
            "updated": true,
        })
        .to_string())
    }

    fn definitions(&self) -> Vec<RuntimeToolDefinition> {
        vec![RuntimeToolDefinition {
            name: "edit_file".to_string(),
            description: format!(
                "Edit the session-memory file in place. Only `{}` is allowed.",
                self.memory_path.display()
            ),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string" },
                    "old_string": { "type": "string" },
                    "new_string": { "type": "string" }
                },
                "required": ["file_path", "old_string", "new_string"]
            }),
            required_permission: PermissionMode::ReadOnly,
        }]
    }
}

#[must_use]
pub fn should_report_instruction_file(
    label: &str,
    cwd: &Path,
    config_home: &Path,
    session: &Session,
) -> bool {
    if label == "available-skills"
        || label == "current-session-memory"
        || label.starts_with("skill:")
        || label.starts_with("conditional-skill:")
        || label.starts_with("relevant-memory:")
    {
        return false;
    }

    let path = PathBuf::from(label);
    let memory_root = project_memory_root(config_home, cwd);
    let session_memory_path = project_session_memory_path(config_home, cwd, session);
    !path.starts_with(memory_root) && path != session_memory_path
}

pub fn discover_relevant_memory_sources(
    cwd: &Path,
    config_home: &Path,
    session: &Session,
    user_query: Option<&str>,
    max_instruction_tokens: usize,
) -> Result<Vec<InstructionSource>, std::io::Error> {
    if max_instruction_tokens == 0 {
        return Ok(Vec::new());
    }
    let Some(user_query) = user_query
        .map(str::trim)
        .filter(|value| !value.is_empty() && !looks_like_ignore_memory_request(value))
    else {
        return Ok(Vec::new());
    };

    let memory_root = project_memory_root(config_home, cwd);
    if !memory_root.exists() {
        return Ok(Vec::new());
    }

    let query_tokens = collect_query_tokens(user_query);
    let touched_tokens = touched_paths_from_session(cwd, session)
        .iter()
        .filter_map(|path| {
            path.strip_prefix(cwd)
                .ok()
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        })
        .flat_map(|value| collect_query_tokens(&value))
        .collect::<Vec<_>>();

    let mut candidates = scan_memory_candidates(
        &memory_root,
        &project_session_memory_path(config_home, cwd, session),
        &query_tokens,
        &touched_tokens,
    )?;

    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| right.modified_ms.cmp(&left.modified_ms))
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });

    let mut remaining_chars = max_instruction_tokens.saturating_mul(4).min(20_000);
    let mut selected = Vec::new();
    for (index, candidate) in candidates
        .into_iter()
        .filter(|candidate| candidate.score > 0)
        .take(MAX_RELEVANT_MEMORIES)
        .enumerate()
    {
        if remaining_chars == 0 {
            break;
        }
        let content = render_relevant_memory_content(&candidate)?;
        let content = truncate_chars(
            &content,
            remaining_chars.min(MAX_MEMORY_BYTES.saturating_add(512)),
        );
        remaining_chars = remaining_chars.saturating_sub(content.chars().count());
        selected.push(InstructionSource {
            label: format!("relevant-memory:{}", candidate.relative_path),
            content,
            priority: 17_000usize.saturating_sub(index),
        });
    }

    Ok(selected)
}

pub fn discover_relevant_memory_sources_with_provider<P: ProviderClient>(
    cwd: &Path,
    config_home: &Path,
    session: &Session,
    user_query: Option<&str>,
    max_instruction_tokens: usize,
    provider: &mut P,
    model: &str,
) -> Result<Vec<InstructionSource>, std::io::Error> {
    let Some(user_query) = user_query
        .map(str::trim)
        .filter(|value| !value.is_empty() && !looks_like_ignore_memory_request(value))
    else {
        return Ok(Vec::new());
    };

    let memory_root = project_memory_root(config_home, cwd);
    if !memory_root.exists() {
        return Ok(Vec::new());
    }

    let query_tokens = collect_query_tokens(user_query);
    let touched_tokens = touched_paths_from_session(cwd, session)
        .iter()
        .filter_map(|path| {
            path.strip_prefix(cwd)
                .ok()
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        })
        .flat_map(|value| collect_query_tokens(&value))
        .collect::<Vec<_>>();

    let mut candidates = scan_memory_candidates(
        &memory_root,
        &project_session_memory_path(config_home, cwd, session),
        &query_tokens,
        &touched_tokens,
    )?;
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let selected_filenames =
        select_memory_candidates_with_provider(provider, model, user_query, &candidates)
            .unwrap_or_else(|_| {
                candidates
                    .iter()
                    .filter(|candidate| candidate.score > 0)
                    .take(MAX_RELEVANT_MEMORIES)
                    .map(|candidate| candidate.filename.clone())
                    .collect::<Vec<_>>()
            });
    let selected_set = selected_filenames.into_iter().collect::<BTreeSet<_>>();
    candidates.retain(|candidate| selected_set.contains(&candidate.filename));
    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| right.modified_ms.cmp(&left.modified_ms))
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });

    let mut remaining_chars = max_instruction_tokens.saturating_mul(4).min(20_000);
    let mut selected = Vec::new();
    for (index, candidate) in candidates
        .into_iter()
        .take(MAX_RELEVANT_MEMORIES)
        .enumerate()
    {
        if remaining_chars == 0 {
            break;
        }
        let content = render_relevant_memory_content(&candidate)?;
        if content.trim().is_empty() {
            continue;
        }
        let content = truncate_chars(
            &content,
            remaining_chars.min(MAX_MEMORY_BYTES.saturating_add(512)),
        );
        remaining_chars = remaining_chars.saturating_sub(content.chars().count());
        selected.push(InstructionSource {
            label: format!("relevant-memory:{}", candidate.relative_path),
            content,
            priority: 17_000usize.saturating_sub(index),
        });
    }

    Ok(selected)
}

#[must_use]
pub fn touched_paths_from_session(cwd: &Path, session: &Session) -> Vec<PathBuf> {
    let mut touched = Vec::new();
    let mut seen = BTreeSet::new();
    for message in &session.messages {
        for path in touched_paths_from_message(cwd, message) {
            if seen.insert(path.clone()) {
                touched.push(path);
            }
        }
    }
    touched
}

#[must_use]
pub fn touched_paths_from_message(cwd: &Path, message: &ConversationMessage) -> Vec<PathBuf> {
    let mut touched = Vec::new();
    for block in &message.blocks {
        match block {
            ContentBlock::ToolUse { name, input, .. } => {
                if let Some(path) = path_from_tool_payload(cwd, name, input, false) {
                    touched.push(path);
                }
            }
            ContentBlock::ToolResult {
                tool_name,
                output,
                is_error,
                ..
            } => {
                if let Some(path) = path_from_tool_payload(cwd, tool_name, output, *is_error) {
                    touched.push(path);
                }
            }
            ContentBlock::Text { .. } => {}
        }
    }
    touched
}

fn render_current_session_memory(session: &Session, cwd: &Path) -> Option<String> {
    let task_specification = session
        .messages
        .iter()
        .filter(|message| matches!(message.role, crate::MessageRole::User))
        .filter_map(ConversationMessage::first_text)
        .map(normalize_block)
        .filter(|value| !value.is_empty())
        .scan(BTreeSet::new(), |seen, value| {
            if seen.insert(value.clone()) {
                Some(Some(value))
            } else {
                Some(None)
            }
        })
        .flatten()
        .take(6)
        .collect::<Vec<_>>();

    let touched_files = touched_paths_from_session(cwd, session)
        .iter()
        .filter_map(|path| {
            path.strip_prefix(cwd)
                .ok()
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                .or_else(|| Some(path.display().to_string()))
        })
        .take(12)
        .collect::<Vec<_>>();

    let active_skills = skill_instruction_sources_from_session(session)
        .iter()
        .filter_map(|source| source.label.strip_prefix("skill:"))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    let current_state = session
        .active_context_summary()
        .map(normalize_block)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            session
                .messages
                .iter()
                .rev()
                .filter_map(ConversationMessage::first_text)
                .map(normalize_block)
                .find(|value| !value.is_empty())
        });

    let workflow = session
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| match block {
            ContentBlock::ToolUse { name, input, .. } if name.eq_ignore_ascii_case("bash") => {
                Some(format!("bash {input}"))
            }
            _ => None,
        })
        .take(10)
        .collect::<Vec<_>>();

    let errors_and_corrections = session
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| match block {
            ContentBlock::ToolResult {
                tool_name,
                output,
                is_error: true,
                ..
            } => Some(format!(
                "{tool_name}: {}",
                truncate_chars(&normalize_block(output), 400)
            )),
            ContentBlock::Text { text } if looks_like_correction(text) => {
                Some(truncate_chars(&normalize_block(text), 400))
            }
            _ => None,
        })
        .take(8)
        .collect::<Vec<_>>();

    let codebase_docs = session
        .active_context_summary()
        .map(normalize_block)
        .filter(|value| !value.is_empty())
        .into_iter()
        .collect::<Vec<_>>();

    let learnings = active_skills
        .iter()
        .map(|skill| format!("Use loaded skill `{skill}` when relevant."))
        .take(6)
        .collect::<Vec<_>>();

    let key_results = session
        .messages
        .iter()
        .rev()
        .filter(|message| matches!(message.role, crate::MessageRole::Assistant))
        .filter_map(ConversationMessage::first_text)
        .map(normalize_block)
        .find(|value| !value.is_empty());

    let worklog = session
        .messages
        .iter()
        .filter_map(render_worklog_entry)
        .take(20)
        .collect::<Vec<_>>();

    if current_state.is_none()
        && task_specification.is_empty()
        && touched_files.is_empty()
        && workflow.is_empty()
        && errors_and_corrections.is_empty()
        && codebase_docs.is_empty()
        && learnings.is_empty()
        && key_results.is_none()
        && worklog.is_empty()
    {
        return None;
    }

    Some(truncate_chars(
        &fill_session_memory_template(
            &load_session_memory_template(cwd),
            &[
                (
                    "# Session Title",
                    session_memory_title(
                        task_specification.first().map(String::as_str),
                        cwd.file_name().and_then(|value| value.to_str()),
                    ),
                ),
                ("# Current State", current_state.unwrap_or_default()),
                (
                    "# Task specification",
                    join_memory_lines(&task_specification, MAX_SESSION_MEMORY_SECTION_CHARS),
                ),
                (
                    "# Files and Functions",
                    join_memory_lines(&touched_files, MAX_SESSION_MEMORY_SECTION_CHARS),
                ),
                (
                    "# Workflow",
                    join_memory_lines(&workflow, MAX_SESSION_MEMORY_SECTION_CHARS),
                ),
                (
                    "# Errors & Corrections",
                    join_memory_lines(&errors_and_corrections, MAX_SESSION_MEMORY_SECTION_CHARS),
                ),
                (
                    "# Codebase and System Documentation",
                    join_memory_lines(&codebase_docs, MAX_SESSION_MEMORY_SECTION_CHARS),
                ),
                (
                    "# Learnings",
                    join_memory_lines(&learnings, MAX_SESSION_MEMORY_SECTION_CHARS),
                ),
                ("# Key results", key_results.unwrap_or_default()),
                (
                    "# Worklog",
                    join_memory_lines(&worklog, MAX_SESSION_MEMORY_SECTION_CHARS),
                ),
            ],
        ),
        MAX_TOTAL_SESSION_MEMORY_CHARS,
    ))
}

fn scan_memory_candidates(
    memory_root: &Path,
    session_memory_path: &Path,
    query_tokens: &[String],
    touched_tokens: &[String],
) -> Result<Vec<MemoryCandidate>, std::io::Error> {
    let mut files = Vec::new();
    collect_markdown_candidates(memory_root, memory_root, &mut files)?;

    let mut candidates = Vec::new();
    for path in files {
        if path == session_memory_path {
            continue;
        }
        if path.file_name().and_then(|value| value.to_str()) == Some(MEMORY_ENTRYPOINT_NAME) {
            continue;
        }
        let Some(candidate) =
            build_memory_candidate(memory_root, &path, query_tokens, touched_tokens)?
        else {
            continue;
        };
        candidates.push(candidate);
    }

    candidates.sort_by(|left, right| right.modified_ms.cmp(&left.modified_ms));
    candidates.truncate(MAX_MEMORY_FILES);
    Ok(candidates)
}

fn build_memory_candidate(
    memory_root: &Path,
    path: &Path,
    query_tokens: &[String],
    touched_tokens: &[String],
) -> Result<Option<MemoryCandidate>, std::io::Error> {
    let Some(content) = read_optional_text(path)? else {
        return Ok(None);
    };
    let header_preview = take_lines(&content, FRONTMATTER_MAX_LINES);
    let (frontmatter, _) = split_frontmatter(&header_preview);
    let metadata = fs::metadata(path)?;
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |value| value.as_millis());
    let relative_path = path
        .strip_prefix(memory_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let title = frontmatter
        .get("title")
        .or_else(|| frontmatter.get("name"))
        .cloned();
    let description = frontmatter.get("description").cloned();
    let memory_type = frontmatter.get("type").cloned();
    let score = score_memory_candidate(
        &filename,
        &relative_path,
        title.as_deref(),
        description.as_deref(),
        memory_type.as_deref(),
        query_tokens,
        touched_tokens,
    );

    Ok(Some(MemoryCandidate {
        filename,
        path: path.to_path_buf(),
        relative_path,
        title,
        description,
        memory_type,
        score,
        modified_ms,
    }))
}

fn select_memory_candidates_with_provider<P: ProviderClient>(
    provider: &mut P,
    model: &str,
    user_query: &str,
    candidates: &[MemoryCandidate],
) -> Result<Vec<String>, String> {
    let manifest = format_memory_manifest(candidates);
    let response = provider.execute(ProviderRequest {
        max_output_tokens: None,
        model: model.to_string(),
        system_prompt: vec![MEMORY_SELECTOR_SYSTEM_PROMPT.to_string()],
        messages: vec![InputMessage {
            image_urls: Vec::new(),
            role: "user".to_string(),
            content: Some(format!(
                "Query: {user_query}\n\nAvailable memories:\n{manifest}"
            )),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }],
        tools: Vec::new(),
    })?;
    let text = response
        .events
        .iter()
        .filter_map(|event| match event {
            opencowork_api::ProviderEvent::TextDelta(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    parse_selected_memories(&text)
}

fn score_memory_candidate(
    filename: &str,
    relative_path: &str,
    title: Option<&str>,
    description: Option<&str>,
    memory_type: Option<&str>,
    query_tokens: &[String],
    touched_tokens: &[String],
) -> usize {
    let filename_text = filename.to_lowercase();
    let path_text = relative_path.to_lowercase();
    let title_text = title.unwrap_or_default().to_lowercase();
    let description_text = description.unwrap_or_default().to_lowercase();
    let type_text = memory_type.unwrap_or_default().to_lowercase();
    let mut score = 0;

    for token in query_tokens {
        if token.len() < 2 {
            continue;
        }
        if filename_text.contains(token) {
            score += 8;
        }
        if path_text.contains(token) {
            score += 6;
        }
        if title_text.contains(token) {
            score += 6;
        }
        if description_text.contains(token) {
            score += 5;
        }
        if type_text.contains(token) {
            score += 2;
        }
    }

    for token in touched_tokens {
        if token.len() < 2 {
            continue;
        }
        if filename_text.contains(token)
            || path_text.contains(token)
            || description_text.contains(token)
            || title_text.contains(token)
        {
            score += 2;
        }
    }

    score
}

fn render_relevant_memory_content(candidate: &MemoryCandidate) -> Result<String, std::io::Error> {
    let Some(raw_content) = read_optional_text(&candidate.path)? else {
        return Ok(String::new());
    };
    let (content, was_truncated) = truncate_content_by_lines_and_bytes(
        &normalize_block(&raw_content),
        MAX_MEMORY_LINES,
        MAX_MEMORY_BYTES,
    );
    let mut lines = vec![format!(
        "Relevant memory selected for the current request.\nMemory: {}",
        candidate.path.display()
    )];
    if let Some(memory_type) = candidate
        .memory_type
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("Type: {memory_type}"));
    }
    if let Some(title) = candidate
        .title
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("Title: {title}"));
    }
    if let Some(description) = candidate
        .description
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("Description: {description}"));
    }
    lines.push(String::new());
    lines.push(content);
    if was_truncated {
        lines.push(String::new());
        lines.push(format!(
            "> This memory file was truncated (first {MAX_MEMORY_LINES} lines or {MAX_MEMORY_BYTES} bytes). Use read_file to inspect the complete file."
        ));
    }
    Ok(lines.join("\n"))
}

fn format_memory_manifest(candidates: &[MemoryCandidate]) -> String {
    candidates
        .iter()
        .map(|candidate| {
            let type_tag = candidate
                .memory_type
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(|value| format!("[{value}] "))
                .unwrap_or_default();
            let ts = format_memory_timestamp(candidate.modified_ms);
            match candidate.description.as_deref() {
                Some(description) if !description.trim().is_empty() => {
                    format!("- {type_tag}{} ({ts}): {}", candidate.filename, description)
                }
                _ => format!("- {type_tag}{} ({ts})", candidate.filename),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_selected_memories(text: &str) -> Result<Vec<String>, String> {
    let trimmed = text.trim();
    let payload = if trimmed.starts_with('{') {
        trimmed.to_string()
    } else {
        let Some(start) = trimmed.find('{') else {
            return Err("memory selector returned no JSON".to_string());
        };
        let Some(end) = trimmed.rfind('}') else {
            return Err("memory selector returned malformed JSON".to_string());
        };
        trimmed[start..=end].to_string()
    };
    let value = serde_json::from_str::<serde_json::Value>(&payload)
        .map_err(|error| format!("memory selector JSON parse failed: {error}"))?;
    let selected = value
        .get("selected_memories")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "memory selector JSON missing selected_memories".to_string())?;
    Ok(selected
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .take(MAX_RELEVANT_MEMORIES)
        .collect())
}

fn format_memory_timestamp(modified_ms: u128) -> String {
    format!("{modified_ms}")
}

fn split_frontmatter(content: &str) -> (BTreeMap<String, String>, String) {
    let normalized = content.replace("\r\n", "\n");
    let Some(stripped) = normalized.strip_prefix("---\n") else {
        return (BTreeMap::new(), normalized);
    };
    let Some((frontmatter, body)) = stripped.split_once("\n---\n") else {
        return (BTreeMap::new(), normalized);
    };
    let mut values = BTreeMap::new();
    for line in frontmatter.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if value.is_empty() {
            continue;
        }
        values.insert(key.trim().to_string(), value.to_string());
    }
    (values, body.to_string())
}

fn collect_markdown_candidates(
    _root: &Path,
    current: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), std::io::Error> {
    let entries = match fs::read_dir(current) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_markdown_candidates(current, &path, files)?;
            continue;
        }
        if file_type.is_file() && path.extension().and_then(|value| value.to_str()) == Some("md") {
            files.push(path);
        }
    }

    Ok(())
}

fn load_session_memory_template(cwd: &Path) -> String {
    let config_home = std::env::var_os("OPENCOWORK_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".opencowork")))
        .or_else(|| {
            std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".opencowork"))
        })
        .unwrap_or_else(|| cwd.join(".opencowork"));
    let template_path = config_home
        .join("session-memory")
        .join("config")
        .join("template.md");
    fs::read_to_string(template_path)
        .unwrap_or_else(|_| DEFAULT_SESSION_MEMORY_TEMPLATE.to_string())
}

fn load_session_memory_prompt(cwd: &Path) -> String {
    let config_home = std::env::var_os("OPENCOWORK_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".opencowork")))
        .or_else(|| {
            std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".opencowork"))
        })
        .unwrap_or_else(|| cwd.join(".opencowork"));
    let prompt_path = config_home
        .join("session-memory")
        .join("config")
        .join("prompt.md");
    fs::read_to_string(prompt_path)
        .unwrap_or_else(|_| DEFAULT_SESSION_MEMORY_UPDATE_PROMPT.to_string())
}

fn fill_session_memory_template(template: &str, replacements: &[(&str, String)]) -> String {
    let mut lines = Vec::new();
    let replacement_map = replacements
        .iter()
        .map(|(header, value)| (*header, value.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut index = 0;
    let template_lines = template.trim().lines().collect::<Vec<_>>();
    while index < template_lines.len() {
        let line = template_lines[index].trim_end();
        lines.push(line.to_string());
        if line.starts_with("# ") {
            index += 1;
            if let Some(description_line) = template_lines.get(index) {
                lines.push(description_line.trim_end().to_string());
            }
            let content = replacement_map
                .get(line)
                .copied()
                .unwrap_or_default()
                .trim()
                .to_string();
            if !content.is_empty() {
                lines.push(content);
            }
        }
        index += 1;
        if index < template_lines.len() {
            lines.push(String::new());
        }
    }
    normalize_block(&lines.join("\n"))
}

fn build_session_memory_update_prompt(
    current_notes: &str,
    notes_path: &Path,
    cwd: &Path,
) -> String {
    let prompt_template = load_session_memory_prompt(cwd);
    let section_sizes = analyze_section_sizes(current_notes);
    let total_tokens = rough_token_count_estimation(current_notes);
    let section_reminders = generate_section_reminders(&section_sizes, total_tokens);
    substitute_template_variables(
        &(prompt_template + &section_reminders),
        &[
            ("currentNotes", current_notes.to_string()),
            ("notesPath", notes_path.display().to_string()),
            (
                "sectionTokenLimit",
                MAX_SESSION_MEMORY_SECTION_TOKENS.to_string(),
            ),
        ],
    )
}

fn analyze_section_sizes(content: &str) -> BTreeMap<String, usize> {
    let mut sections = BTreeMap::new();
    let mut current_section = String::new();
    let mut current_content = Vec::new();

    for line in content.lines() {
        if line.starts_with("# ") {
            if !current_section.is_empty() && !current_content.is_empty() {
                sections.insert(
                    current_section.clone(),
                    rough_token_count_estimation(&current_content.join("\n")),
                );
            }
            current_section = line.to_string();
            current_content.clear();
        } else {
            current_content.push(line.to_string());
        }
    }

    if !current_section.is_empty() && !current_content.is_empty() {
        sections.insert(
            current_section,
            rough_token_count_estimation(&current_content.join("\n")),
        );
    }

    sections
}

fn generate_section_reminders(
    section_sizes: &BTreeMap<String, usize>,
    total_tokens: usize,
) -> String {
    let over_budget = total_tokens > MAX_TOTAL_SESSION_MEMORY_TOKENS;
    let oversized_sections = section_sizes
        .iter()
        .filter(|(_, tokens)| **tokens > MAX_SESSION_MEMORY_SECTION_TOKENS)
        .map(|(section, tokens)| {
            format!(
                "- \"{section}\" is ~{tokens} tokens (limit: {MAX_SESSION_MEMORY_SECTION_TOKENS})"
            )
        })
        .collect::<Vec<_>>();

    if oversized_sections.is_empty() && !over_budget {
        return String::new();
    }

    let mut parts = Vec::new();
    if over_budget {
        parts.push(format!(
            "\n\nCRITICAL: The session memory file is currently ~{total_tokens} tokens, which exceeds the maximum of {MAX_TOTAL_SESSION_MEMORY_TOKENS} tokens. You MUST condense the file to fit within this budget. Aggressively shorten oversized sections by removing less important details, merging related items, and summarizing older entries. Prioritize keeping \"Current State\" and \"Errors & Corrections\" accurate and detailed."
        ));
    }
    if !oversized_sections.is_empty() {
        parts.push(format!(
            "\n\n{}:\n{}",
            if over_budget {
                "Oversized sections to condense"
            } else {
                "IMPORTANT: The following sections exceed the per-section limit and MUST be condensed"
            },
            oversized_sections.join("\n")
        ));
    }

    parts.join("")
}

fn substitute_template_variables(template: &str, replacements: &[(&str, String)]) -> String {
    replacements
        .iter()
        .fold(template.to_string(), |content, (key, value)| {
            content.replace(&format!("{{{{{key}}}}}"), value)
        })
}

fn rough_token_count_estimation(content: &str) -> usize {
    content.chars().count().div_ceil(4)
}

fn strip_code_fences(content: &str) -> String {
    let trimmed = content.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }
    let body = trimmed
        .trim_start_matches("```markdown")
        .trim_start_matches("```md")
        .trim_start_matches("```");
    body.trim_end_matches("```").trim().to_string()
}

fn session_memory_title(task: Option<&str>, fallback: Option<&str>) -> String {
    let source = task.unwrap_or_else(|| fallback.unwrap_or("OpenCoWork session"));
    source
        .split_whitespace()
        .take(10)
        .collect::<Vec<_>>()
        .join(" ")
}

fn join_memory_lines(values: &[String], max_chars: usize) -> String {
    truncate_chars(
        &values
            .iter()
            .map(|value| format!("- {value}"))
            .collect::<Vec<_>>()
            .join("\n"),
        max_chars,
    )
}

fn has_tool_calls_in_last_assistant_turn(session: &Session) -> bool {
    session
        .messages
        .iter()
        .rev()
        .find(|message| matches!(message.role, crate::MessageRole::Assistant))
        .is_some_and(|message| {
            message
                .blocks
                .iter()
                .any(|block| matches!(block, ContentBlock::ToolUse { .. }))
        })
}

fn render_worklog_entry(message: &ConversationMessage) -> Option<String> {
    let prefix = match message.role {
        crate::MessageRole::User => "User",
        crate::MessageRole::Assistant => "Assistant",
        crate::MessageRole::System => "System",
        crate::MessageRole::Tool => "Tool",
    };
    message
        .first_text()
        .map(normalize_block)
        .filter(|value| !value.is_empty())
        .map(|value| format!("{prefix}: {}", truncate_chars(&value, 240)))
}

fn looks_like_correction(text: &str) -> bool {
    let normalized = text.to_lowercase();
    normalized.contains("不要")
        || normalized.contains("不是这样")
        || normalized.contains("改成")
        || normalized.contains("instead")
        || normalized.contains("don't")
        || normalized.contains("do not")
}

fn truncate_entrypoint_content(raw: &str) -> String {
    let trimmed = raw.trim();
    let (mut truncated, was_truncated) =
        truncate_content_by_lines_and_bytes(trimmed, MAX_ENTRYPOINT_LINES, MAX_ENTRYPOINT_BYTES);
    if was_truncated {
        truncated.push_str(
            "\n\n> WARNING: MEMORY.md was only partially loaded. Keep the index concise and move detail into separate memory files.",
        );
    }
    truncated
}

fn truncate_content_by_lines_and_bytes(
    content: &str,
    max_lines: usize,
    max_bytes: usize,
) -> (String, bool) {
    let mut rendered = String::new();
    let mut used_bytes = 0usize;
    let mut was_truncated = false;

    for (used_lines, line) in content.lines().enumerate() {
        let line_with_newline = if rendered.is_empty() {
            line.to_string()
        } else {
            format!("\n{line}")
        };
        let line_bytes = line_with_newline.len();
        if used_lines >= max_lines || used_bytes + line_bytes > max_bytes {
            was_truncated = true;
            break;
        }
        rendered.push_str(&line_with_newline);
        used_bytes += line_bytes;
    }

    (rendered.trim().to_string(), was_truncated)
}

fn take_lines(content: &str, max_lines: usize) -> String {
    content
        .lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect_query_tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.trim().is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn path_from_tool_payload(
    cwd: &Path,
    tool_name: &str,
    payload: &str,
    is_error: bool,
) -> Option<PathBuf> {
    if is_error || !matches!(tool_name, "read_file" | "write_file" | "edit_file") {
        return None;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) else {
        return None;
    };
    let path = value.get("path").and_then(serde_json::Value::as_str)?;
    Some(normalize_touched_path(cwd, path))
}

fn normalize_touched_path(cwd: &Path, raw: &str) -> PathBuf {
    let candidate = PathBuf::from(raw);
    if candidate.is_absolute() {
        candidate
    } else {
        cwd.join(candidate)
    }
}

fn looks_like_ignore_memory_request(text: &str) -> bool {
    let normalized = text.to_lowercase();
    normalized.contains("ignore memory")
        || normalized.contains("don't use memory")
        || normalized.contains("do not use memory")
        || normalized.contains("不要使用记忆")
        || normalized.contains("忽略记忆")
}

fn read_optional_text(path: &Path) -> Result<Option<String>, std::io::Error> {
    match fs::read_to_string(path) {
        Ok(content) if !content.trim().is_empty() => Ok(Some(content)),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn normalize_block(content: &str) -> String {
    let mut rendered = String::new();
    let mut previous_blank = false;
    for line in content.lines() {
        let trimmed = line.trim_end();
        let is_blank = trimmed.trim().is_empty();
        if is_blank && previous_blank {
            continue;
        }
        rendered.push_str(trimmed);
        rendered.push('\n');
        previous_blank = is_blank;
    }
    rendered.trim().to_string()
}

fn truncate_chars(content: &str, max_chars: usize) -> String {
    if content.chars().count() <= max_chars {
        return content.to_string();
    }
    let mut rendered = content.chars().take(max_chars).collect::<String>();
    rendered.push_str("\n\n[truncated]");
    rendered
}

fn project_slug(cwd: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    cwd.to_string_lossy().replace('\\', "/").hash(&mut hasher);
    let hash = hasher.finish();
    let leaf = cwd
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("workspace");
    let sanitized_leaf = leaf
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let sanitized_leaf = if sanitized_leaf.is_empty() {
        "workspace"
    } else {
        sanitized_leaf.as_str()
    };
    format!("{sanitized_leaf}-{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::{
        discover_project_memory_source, discover_relevant_memory_sources,
        discover_relevant_memory_sources_with_provider, hydrate_current_session_memory,
        load_current_session_memory_source, project_memory_entrypoint, project_memory_root,
        project_session_memory_path, refresh_current_session_memory,
        refresh_current_session_memory_with_provider, touched_paths_from_message,
    };
    use crate::{ContentBlock, ConversationMessage, Session};
    use opencowork_api::{ProviderEvent, ProviderResponse, ReplayProviderClient};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("opencowork-{prefix}-{stamp}"))
    }

    #[test]
    fn discovers_project_memory_entrypoint() {
        let cwd = temp_dir("memory-project");
        let config_home = temp_dir("memory-config");
        let entry = project_memory_entrypoint(&config_home, &cwd);
        fs::create_dir_all(entry.parent().expect("parent")).expect("mkdir");
        fs::write(&entry, "# Memory\nPersistent notes").expect("write memory");

        let source = discover_project_memory_source(&cwd, &config_home, 2_000)
            .expect("discover")
            .expect("source");
        assert!(source.content.contains("Persistent notes"));
        assert_eq!(source.label, entry.display().to_string());

        let _ = fs::remove_dir_all(cwd);
        let _ = fs::remove_dir_all(config_home);
    }

    #[test]
    fn refreshes_and_loads_current_session_memory() {
        let cwd = temp_dir("memory-session");
        let config_home = temp_dir("memory-config");
        fs::create_dir_all(project_memory_root(&config_home, &cwd)).expect("mkdir");
        let mut session = Session::from_messages(vec![
            ConversationMessage::user("Keep the old frontend shell."),
            ConversationMessage::tool_result(
                "tool-1",
                "read_file",
                r#"{"path":"src/main.ts"}"#,
                false,
            ),
        ]);
        let path = refresh_current_session_memory(&mut session, &cwd, &config_home)
            .expect("refresh")
            .expect("path");
        assert!(path.exists());
        assert!(session
            .current_session_memory
            .as_deref()
            .is_some_and(|value| value.contains("Keep the old frontend shell")));

        let source = load_current_session_memory_source(&session, &cwd, &config_home, 2_000)
            .expect("load")
            .expect("source");
        assert!(source.content.contains("# Session Title"));

        let _ = fs::remove_dir_all(cwd);
        let _ = fs::remove_dir_all(config_home);
    }

    #[test]
    fn provider_backed_refresh_persists_session_memory_and_state() {
        let cwd = temp_dir("memory-session-provider");
        let config_home = temp_dir("memory-config");
        fs::create_dir_all(project_memory_root(&config_home, &cwd)).expect("mkdir");
        let mut session = Session::from_messages(vec![
            ConversationMessage::user("Track the runtime memory migration."),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "I updated the runtime prompt and memory pipeline.".to_string(),
                }],
                None,
            ),
        ]);
        session.session_memory_state.last_triggered_message_count = session.messages.len();
        let mut provider = ReplayProviderClient::new(vec![
            ProviderResponse {
                events: vec![
                    ProviderEvent::ToolCall {
                        id: "tool-1".to_string(),
                        name: "edit_file".to_string(),
                        input: serde_json::json!({
                            "file_path": project_session_memory_path(&config_home, &cwd, &session)
                                .display()
                                .to_string(),
                            "old_string": "# Session Title\n_A short and distinctive 5-10 word descriptive title for the session. Super info dense, no filler_",
                            "new_string": "# Session Title\n_A short and distinctive 5-10 word descriptive title for the session. Super info dense, no filler_\n\nRuntime memory migration",
                        }),
                    },
                    ProviderEvent::MessageStop,
                ],
            },
            ProviderResponse {
                events: vec![
                    ProviderEvent::TextDelta("Done".to_string()),
                    ProviderEvent::MessageStop,
                ],
            },
        ]);

        let path = refresh_current_session_memory_with_provider(
            &mut session,
            &cwd,
            &config_home,
            &mut provider,
            "gpt-5.4-mini",
        )
        .expect("provider refresh")
        .expect("path");

        assert!(path.exists());
        let hydrated = hydrate_current_session_memory(
            &Session::from_messages(session.messages.clone()),
            &cwd,
            &config_home,
        )
        .expect("hydrate");
        assert!(hydrated
            .current_session_memory
            .as_deref()
            .is_some_and(|value| value.contains("Runtime memory migration")));
        assert!(hydrated.session_memory_state.initialized);
        assert_eq!(
            hydrated.session_memory_state.last_triggered_message_count,
            session.messages.len()
        );
        assert!(hydrated.session_memory_state.tokens_at_last_extraction > 0);

        let _ = fs::remove_dir_all(cwd);
        let _ = fs::remove_dir_all(config_home);
    }

    #[test]
    fn selects_relevant_memories_from_query() {
        let cwd = temp_dir("memory-recall");
        let config_home = temp_dir("memory-config");
        let memory_root = project_memory_root(&config_home, &cwd);
        fs::create_dir_all(&memory_root).expect("mkdir");
        fs::write(
            memory_root.join("frontend.md"),
            "---\ndescription: Notes about the old frontend shell\n---\nRestore the original cards and toolbar layout.",
        )
        .expect("write frontend");
        fs::write(
            memory_root.join("backend.md"),
            "---\ndescription: Rust worker notes\n---\nFocus on runtime pipelines.",
        )
        .expect("write backend");

        let session = Session::from_messages(vec![ConversationMessage::user(
            "Bring back the old frontend shell.",
        )]);
        let sources = discover_relevant_memory_sources(
            &cwd,
            &config_home,
            &session,
            Some("restore the old frontend shell"),
            3_000,
        )
        .expect("discover relevant");

        assert_eq!(sources.len(), 1);
        assert!(sources[0].label.contains("frontend.md"));

        let _ = fs::remove_dir_all(cwd);
        let _ = fs::remove_dir_all(config_home);
    }

    #[test]
    fn provider_backed_recall_uses_selected_memory_files() {
        let cwd = temp_dir("memory-provider-recall");
        let config_home = temp_dir("memory-config");
        let memory_root = project_memory_root(&config_home, &cwd);
        fs::create_dir_all(&memory_root).expect("mkdir");
        fs::write(
            memory_root.join("frontend.md"),
            "---\ndescription: Old frontend shell notes\n---\nRestore the original cards and toolbar layout.",
        )
        .expect("write frontend");
        fs::write(
            memory_root.join("backend.md"),
            "---\ndescription: Runtime memory and compaction notes\n---\nUse session memory compaction before legacy compact.",
        )
        .expect("write backend");

        let session = Session::from_messages(vec![ConversationMessage::user(
            "Keep the runtime memory pipeline aligned.",
        )]);
        let mut provider = ReplayProviderClient::new(vec![ProviderResponse {
            events: vec![ProviderEvent::TextDelta(
                r#"{"selected_memories":["backend.md"]}"#.to_string(),
            )],
        }]);
        let sources = discover_relevant_memory_sources_with_provider(
            &cwd,
            &config_home,
            &session,
            Some("align the runtime memory pipeline"),
            3_000,
            &mut provider,
            "gpt-5.4-mini",
        )
        .expect("discover relevant with provider");

        assert_eq!(sources.len(), 1);
        assert!(sources[0].label.contains("backend.md"));
        assert!(sources[0]
            .content
            .contains("Use session memory compaction before legacy compact."));

        let _ = fs::remove_dir_all(cwd);
        let _ = fs::remove_dir_all(config_home);
    }

    #[test]
    fn extracts_touched_paths_from_tool_messages() {
        let cwd = PathBuf::from("/tmp/project");
        let message = ConversationMessage::assistant(
            vec![ContentBlock::ToolUse {
                id: "tool-1".to_string(),
                name: "read_file".to_string(),
                input: r#"{"path":"src/main.ts"}"#.to_string(),
            }],
            None,
        );

        let touched = touched_paths_from_message(&cwd, &message);
        assert_eq!(touched, vec![cwd.join("src").join("main.ts")]);
    }
}

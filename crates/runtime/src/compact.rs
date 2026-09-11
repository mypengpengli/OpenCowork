use crate::session::{
    ContentBlock, ContextCollapseCommit, ContextCollapseSnapshot, ConversationMessage, MessageRole,
    Session,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    AUTOCOMPACT_BUFFER_TOKENS, DEFAULT_MAX_TOOL_RESULTS_PER_MESSAGE_CHARS,
    DEFAULT_MAX_TOOL_RESULT_CHARS, DEFAULT_TOOL_RESULT_PREVIEW_CHARS,
};

const COMPACT_CONTINUATION_PREAMBLE: &str =
    "This session is being continued from an earlier conversation that was compacted to stay within the context budget.\n\n";
const COMPACT_RECENT_MESSAGES_NOTE: &str = "Recent messages are preserved verbatim.";
const COMPACT_DIRECT_RESUME_INSTRUCTION: &str =
    "Continue directly from this point. Do not mention the summary or explain that compaction happened.";
pub const MICROCOMPACT_CLEARED_MESSAGE: &str = "[Old tool result content cleared]";
const DEFAULT_SESSION_MEMORY_COMPACT_MIN_TOKENS: usize = 10_000;
const DEFAULT_SESSION_MEMORY_COMPACT_MIN_TEXT_BLOCK_MESSAGES: usize = 5;
const DEFAULT_SESSION_MEMORY_COMPACT_MAX_TOKENS: usize = 40_000;
const SESSION_MEMORY_COMPACT_SECTION_CHARS: usize = 8_000;
const MICROCOMPACT_KEEP_RECENT: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactConfig {
    pub preserve_recent_messages: usize,
    pub max_estimated_tokens: usize,
}

impl Default for CompactConfig {
    fn default() -> Self {
        Self {
            preserve_recent_messages: 6,
            max_estimated_tokens: 200_000 - 20_000 - AUTOCOMPACT_BUFFER_TOKENS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionMemoryCompactConfig {
    pub min_tokens: usize,
    pub min_text_block_messages: usize,
    pub max_tokens: usize,
}

impl Default for SessionMemoryCompactConfig {
    fn default() -> Self {
        Self {
            min_tokens: DEFAULT_SESSION_MEMORY_COMPACT_MIN_TOKENS,
            min_text_block_messages: DEFAULT_SESSION_MEMORY_COMPACT_MIN_TEXT_BLOCK_MESSAGES,
            max_tokens: DEFAULT_SESSION_MEMORY_COMPACT_MAX_TOKENS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResultBudgetConfig {
    pub max_result_chars: usize,
    pub max_message_chars: usize,
    pub preview_chars: usize,
    pub storage_root: Option<PathBuf>,
}

impl Default for ToolResultBudgetConfig {
    fn default() -> Self {
        Self {
            max_result_chars: DEFAULT_MAX_TOOL_RESULT_CHARS,
            max_message_chars: DEFAULT_MAX_TOOL_RESULTS_PER_MESSAGE_CHARS,
            preview_chars: DEFAULT_TOOL_RESULT_PREVIEW_CHARS,
            storage_root: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageCollapseConfig {
    pub preserve_recent_messages: usize,
    pub max_chars: usize,
}

impl Default for MessageCollapseConfig {
    fn default() -> Self {
        Self {
            preserve_recent_messages: 8,
            max_chars: 8_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnipConfig {
    pub preserve_recent_messages: usize,
    pub max_estimated_tokens: usize,
}

impl Default for SnipConfig {
    fn default() -> Self {
        Self {
            preserve_recent_messages: 8,
            max_estimated_tokens: 200_000 - 20_000 - AUTOCOMPACT_BUFFER_TOKENS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnipResult {
    pub snipped_session: Session,
    pub removed_message_count: usize,
    pub removed_estimated_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicrocompactResult {
    pub microcompacted_session: Session,
    pub cleared_tool_results: usize,
    pub saved_estimated_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactResult {
    pub summary: String,
    pub formatted_summary: String,
    pub compacted_session: Session,
    pub removed_message_count: usize,
}

#[must_use]
pub fn estimate_session_tokens(session: &Session) -> usize {
    session.messages.iter().map(estimate_message_tokens).sum()
}

#[must_use]
pub fn should_compact(session: &Session, config: CompactConfig) -> bool {
    let start = compacted_summary_prefix_len(session);
    let compactable = &session.messages[start..];

    compactable.len() > config.preserve_recent_messages
        && compactable
            .iter()
            .map(estimate_message_tokens)
            .sum::<usize>()
            >= config.max_estimated_tokens
}

#[must_use]
pub fn format_compact_summary(summary: &str) -> String {
    let without_analysis = strip_tag_block(summary, "analysis");
    let formatted = if let Some(content) = extract_tag_block(&without_analysis, "summary") {
        without_analysis.replace(
            &format!("<summary>{content}</summary>"),
            &format!("Summary:\n{}", content.trim()),
        )
    } else {
        without_analysis
    };

    collapse_blank_lines(&formatted).trim().to_string()
}

#[must_use]
pub fn get_compact_continuation_message(
    summary: &str,
    suppress_follow_up_questions: bool,
    recent_messages_preserved: bool,
) -> String {
    let mut message = format!(
        "{COMPACT_CONTINUATION_PREAMBLE}{}",
        format_compact_summary(summary)
    );

    if recent_messages_preserved {
        message.push_str("\n\n");
        message.push_str(COMPACT_RECENT_MESSAGES_NOTE);
    }

    if suppress_follow_up_questions {
        message.push('\n');
        message.push_str(COMPACT_DIRECT_RESUME_INSTRUCTION);
    }

    message
}

#[must_use]
pub fn compact_session(session: &Session, config: CompactConfig) -> CompactResult {
    if !should_compact(session, config) {
        return CompactResult {
            summary: session
                .active_context_summary()
                .unwrap_or_default()
                .to_string(),
            formatted_summary: session
                .active_context_summary()
                .map(format_compact_summary)
                .unwrap_or_default(),
            compacted_session: session.clone(),
            removed_message_count: 0,
        };
    }

    if let Some(result) =
        compact_session_with_session_memory(session, config, SessionMemoryCompactConfig::default())
    {
        return result;
    }

    let existing_summary = session
        .active_context_summary()
        .map(ToOwned::to_owned)
        .or_else(|| {
            session
                .messages
                .first()
                .and_then(extract_existing_compacted_summary)
        });
    let compacted_prefix_len = usize::from(
        session
            .messages
            .first()
            .and_then(extract_existing_compacted_summary)
            .is_some(),
    );
    let keep_from = session
        .messages
        .len()
        .saturating_sub(config.preserve_recent_messages);
    let removed = &session.messages[compacted_prefix_len..keep_from];
    let preserved = session.messages[keep_from..].to_vec();
    let new_commit_summary = summarize_messages(removed);
    let summary = merge_compact_summaries(existing_summary.as_deref(), &new_commit_summary);
    let formatted_summary = format_compact_summary(&summary);
    let continuation = get_compact_continuation_message(&summary, true, !preserved.is_empty());
    let removed_estimated_tokens = removed.iter().map(estimate_message_tokens).sum();
    let mut context_collapse_archive = session.context_collapse_archive.clone();
    context_collapse_archive.extend_from_slice(removed);
    let mut context_collapse_commits = session.context_collapse_commits.clone();
    context_collapse_commits.push(ContextCollapseCommit {
        summary: new_commit_summary,
        removed_message_count: removed.len(),
        estimated_tokens: removed_estimated_tokens,
    });
    let context_collapse_snapshot = Some(ContextCollapseSnapshot {
        summary: summary.clone(),
        collapsed_spans: context_collapse_commits.len(),
        collapsed_messages: context_collapse_commits
            .iter()
            .map(|commit| commit.removed_message_count)
            .sum(),
        estimated_tokens: context_collapse_commits
            .iter()
            .map(|commit| commit.estimated_tokens)
            .sum(),
    });

    let mut compacted_messages = vec![ConversationMessage::system(continuation)];
    compacted_messages.extend(preserved);

    CompactResult {
        summary,
        formatted_summary,
        compacted_session: Session {
            version: session.version,
            id: session.id.clone(),
            workspace: session.workspace.clone(),
            delivered_user_messages: session.delivered_user_messages.clone(),
            messages: compacted_messages,
            current_session_memory: session.current_session_memory.clone(),
            session_memory_state: session.session_memory_state.clone(),
            context_collapse_archive,
            context_collapse_commits,
            context_collapse_snapshot,
        },
        removed_message_count: removed.len(),
    }
}

#[must_use]
pub fn budget_session_tool_results(session: &Session, config: ToolResultBudgetConfig) -> Session {
    if config.max_result_chars == 0
        || config.max_message_chars == 0
        || config.max_result_chars >= config.max_message_chars
    {
        return session.clone();
    }

    session.with_messages(
        session
            .messages
            .iter()
            .map(|message| budget_message_tool_results(message, config.clone()))
            .collect(),
    )
}

fn compact_session_with_session_memory(
    session: &Session,
    _compact_config: CompactConfig,
    session_memory_config: SessionMemoryCompactConfig,
) -> Option<CompactResult> {
    let session_memory = session.current_session_memory.as_deref()?.trim();
    if session_memory.is_empty() {
        return None;
    }

    let compacted_prefix_len = compacted_summary_prefix_len(session);
    let last_summarized_index = match session.session_memory_state.last_summarized_message_count {
        0 => session.messages.len().checked_sub(1)?,
        count => count.min(session.messages.len()).saturating_sub(1),
    };
    let keep_from = calculate_session_memory_keep_from(
        session,
        last_summarized_index,
        session_memory_config,
        compacted_prefix_len,
    );
    let removed = &session.messages[compacted_prefix_len..keep_from];
    let preserved = session.messages[keep_from..].to_vec();
    let removed_estimated_tokens = removed.iter().map(estimate_message_tokens).sum();
    let mut context_collapse_archive = session.context_collapse_archive.clone();
    context_collapse_archive.extend_from_slice(removed);
    let (summary, formatted_summary) = build_session_memory_compact_summary(session_memory);
    let continuation = get_compact_continuation_message(&summary, true, !preserved.is_empty());
    let mut compacted_messages = vec![ConversationMessage::system(continuation)];
    compacted_messages.extend(preserved);

    let mut context_collapse_commits = session.context_collapse_commits.clone();
    context_collapse_commits.push(ContextCollapseCommit {
        summary: summary.clone(),
        removed_message_count: removed.len(),
        estimated_tokens: removed_estimated_tokens,
    });
    let context_collapse_snapshot = Some(ContextCollapseSnapshot {
        summary: summary.clone(),
        collapsed_spans: context_collapse_commits.len(),
        collapsed_messages: context_collapse_commits
            .iter()
            .map(|commit| commit.removed_message_count)
            .sum(),
        estimated_tokens: context_collapse_commits
            .iter()
            .map(|commit| commit.estimated_tokens)
            .sum(),
    });

    let mut session_memory_state = session.session_memory_state.clone();
    session_memory_state.last_summarized_message_count = 0;
    session_memory_state.extraction_started_at_unix_ms = None;

    Some(CompactResult {
        summary,
        formatted_summary,
        compacted_session: Session {
            version: session.version,
            id: session.id.clone(),
            workspace: session.workspace.clone(),
            delivered_user_messages: session.delivered_user_messages.clone(),
            messages: compacted_messages,
            current_session_memory: session.current_session_memory.clone(),
            session_memory_state,
            context_collapse_archive,
            context_collapse_commits,
            context_collapse_snapshot,
        },
        removed_message_count: removed.len(),
    })
}

#[must_use]
pub fn microcompact_session_messages(session: &Session) -> MicrocompactResult {
    let compactable_tool_ids = collect_compactable_tool_ids(session);
    let keep_set = compactable_tool_ids
        .iter()
        .rev()
        .take(MICROCOMPACT_KEEP_RECENT)
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    if compactable_tool_ids.len() <= keep_set.len() {
        return MicrocompactResult {
            microcompacted_session: session.clone(),
            cleared_tool_results: 0,
            saved_estimated_tokens: 0,
        };
    }

    let clear_set = compactable_tool_ids
        .into_iter()
        .filter(|tool_use_id| !keep_set.contains(tool_use_id))
        .collect::<std::collections::BTreeSet<_>>();
    let mut cleared_tool_results = 0usize;
    let mut saved_estimated_tokens = 0usize;
    let messages = session
        .messages
        .iter()
        .map(|message| {
            let mut touched = false;
            let blocks = message
                .blocks
                .iter()
                .map(|block| match block {
                    ContentBlock::ToolResult {
                        tool_use_id,
                        tool_name,
                        output,
                        is_error,
                    } if clear_set.contains(tool_use_id)
                        && output != MICROCOMPACT_CLEARED_MESSAGE =>
                    {
                        touched = true;
                        cleared_tool_results += 1;
                        saved_estimated_tokens += estimate_text_tokens(output);
                        ContentBlock::ToolResult {
                            tool_use_id: tool_use_id.clone(),
                            tool_name: tool_name.clone(),
                            output: MICROCOMPACT_CLEARED_MESSAGE.to_string(),
                            is_error: *is_error,
                        }
                    }
                    _ => block.clone(),
                })
                .collect::<Vec<_>>();
            if touched {
                ConversationMessage {
                    role: message.role,
                    blocks,
                    usage: message.usage,
                }
            } else {
                message.clone()
            }
        })
        .collect::<Vec<_>>();

    MicrocompactResult {
        microcompacted_session: session.with_messages(messages),
        cleared_tool_results,
        saved_estimated_tokens,
    }
}

fn build_session_memory_compact_summary(session_memory: &str) -> (String, String) {
    let (truncated_content, was_truncated) = truncate_session_memory_for_compact(session_memory);
    let summary = if was_truncated {
        format!(
            "{truncated_content}\n\nSome session memory sections were truncated for length. Read the full session-memory file if you need the omitted detail."
        )
    } else {
        truncated_content
    };
    let formatted_summary = format_compact_summary(&summary);
    (summary, formatted_summary)
}

fn calculate_session_memory_keep_from(
    session: &Session,
    last_summarized_index: usize,
    config: SessionMemoryCompactConfig,
    floor: usize,
) -> usize {
    if session.messages.is_empty() {
        return 0;
    }

    let mut start_index = last_summarized_index
        .saturating_add(1)
        .clamp(floor, session.messages.len());
    let mut total_tokens = 0usize;
    let mut text_block_messages = 0usize;
    for message in session.messages.iter().skip(start_index) {
        total_tokens += estimate_message_tokens(message);
        if has_text_blocks(message) {
            text_block_messages += 1;
        }
    }

    if total_tokens >= config.max_tokens
        || (total_tokens >= config.min_tokens
            && text_block_messages >= config.min_text_block_messages)
    {
        return adjust_index_to_preserve_tool_pairs(session, start_index, floor);
    }

    for index in (floor..start_index).rev() {
        let message = &session.messages[index];
        total_tokens += estimate_message_tokens(message);
        if has_text_blocks(message) {
            text_block_messages += 1;
        }
        start_index = index;

        if total_tokens >= config.max_tokens {
            break;
        }
        if total_tokens >= config.min_tokens
            && text_block_messages >= config.min_text_block_messages
        {
            break;
        }
    }

    adjust_index_to_preserve_tool_pairs(session, start_index, floor)
}

fn adjust_index_to_preserve_tool_pairs(
    session: &Session,
    start_index: usize,
    floor: usize,
) -> usize {
    if start_index <= floor || start_index >= session.messages.len() {
        return start_index;
    }

    let mut adjusted_index = start_index;
    let mut needed_tool_use_ids = session
        .messages
        .iter()
        .skip(start_index)
        .flat_map(tool_result_ids)
        .collect::<Vec<_>>();
    if needed_tool_use_ids.is_empty() {
        return adjusted_index;
    }

    let tool_uses_in_kept_range = session
        .messages
        .iter()
        .skip(start_index)
        .flat_map(tool_use_ids)
        .collect::<std::collections::BTreeSet<_>>();
    needed_tool_use_ids.retain(|id| !tool_uses_in_kept_range.contains(id));
    if needed_tool_use_ids.is_empty() {
        return adjusted_index;
    }

    let mut remaining = needed_tool_use_ids
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    for index in (floor..start_index).rev() {
        let message = &session.messages[index];
        if !has_tool_use_with_ids(message, &remaining) {
            continue;
        }
        adjusted_index = index;
        for tool_use_id in tool_use_ids(message) {
            remaining.remove(&tool_use_id);
        }
        if remaining.is_empty() {
            break;
        }
    }

    adjusted_index
}

fn has_text_blocks(message: &ConversationMessage) -> bool {
    !matches!(message.role, MessageRole::Tool)
        && message
            .blocks
            .iter()
            .any(|block| matches!(block, ContentBlock::Text { text } if !text.trim().is_empty()))
}

fn collect_compactable_tool_ids(session: &Session) -> Vec<String> {
    session
        .messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, name, .. }
                if matches!(
                    name.as_str(),
                    "read_file"
                        | "write_file"
                        | "edit_file"
                        | "bash"
                        | "grep_search"
                        | "glob_search"
                        | "web_fetch"
                        | "web_search"
                ) =>
            {
                Some(id.clone())
            }
            _ => None,
        })
        .collect()
}

fn tool_result_ids(message: &ConversationMessage) -> Vec<String> {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.clone()),
            _ => None,
        })
        .collect()
}

fn tool_use_ids(message: &ConversationMessage) -> Vec<String> {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect()
}

fn has_tool_use_with_ids(
    message: &ConversationMessage,
    tool_use_ids: &std::collections::BTreeSet<String>,
) -> bool {
    message
        .blocks
        .iter()
        .any(|block| matches!(block, ContentBlock::ToolUse { id, .. } if tool_use_ids.contains(id)))
}

fn truncate_session_memory_for_compact(content: &str) -> (String, bool) {
    let mut output_lines = Vec::new();
    let mut current_header = String::new();
    let mut current_section_lines = Vec::new();
    let mut was_truncated = false;

    for line in content.lines() {
        if line.starts_with("# ") {
            let flush = flush_session_memory_section(
                &current_header,
                &current_section_lines,
                SESSION_MEMORY_COMPACT_SECTION_CHARS,
            );
            output_lines.extend(flush.lines);
            was_truncated |= flush.was_truncated;
            current_header = line.to_string();
            current_section_lines.clear();
        } else {
            current_section_lines.push(line.to_string());
        }
    }

    let flush = flush_session_memory_section(
        &current_header,
        &current_section_lines,
        SESSION_MEMORY_COMPACT_SECTION_CHARS,
    );
    output_lines.extend(flush.lines);
    was_truncated |= flush.was_truncated;

    (output_lines.join("\n").trim().to_string(), was_truncated)
}

struct FlushSectionResult {
    lines: Vec<String>,
    was_truncated: bool,
}

fn flush_session_memory_section(
    header: &str,
    section_lines: &[String],
    max_chars: usize,
) -> FlushSectionResult {
    if header.is_empty() {
        return FlushSectionResult {
            lines: section_lines.to_vec(),
            was_truncated: false,
        };
    }

    let content = section_lines.join("\n");
    if content.chars().count() <= max_chars {
        let mut lines = vec![header.to_string()];
        lines.extend(section_lines.to_vec());
        return FlushSectionResult {
            lines,
            was_truncated: false,
        };
    }

    let mut lines = vec![header.to_string()];
    let mut used_chars = 0usize;
    for line in section_lines {
        let line_chars = if lines.len() == 1 {
            line.chars().count()
        } else {
            line.chars().count().saturating_add(1)
        };
        if used_chars + line_chars > max_chars {
            break;
        }
        lines.push(line.clone());
        used_chars += line_chars;
    }
    lines.push(String::new());
    lines.push("[truncated for compact]".to_string());

    FlushSectionResult {
        lines,
        was_truncated: true,
    }
}

fn estimate_text_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

#[must_use]
pub fn collapse_session_messages(session: &Session, config: MessageCollapseConfig) -> Session {
    if config.max_chars == 0 {
        return session.clone();
    }

    let prefix_len = compacted_summary_prefix_len(session);
    let collapse_until = session
        .messages
        .len()
        .saturating_sub(config.preserve_recent_messages);

    session.with_messages(
        session
            .messages
            .iter()
            .enumerate()
            .map(|(index, message)| {
                if index < prefix_len || index >= collapse_until {
                    return message.clone();
                }
                collapse_message_text(message, config.max_chars)
            })
            .collect(),
    )
}

#[must_use]
pub fn snip_session_messages(session: &Session, config: SnipConfig) -> SnipResult {
    let total_tokens = estimate_session_tokens(session);
    let prefix_len = compacted_summary_prefix_len(session);
    let preserve_from = session
        .messages
        .len()
        .saturating_sub(config.preserve_recent_messages);
    if total_tokens <= config.max_estimated_tokens || preserve_from <= prefix_len + 1 {
        return SnipResult {
            snipped_session: session.clone(),
            removed_message_count: 0,
            removed_estimated_tokens: 0,
        };
    }

    let max_removable = preserve_from.saturating_sub(prefix_len);
    let mut removed_count = 0;
    let mut removed_estimated_tokens = 0;
    let mut projected_tokens = total_tokens;

    for offset in 1..=max_removable {
        let removed_message = &session.messages[prefix_len + offset - 1];
        let removed_tokens = estimate_message_tokens(removed_message);
        removed_count = offset;
        removed_estimated_tokens += removed_tokens;
        projected_tokens = projected_tokens.saturating_sub(removed_tokens);

        let next_index = prefix_len + offset;
        let safe_restart = session
            .messages
            .get(next_index)
            .is_none_or(|message| matches!(message.role, MessageRole::User | MessageRole::System));
        if safe_restart && projected_tokens <= config.max_estimated_tokens {
            break;
        }
    }

    if removed_count == 0 {
        return SnipResult {
            snipped_session: session.clone(),
            removed_message_count: 0,
            removed_estimated_tokens: 0,
        };
    }

    let mut messages = session.messages[..prefix_len].to_vec();
    messages.extend_from_slice(&session.messages[prefix_len + removed_count..]);

    SnipResult {
        snipped_session: session.with_messages(messages),
        removed_message_count: removed_count,
        removed_estimated_tokens,
    }
}

#[must_use]
pub fn budget_message_tool_results(
    message: &ConversationMessage,
    config: ToolResultBudgetConfig,
) -> ConversationMessage {
    let mut blocks = message
        .blocks
        .iter()
        .map(|block| match block {
            ContentBlock::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            } => ContentBlock::ToolResult {
                tool_use_id: tool_use_id.clone(),
                tool_name: tool_name.clone(),
                output: if output.chars().count() > config.max_result_chars {
                    build_budgeted_tool_result_output(tool_use_id, tool_name, output, &config)
                } else {
                    output.clone()
                },
                is_error: *is_error,
            },
            _ => block.clone(),
        })
        .collect::<Vec<_>>();

    enforce_message_tool_result_budget(&mut blocks, config);

    ConversationMessage {
        role: message.role,
        blocks,
        usage: message.usage,
    }
}

#[must_use]
pub fn compacted_summary_prefix_len(session: &Session) -> usize {
    usize::from(
        session
            .messages
            .first()
            .and_then(extract_existing_compacted_summary)
            .is_some(),
    )
}

fn collapse_message_text(message: &ConversationMessage, max_chars: usize) -> ConversationMessage {
    let blocks = message
        .blocks
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text } => ContentBlock::Text {
                text: collapse_text_block(text, max_chars),
            },
            _ => block.clone(),
        })
        .collect();

    ConversationMessage {
        role: message.role,
        blocks,
        usage: message.usage,
    }
}

fn build_budgeted_tool_result_output(
    tool_use_id: &str,
    tool_name: &str,
    output: &str,
    config: &ToolResultBudgetConfig,
) -> String {
    let metadata = extract_tool_result_metadata(output);
    let persisted_path = persist_tool_result(tool_use_id, output, config.storage_root.as_deref());
    serde_json::to_string(&json!({
        "_context_budget": {
            "tool": tool_name,
            "truncated": true,
            "original_chars": output.chars().count(),
            "original_lines": line_count(output),
        },
        "metadata": metadata,
        "persisted_path": persisted_path.as_ref().map(|path| path.display().to_string()),
        "preview": preview_head_tail(output, config.preview_chars),
    }))
    .unwrap_or_else(|_| truncate_summary(output, config.preview_chars))
}

fn enforce_message_tool_result_budget(blocks: &mut [ContentBlock], config: ToolResultBudgetConfig) {
    loop {
        let total_chars = blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolResult { output, .. } => Some(output.chars().count()),
                _ => None,
            })
            .sum::<usize>();
        if total_chars <= config.max_message_chars {
            break;
        }

        let Some(candidate_index) = blocks
            .iter()
            .enumerate()
            .filter_map(|(index, block)| match block {
                ContentBlock::ToolResult { output, .. } if !is_budgeted_output(output) => {
                    Some((index, output.chars().count()))
                }
                _ => None,
            })
            .max_by_key(|(_, chars)| *chars)
            .map(|(index, _)| index)
        else {
            break;
        };

        if let Some(ContentBlock::ToolResult {
            tool_use_id,
            tool_name,
            output,
            ..
        }) = blocks.get_mut(candidate_index)
        {
            *output = build_budgeted_tool_result_output(tool_use_id, tool_name, output, &config);
        } else {
            break;
        }
    }
}

fn persist_tool_result(
    tool_use_id: &str,
    output: &str,
    storage_root: Option<&Path>,
) -> Option<PathBuf> {
    let root = storage_root?;
    if fs::create_dir_all(root).is_err() {
        return None;
    }
    let extension = if serde_json::from_str::<Value>(output).is_ok() {
        "json"
    } else {
        "txt"
    };
    let path = root.join(format!("{tool_use_id}.{extension}"));
    fs::write(&path, output).ok()?;
    Some(path)
}

fn extract_tool_result_metadata(output: &str) -> Value {
    let Ok(Value::Object(object)) = serde_json::from_str::<Value>(output) else {
        return Value::Null;
    };
    let mut metadata = serde_json::Map::new();
    for key in [
        "path",
        "success",
        "exit_code",
        "command",
        "diagnostic_count",
        "definition_count",
        "reference_count",
        "line",
        "character",
    ] {
        let Some(value) = object.get(key) else {
            continue;
        };
        if value.is_string() || value.is_boolean() || value.is_number() {
            metadata.insert(key.to_string(), value.clone());
        }
    }
    if metadata.is_empty() {
        Value::Null
    } else {
        Value::Object(metadata)
    }
}

fn is_budgeted_output(output: &str) -> bool {
    serde_json::from_str::<Value>(output)
        .ok()
        .and_then(|value| value.get("_context_budget").cloned())
        .is_some()
}

fn preview_head_tail(content: &str, max_chars: usize) -> String {
    let total = content.chars().count();
    if total <= max_chars {
        return content.to_string();
    }

    let head = (max_chars * 2 / 3).max(120);
    let tail = max_chars.saturating_sub(head).max(40);
    let head_text = content.chars().take(head).collect::<String>();
    let tail_text = content
        .chars()
        .rev()
        .take(tail)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();

    format!(
        "{head_text}\n\n[truncated for context budget: omitted {} chars]\n\n{tail_text}",
        total.saturating_sub(head + tail)
    )
}

fn collapse_text_block(content: &str, max_chars: usize) -> String {
    if content.chars().count() <= max_chars {
        return content.to_string();
    }

    format!(
        "{}\n\n[collapsed in context pipeline]\n\n{}",
        content
            .chars()
            .take((max_chars * 2 / 3).max(120))
            .collect::<String>(),
        content
            .chars()
            .rev()
            .take((max_chars / 3).max(40))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>()
    )
}

fn line_count(content: &str) -> usize {
    content.lines().count().max(1)
}

fn summarize_messages(messages: &[ConversationMessage]) -> String {
    let user_messages = messages
        .iter()
        .filter(|message| message.role == MessageRole::User)
        .count();
    let assistant_messages = messages
        .iter()
        .filter(|message| message.role == MessageRole::Assistant)
        .count();
    let tool_messages = messages
        .iter()
        .filter(|message| message.role == MessageRole::Tool)
        .count();

    let mut tool_names = messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| match block {
            ContentBlock::ToolUse { name, .. } => Some(name.as_str()),
            ContentBlock::ToolResult { tool_name, .. } => Some(tool_name.as_str()),
            ContentBlock::Text { .. } => None,
        })
        .collect::<Vec<_>>();
    tool_names.sort_unstable();
    tool_names.dedup();

    let mut lines = vec![
        "<summary>".to_string(),
        "Conversation summary:".to_string(),
        format!(
            "- Scope: {} earlier messages compacted (user={}, assistant={}, tool={}).",
            messages.len(),
            user_messages,
            assistant_messages,
            tool_messages
        ),
    ];

    if !tool_names.is_empty() {
        lines.push(format!("- Tools mentioned: {}.", tool_names.join(", ")));
    }

    let recent_user_requests = collect_recent_role_summaries(messages, MessageRole::User, 3);
    if !recent_user_requests.is_empty() {
        lines.push("- Recent user requests:".to_string());
        lines.extend(
            recent_user_requests
                .into_iter()
                .map(|request| format!("  - {request}")),
        );
    }

    let pending_work = infer_pending_work(messages);
    if !pending_work.is_empty() {
        lines.push("- Pending work:".to_string());
        lines.extend(pending_work.into_iter().map(|item| format!("  - {item}")));
    }

    let key_files = collect_key_files(messages);
    if !key_files.is_empty() {
        lines.push(format!("- Key files referenced: {}.", key_files.join(", ")));
    }

    if let Some(current_work) = infer_current_work(messages) {
        lines.push(format!("- Current work: {current_work}"));
    }

    lines.push("- Key timeline:".to_string());
    for message in messages {
        let role = match message.role {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        };
        let content = message
            .blocks
            .iter()
            .map(summarize_block)
            .collect::<Vec<_>>()
            .join(" | ");
        lines.push(format!("  - {role}: {content}"));
    }
    lines.push("</summary>".to_string());
    lines.join("\n")
}

fn merge_compact_summaries(existing_summary: Option<&str>, new_summary: &str) -> String {
    let Some(existing_summary) = existing_summary else {
        return new_summary.to_string();
    };

    let previous_highlights = extract_summary_highlights(existing_summary);
    let new_formatted_summary = format_compact_summary(new_summary);
    let new_highlights = extract_summary_highlights(&new_formatted_summary);
    let new_timeline = extract_summary_timeline(&new_formatted_summary);

    let mut lines = vec!["<summary>".to_string(), "Conversation summary:".to_string()];

    if !previous_highlights.is_empty() {
        lines.push("- Previously compacted context:".to_string());
        lines.extend(
            previous_highlights
                .into_iter()
                .map(|line| format!("  {line}")),
        );
    }

    if !new_highlights.is_empty() {
        lines.push("- Newly compacted context:".to_string());
        lines.extend(new_highlights.into_iter().map(|line| format!("  {line}")));
    }

    if !new_timeline.is_empty() {
        lines.push("- Key timeline:".to_string());
        lines.extend(new_timeline.into_iter().map(|line| format!("  {line}")));
    }

    lines.push("</summary>".to_string());
    lines.join("\n")
}

fn summarize_block(block: &ContentBlock) -> String {
    let raw = match block {
        ContentBlock::Text { text } => text.clone(),
        ContentBlock::ToolUse { name, input, .. } => format!("tool_use {name}({input})"),
        ContentBlock::ToolResult {
            tool_name,
            output,
            is_error,
            ..
        } => format!(
            "tool_result {tool_name}: {}{output}",
            if *is_error { "error " } else { "" }
        ),
    };
    truncate_summary(&raw, 160)
}

fn collect_recent_role_summaries(
    messages: &[ConversationMessage],
    role: MessageRole,
    limit: usize,
) -> Vec<String> {
    messages
        .iter()
        .filter(|message| message.role == role)
        .rev()
        .filter_map(ConversationMessage::first_text)
        .take(limit)
        .map(|text| truncate_summary(text, 160))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn infer_pending_work(messages: &[ConversationMessage]) -> Vec<String> {
    messages
        .iter()
        .rev()
        .filter_map(ConversationMessage::first_text)
        .filter(|text| {
            let lowered = text.to_ascii_lowercase();
            lowered.contains("todo")
                || lowered.contains("next")
                || lowered.contains("pending")
                || lowered.contains("follow up")
                || lowered.contains("remaining")
        })
        .take(3)
        .map(|text| truncate_summary(text, 160))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn collect_key_files(messages: &[ConversationMessage]) -> Vec<String> {
    let mut files = messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .map(|block| match block {
            ContentBlock::Text { text } => text.as_str(),
            ContentBlock::ToolUse { input, .. } => input.as_str(),
            ContentBlock::ToolResult { output, .. } => output.as_str(),
        })
        .flat_map(extract_file_candidates)
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    files.into_iter().take(8).collect()
}

fn infer_current_work(messages: &[ConversationMessage]) -> Option<String> {
    messages
        .iter()
        .rev()
        .filter_map(ConversationMessage::first_text)
        .find(|text| !text.trim().is_empty())
        .map(|text| truncate_summary(text, 200))
}

fn extract_file_candidates(content: &str) -> Vec<String> {
    content
        .split_whitespace()
        .filter_map(|token| {
            let candidate = token.trim_matches(|ch: char| ",.:;()[]{}'\"`".contains(ch));
            if candidate.contains('/') && has_interesting_extension(candidate) {
                Some(candidate.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn has_interesting_extension(candidate: &str) -> bool {
    std::path::Path::new(candidate)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["rs", "ts", "tsx", "js", "json", "md"]
                .iter()
                .any(|expected| extension.eq_ignore_ascii_case(expected))
        })
}

fn truncate_summary(content: &str, max_chars: usize) -> String {
    if content.chars().count() <= max_chars {
        return content.to_string();
    }
    let mut truncated = content.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn estimate_message_tokens(message: &ConversationMessage) -> usize {
    message
        .blocks
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text } => text.len() / 4 + 1,
            ContentBlock::ToolUse { name, input, .. } => (name.len() + input.len()) / 4 + 1,
            ContentBlock::ToolResult {
                tool_name, output, ..
            } => (tool_name.len() + output.len()) / 4 + 1,
        })
        .sum()
}

fn extract_tag_block(content: &str, tag: &str) -> Option<String> {
    let start = format!("<{tag}>");
    let end = format!("</{tag}>");
    let start_index = content.find(&start)? + start.len();
    let end_index = content[start_index..].find(&end)? + start_index;
    Some(content[start_index..end_index].to_string())
}

fn strip_tag_block(content: &str, tag: &str) -> String {
    let start = format!("<{tag}>");
    let end = format!("</{tag}>");
    if let Some(start_index) = content.find(&start) {
        if let Some(end_rel) = content[start_index..].find(&end) {
            let end_index = start_index + end_rel + end.len();
            let mut stripped = String::new();
            stripped.push_str(&content[..start_index]);
            stripped.push_str(&content[end_index..]);
            return stripped;
        }
    }
    content.to_string()
}

fn collapse_blank_lines(content: &str) -> String {
    let mut result = String::new();
    let mut last_blank = false;
    for line in content.lines() {
        let is_blank = line.trim().is_empty();
        if is_blank && last_blank {
            continue;
        }
        result.push_str(line);
        result.push('\n');
        last_blank = is_blank;
    }
    result
}

fn extract_existing_compacted_summary(message: &ConversationMessage) -> Option<String> {
    if message.role != MessageRole::System {
        return None;
    }

    let text = message.first_text()?;
    let summary = text.strip_prefix(COMPACT_CONTINUATION_PREAMBLE)?;
    let summary = summary
        .split_once(&format!("\n\n{COMPACT_RECENT_MESSAGES_NOTE}"))
        .map_or(summary, |(value, _)| value);
    let summary = summary
        .split_once(&format!("\n{COMPACT_DIRECT_RESUME_INSTRUCTION}"))
        .map_or(summary, |(value, _)| value);
    Some(summary.trim().to_string())
}

fn extract_summary_highlights(summary: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut in_timeline = false;

    for line in format_compact_summary(summary).lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() || trimmed == "Summary:" || trimmed == "Conversation summary:" {
            continue;
        }
        if trimmed == "- Key timeline:" {
            in_timeline = true;
            continue;
        }
        if in_timeline {
            continue;
        }
        lines.push(trimmed.to_string());
    }

    lines
}

fn extract_summary_timeline(summary: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut in_timeline = false;

    for line in format_compact_summary(summary).lines() {
        let trimmed = line.trim_end();
        if trimmed == "- Key timeline:" {
            in_timeline = true;
            continue;
        }
        if !in_timeline {
            continue;
        }
        if trimmed.is_empty() {
            break;
        }
        lines.push(trimmed.to_string());
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::{
        budget_message_tool_results, budget_session_tool_results, collapse_session_messages,
        collect_key_files, compact_session, estimate_session_tokens, format_compact_summary,
        get_compact_continuation_message, infer_pending_work, microcompact_session_messages,
        should_compact, snip_session_messages, CompactConfig, MessageCollapseConfig, SnipConfig,
        ToolResultBudgetConfig, MICROCOMPACT_CLEARED_MESSAGE, SESSION_MEMORY_COMPACT_SECTION_CHARS,
    };
    use crate::session::{ContentBlock, ConversationMessage, MessageRole, Session};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("opencowork-tool-results-{stamp}"))
    }

    #[test]
    fn formats_compact_summary_like_reference() {
        let summary = "<analysis>scratch</analysis>\n<summary>Kept work</summary>";
        assert_eq!(format_compact_summary(summary), "Summary:\nKept work");
    }

    #[test]
    fn leaves_small_sessions_unchanged() {
        let session = Session::from_messages(vec![ConversationMessage::user("hello")]);

        let result = compact_session(&session, CompactConfig::default());
        assert_eq!(result.removed_message_count, 0);
        assert_eq!(result.compacted_session, session);
        assert!(result.summary.is_empty());
        assert!(result.formatted_summary.is_empty());
    }

    #[test]
    fn compacts_older_messages_into_a_system_summary() {
        let session = Session::from_messages(vec![
            ConversationMessage::user("one ".repeat(200)),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "two ".repeat(200),
                }],
                None,
            ),
            ConversationMessage::tool_result("1", "bash", "ok ".repeat(200), false),
            ConversationMessage {
                role: MessageRole::Assistant,
                blocks: vec![ContentBlock::Text {
                    text: "recent".to_string(),
                }],
                usage: None,
            },
        ]);

        let result = compact_session(
            &session,
            CompactConfig {
                preserve_recent_messages: 2,
                max_estimated_tokens: 1,
            },
        );

        assert_eq!(result.removed_message_count, 2);
        assert_eq!(
            result.compacted_session.messages[0].role,
            MessageRole::System
        );
        assert!(matches!(
            &result.compacted_session.messages[0].blocks[0],
            ContentBlock::Text { text } if text.contains("Summary:")
        ));
        assert_eq!(result.compacted_session.context_collapse_commits.len(), 1);
        assert_eq!(
            result
                .compacted_session
                .context_collapse_snapshot
                .as_ref()
                .map(|snapshot| snapshot.collapsed_spans),
            Some(1)
        );
        assert!(result.formatted_summary.contains("Scope:"));
        assert!(result.formatted_summary.contains("Key timeline:"));
        assert!(should_compact(
            &session,
            CompactConfig {
                preserve_recent_messages: 2,
                max_estimated_tokens: 1,
            }
        ));
        assert!(
            estimate_session_tokens(&result.compacted_session) < estimate_session_tokens(&session)
        );
    }

    #[test]
    fn prefers_session_memory_when_available_for_compaction() {
        let mut session = Session::from_messages(vec![
            ConversationMessage::user("old requirement ".repeat(400)),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "old implementation notes ".repeat(400),
                }],
                None,
            ),
            ConversationMessage::user("old follow-up ".repeat(400)),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "old wrap-up ".repeat(400),
                }],
                None,
            ),
            ConversationMessage::user("recent user context ".repeat(800)),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "recent assistant context ".repeat(800),
                }],
                None,
            ),
            ConversationMessage::user("recent review request ".repeat(800)),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "recent implementation details ".repeat(800),
                }],
                None,
            ),
            ConversationMessage::user("recent extra request ".repeat(800)),
        ]);
        session.current_session_memory = Some(
            "# Session Title\nOpenCoWork memory\n\n# Current State\nTrack parity with the reference memory pipeline.\n\n# Worklog\n- moved relevant memory selection to a provider-backed path".to_string(),
        );
        session.session_memory_state.initialized = true;
        session.session_memory_state.last_summarized_message_count = 4;

        let result = compact_session(
            &session,
            CompactConfig {
                preserve_recent_messages: 2,
                max_estimated_tokens: 1,
            },
        );

        assert!(result
            .formatted_summary
            .contains("Track parity with the reference memory pipeline."));
        assert!(matches!(
            &result.compacted_session.messages[1].blocks[0],
            ContentBlock::Text { text } if text.contains("recent user context")
        ));
        assert_eq!(
            result
                .compacted_session
                .session_memory_state
                .last_summarized_message_count,
            0
        );
        assert_eq!(result.removed_message_count, 4);
    }

    #[test]
    fn session_memory_compaction_keeps_tool_use_with_matching_tool_result() {
        let mut session = Session::from_messages(vec![
            ConversationMessage::user("archived context ".repeat(400)),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "archived response ".repeat(400),
                }],
                None,
            ),
            ConversationMessage::assistant(
                vec![ContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "read_file".to_string(),
                    input: r#"{"path":"src/main.rs"}"#.to_string(),
                }],
                None,
            ),
            ConversationMessage::tool_result(
                "tool-1",
                "read_file",
                r#"{"path":"src/main.rs","content":"fn main() {}\n"}"#,
                false,
            ),
            ConversationMessage::user("recent query ".repeat(1_200)),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "recent answer ".repeat(1_200),
                }],
                None,
            ),
            ConversationMessage::user("recent follow-up ".repeat(1_200)),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "recent code walk ".repeat(1_200),
                }],
                None,
            ),
            ConversationMessage::user("recent wrap-up ".repeat(1_200)),
        ]);
        session.current_session_memory = Some(
            "# Session Title\nTool parity\n\n# Current State\nKeep tool pairs intact during compact.".to_string(),
        );
        session.session_memory_state.initialized = true;
        session.session_memory_state.last_summarized_message_count = 3;

        let result = compact_session(
            &session,
            CompactConfig {
                preserve_recent_messages: 1,
                max_estimated_tokens: 1,
            },
        );

        assert!(matches!(
            &result.compacted_session.messages[1].blocks[0],
            ContentBlock::ToolUse { id, .. } if id == "tool-1"
        ));
        assert!(matches!(
            &result.compacted_session.messages[2].blocks[0],
            ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "tool-1"
        ));
    }

    #[test]
    fn truncates_oversized_session_memory_sections_for_compaction() {
        let mut session = Session::from_messages(vec![
            ConversationMessage::user("old context ".repeat(600)),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "old answer ".repeat(600),
                }],
                None,
            ),
            ConversationMessage::user("recent context ".repeat(900)),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "recent answer ".repeat(900),
                }],
                None,
            ),
        ]);
        session.current_session_memory = Some(format!(
            "# Session Title\nBig memory\n\n# Current State\n{}\n\n# Worklog\n- retained the compact path",
            "x".repeat(SESSION_MEMORY_COMPACT_SECTION_CHARS.saturating_add(1_000))
        ));
        session.session_memory_state.initialized = true;
        session.session_memory_state.last_summarized_message_count = 2;

        let result = compact_session(
            &session,
            CompactConfig {
                preserve_recent_messages: 1,
                max_estimated_tokens: 1,
            },
        );

        assert!(result.summary.contains("[truncated for compact]"));
        assert!(result
            .summary
            .contains("Some session memory sections were truncated for length."));
    }

    #[test]
    fn keeps_previous_compacted_context_when_compacting_again() {
        let initial_session = Session::from_messages(vec![
            ConversationMessage::user("Investigate crates/runtime/src/compact.rs"),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "I will inspect the compact flow.".to_string(),
                }],
                None,
            ),
            ConversationMessage::user("Also update crates/runtime/src/conversation.rs"),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "Next: preserve prior summary context during auto compact.".to_string(),
                }],
                None,
            ),
        ]);
        let config = CompactConfig {
            preserve_recent_messages: 2,
            max_estimated_tokens: 1,
        };

        let first = compact_session(&initial_session, config);
        let mut follow_up_messages = first.compacted_session.messages.clone();
        follow_up_messages.extend([
            ConversationMessage::user("Please add regression tests for compaction."),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "Working on regression coverage now.".to_string(),
                }],
                None,
            ),
        ]);

        let second = compact_session(
            &first.compacted_session.with_messages(follow_up_messages),
            config,
        );

        assert!(second
            .formatted_summary
            .contains("Previously compacted context:"));
        assert!(second
            .formatted_summary
            .contains("Newly compacted context:"));
        assert!(second
            .formatted_summary
            .contains("Also update crates/runtime/src/conversation.rs"));
        assert!(matches!(
            &second.compacted_session.messages[0].blocks[0],
            ContentBlock::Text { text }
                if text.contains("Previously compacted context:")
                    && text.contains("Newly compacted context:")
        ));
        assert!(matches!(
            &second.compacted_session.messages[1].blocks[0],
            ContentBlock::Text { text } if text.contains("Please add regression tests for compaction.")
        ));
        assert_eq!(second.compacted_session.context_collapse_commits.len(), 2);
        assert_eq!(
            second
                .compacted_session
                .context_collapse_snapshot
                .as_ref()
                .map(|snapshot| snapshot.collapsed_messages),
            Some(4)
        );
    }

    #[test]
    fn ignores_existing_compacted_summary_when_deciding_to_recompact() {
        let summary = "<summary>Conversation summary:\n- Scope: earlier work preserved.\n- Key timeline:\n  - user: large preserved context\n</summary>";
        let session = Session::from_messages(vec![
            ConversationMessage::system(get_compact_continuation_message(summary, true, true)),
            ConversationMessage::user("tiny"),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "recent".to_string(),
                }],
                None,
            ),
        ]);

        assert!(!should_compact(
            &session,
            CompactConfig {
                preserve_recent_messages: 2,
                max_estimated_tokens: 1,
            }
        ));
    }

    #[test]
    fn extracts_key_files_from_message_content() {
        let files = collect_key_files(&[ConversationMessage::user(
            "Update crates/runtime/src/compact.rs and crates/tools/src/lib.rs next.",
        )]);
        assert!(files.contains(&"crates/runtime/src/compact.rs".to_string()));
        assert!(files.contains(&"crates/tools/src/lib.rs".to_string()));
    }

    #[test]
    fn infers_pending_work_from_recent_messages() {
        let pending = infer_pending_work(&[
            ConversationMessage::user("done"),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "Next: update tests and follow up on remaining CLI polish.".to_string(),
                }],
                None,
            ),
        ]);
        assert_eq!(pending.len(), 1);
        assert!(pending[0].contains("Next: update tests"));
    }

    #[test]
    fn budgets_large_file_tool_results_without_losing_metadata() {
        let message = ConversationMessage::tool_result(
            "tool-1",
            "read_file",
            serde_json::json!({
                "path": "src/main.rs",
                "content": "line\n".repeat(4_000),
            })
            .to_string(),
            false,
        );

        let budgeted = budget_message_tool_results(
            &message,
            ToolResultBudgetConfig {
                max_result_chars: 2_000,
                max_message_chars: 6_000,
                preview_chars: 1_200,
                storage_root: None,
            },
        );

        assert!(matches!(
            &budgeted.blocks[0],
            ContentBlock::ToolResult { output, .. }
                if output.contains("\"path\":\"src/main.rs\"")
                    && output.contains("_context_budget")
                    && output.contains("[truncated for context budget:")
        ));
    }

    #[test]
    fn budgets_session_tool_results_only_for_request_copy() {
        let session = Session::from_messages(vec![ConversationMessage::tool_result(
            "tool-1",
            "bash",
            serde_json::json!({
                "stdout": "x".repeat(10_000),
                "stderr": "",
                "success": true,
            })
            .to_string(),
            false,
        )]);

        let budgeted = budget_session_tool_results(
            &session,
            ToolResultBudgetConfig {
                max_result_chars: 1_000,
                max_message_chars: 4_000,
                preview_chars: 800,
                storage_root: None,
            },
        );

        assert_ne!(budgeted, session);
        assert!(matches!(
            &budgeted.messages[0].blocks[0],
            ContentBlock::ToolResult { output, .. }
                if output.contains("_context_budget")
        ));
    }

    #[test]
    fn collapses_older_long_text_before_full_compaction() {
        let session = Session::from_messages(vec![
            ConversationMessage::user("A".repeat(500)),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "B".repeat(500),
                }],
                None,
            ),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "recent".to_string(),
                }],
                None,
            ),
        ]);

        let collapsed = collapse_session_messages(
            &session,
            MessageCollapseConfig {
                preserve_recent_messages: 1,
                max_chars: 120,
            },
        );

        assert!(matches!(
            &collapsed.messages[0].blocks[0],
            ContentBlock::Text { text } if text.contains("[collapsed in context pipeline]")
        ));
        assert!(matches!(
            &collapsed.messages[2].blocks[0],
            ContentBlock::Text { text } if text == "recent"
        ));
    }

    #[test]
    fn microcompacts_old_tool_results_but_keeps_recent_ones() {
        let mut messages = Vec::new();
        for index in 0..7 {
            messages.push(ConversationMessage::assistant(
                vec![ContentBlock::ToolUse {
                    id: format!("tool-{index}"),
                    name: "read_file".to_string(),
                    input: format!(r#"{{"path":"src/file-{index}.rs"}}"#),
                }],
                None,
            ));
            messages.push(ConversationMessage::tool_result(
                format!("tool-{index}"),
                "read_file",
                format!("content-{index}-{}", "x".repeat(400)),
                false,
            ));
        }
        let session = Session::from_messages(messages);

        let result = microcompact_session_messages(&session);

        assert_eq!(result.cleared_tool_results, 2);
        let outputs = result
            .microcompacted_session
            .messages
            .iter()
            .flat_map(|message| message.blocks.iter())
            .filter_map(|block| match block {
                ContentBlock::ToolResult { output, .. } => Some(output.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(outputs[0], MICROCOMPACT_CLEARED_MESSAGE);
        assert_eq!(outputs[1], MICROCOMPACT_CLEARED_MESSAGE);
        assert!(outputs[2].starts_with("content-2-"));
    }

    #[test]
    fn compaction_persists_archived_messages_in_session() {
        let session = Session::from_messages(vec![
            ConversationMessage::user("legacy context ".repeat(200)),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "legacy reply ".repeat(200),
                }],
                None,
            ),
            ConversationMessage::user("recent context"),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "recent reply".to_string(),
                }],
                None,
            ),
        ]);

        let result = compact_session(
            &session,
            CompactConfig {
                preserve_recent_messages: 2,
                max_estimated_tokens: 200,
            },
        );

        assert_eq!(result.compacted_session.context_collapse_archive.len(), 2);
        assert!(matches!(
            &result.compacted_session.context_collapse_archive[0].blocks[0],
            ContentBlock::Text { text } if text.contains("legacy context")
        ));
    }

    #[test]
    fn enforces_message_level_tool_result_budget() {
        let message = ConversationMessage {
            role: MessageRole::User,
            blocks: vec![
                ContentBlock::ToolResult {
                    tool_use_id: "tool-1".to_string(),
                    tool_name: "read_file".to_string(),
                    output: "A".repeat(4_000),
                    is_error: false,
                },
                ContentBlock::ToolResult {
                    tool_use_id: "tool-2".to_string(),
                    tool_name: "bash".to_string(),
                    output: "B".repeat(4_000),
                    is_error: false,
                },
            ],
            usage: None,
        };

        let budgeted = budget_message_tool_results(
            &message,
            ToolResultBudgetConfig {
                max_result_chars: 10_000,
                max_message_chars: 5_000,
                preview_chars: 600,
                storage_root: None,
            },
        );

        let outputs = budgeted
            .blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolResult { output, .. } => Some(output),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(outputs
            .iter()
            .any(|output| output.contains("_context_budget")));
    }

    #[test]
    fn persists_large_tool_results_when_storage_root_is_configured() {
        let root = temp_dir();
        let message = ConversationMessage::tool_result(
            "tool-1",
            "read_file",
            serde_json::json!({
                "path": "src/main.rs",
                "content": "x".repeat(8_000),
            })
            .to_string(),
            false,
        );

        let budgeted = budget_message_tool_results(
            &message,
            ToolResultBudgetConfig {
                max_result_chars: 1_000,
                max_message_chars: 4_000,
                preview_chars: 500,
                storage_root: Some(root.clone()),
            },
        );

        let ContentBlock::ToolResult { output, .. } = &budgeted.blocks[0] else {
            panic!("expected tool result block");
        };
        assert!(output.contains("persisted_path"));
        assert!(root.join("tool-1.json").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn snips_old_prefix_before_full_compaction() {
        let session = Session::from_messages(vec![
            ConversationMessage::user("A".repeat(600)),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "B".repeat(600),
                }],
                None,
            ),
            ConversationMessage::user("keep-me".to_string()),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "keep-me-too".to_string(),
                }],
                None,
            ),
        ]);

        let result = snip_session_messages(
            &session,
            SnipConfig {
                preserve_recent_messages: 2,
                max_estimated_tokens: 200,
            },
        );

        assert_eq!(result.removed_message_count, 2);
        assert!(result.removed_estimated_tokens > 0);
        assert_eq!(result.snipped_session.messages.len(), 2);
        assert!(matches!(
            &result.snipped_session.messages[0].blocks[0],
            ContentBlock::Text { text } if text == "keep-me"
        ));
    }

    #[test]
    fn keeps_existing_collapse_snapshot_visible_without_new_compaction() {
        let compacted = compact_session(
            &Session::from_messages(vec![
                ConversationMessage::user("one ".repeat(200)),
                ConversationMessage::assistant(
                    vec![ContentBlock::Text {
                        text: "two ".repeat(200),
                    }],
                    None,
                ),
                ConversationMessage::user("recent"),
            ]),
            CompactConfig {
                preserve_recent_messages: 1,
                max_estimated_tokens: 1,
            },
        )
        .compacted_session;

        let result = compact_session(
            &compacted,
            CompactConfig {
                preserve_recent_messages: 4,
                max_estimated_tokens: usize::MAX,
            },
        );

        assert!(!result.summary.is_empty());
        assert_eq!(
            result.summary,
            compacted
                .context_collapse_snapshot
                .as_ref()
                .expect("snapshot")
                .summary
        );
        assert_eq!(result.removed_message_count, 0);
    }
}

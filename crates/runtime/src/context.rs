use crate::{
    budget_session_tool_results, collapse_session_messages, compact_session,
    compacted_summary_prefix_len, estimate_session_tokens, get_auto_compact_threshold,
    microcompact_session_messages, snip_session_messages, CompactConfig, ConversationMessage,
    InstructionSource, MessageCollapseConfig, Session, SnipConfig, ToolResultBudgetConfig,
    AUTOCOMPACT_BUFFER_TOKENS, DEFAULT_MAX_TOOL_RESULTS_PER_MESSAGE_CHARS,
    DEFAULT_MAX_TOOL_RESULT_CHARS, DEFAULT_TOOL_RESULT_PREVIEW_CHARS, MAX_OUTPUT_TOKENS_DEFAULT,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextOptimizerConfig {
    pub model: String,
    pub preserve_recent_messages: usize,
    pub context_window_tokens_override: Option<usize>,
    pub max_instruction_tokens: usize,
    pub max_output_tokens: usize,
    pub auto_compact_buffer_tokens: usize,
    pub message_collapse_chars: usize,
    pub max_tool_result_chars: usize,
    pub max_tool_results_per_message_chars: usize,
}

impl Default for ContextOptimizerConfig {
    fn default() -> Self {
        Self {
            model: "gpt-5.4-mini".to_string(),
            preserve_recent_messages: 8,
            context_window_tokens_override: None,
            max_instruction_tokens: 12_000,
            max_output_tokens: MAX_OUTPUT_TOKENS_DEFAULT,
            auto_compact_buffer_tokens: AUTOCOMPACT_BUFFER_TOKENS,
            message_collapse_chars: 8_000,
            max_tool_result_chars: DEFAULT_MAX_TOOL_RESULT_CHARS,
            max_tool_results_per_message_chars: DEFAULT_MAX_TOOL_RESULTS_PER_MESSAGE_CHARS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPreparation {
    pub session: Session,
    pub summary: Option<String>,
    pub compacted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSelection {
    pub instructions: Vec<InstructionSource>,
    pub recent_messages: Vec<ConversationMessage>,
    pub summary: Option<String>,
    pub estimated_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextOptimizer {
    config: ContextOptimizerConfig,
}

impl ContextOptimizer {
    #[must_use]
    pub fn new(config: ContextOptimizerConfig) -> Self {
        Self { config }
    }

    #[must_use]
    pub fn prepare_session(&self, session: &Session) -> SessionPreparation {
        let budgeted = budget_session_tool_results(
            session,
            ToolResultBudgetConfig {
                max_result_chars: self.config.max_tool_result_chars,
                max_message_chars: self.config.max_tool_results_per_message_chars,
                preview_chars: DEFAULT_TOOL_RESULT_PREVIEW_CHARS,
                storage_root: None,
            },
        );
        let compact_trigger_tokens = get_auto_compact_threshold(
            &self.config.model,
            self.config.context_window_tokens_override,
            self.config.max_output_tokens,
            self.config.auto_compact_buffer_tokens,
        )
        .max(1);
        let snipped = snip_session_messages(
            &budgeted,
            SnipConfig {
                preserve_recent_messages: self.config.preserve_recent_messages,
                max_estimated_tokens: compact_trigger_tokens,
            },
        );
        let microcompacted =
            if estimate_session_tokens(&snipped.snipped_session) > compact_trigger_tokens {
                microcompact_session_messages(&snipped.snipped_session).microcompacted_session
            } else {
                snipped.snipped_session
            };
        let collapsed = collapse_session_messages(
            &microcompacted,
            MessageCollapseConfig {
                preserve_recent_messages: self.config.preserve_recent_messages,
                max_chars: self.config.message_collapse_chars,
            },
        );
        let result = compact_session(
            &collapsed,
            CompactConfig {
                preserve_recent_messages: self.config.preserve_recent_messages,
                max_estimated_tokens: compact_trigger_tokens,
            },
        );

        SessionPreparation {
            session: result.compacted_session,
            summary: (!result.formatted_summary.is_empty()).then_some(result.formatted_summary),
            compacted: result.removed_message_count > 0,
        }
    }

    #[must_use]
    pub fn select(
        &self,
        _session: &Session,
        instructions: &[InstructionSource],
        preparation: &SessionPreparation,
    ) -> ContextSelection {
        let mut kept_instructions = Vec::new();
        let mut used_instruction_tokens = 0;
        let mut sorted = instructions.to_vec();
        sorted.sort_by(|left, right| right.priority.cmp(&left.priority));

        for instruction in sorted {
            let estimate = instruction.content.len() / 4 + 1;
            if used_instruction_tokens + estimate > self.config.max_instruction_tokens {
                continue;
            }
            used_instruction_tokens += estimate;
            kept_instructions.push(instruction);
        }

        let visible_messages =
            &preparation.session.messages[compacted_summary_prefix_len(&preparation.session)..];
        let recent_messages = if preparation.compacted {
            visible_messages.to_vec()
        } else {
            visible_messages
                .iter()
                .rev()
                .take(self.config.preserve_recent_messages)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        };

        let estimated_tokens = recent_messages
            .iter()
            .map(|message| message.first_text().map_or(0, |text| text.len() / 4 + 1))
            .sum::<usize>()
            + used_instruction_tokens;

        ContextSelection {
            instructions: kept_instructions,
            recent_messages,
            summary: preparation.summary.clone(),
            estimated_tokens,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ContextOptimizer, ContextOptimizerConfig};
    use crate::{ConversationMessage, InstructionSource, Session};

    #[test]
    fn keeps_high_priority_instructions_under_budget() {
        let optimizer = ContextOptimizer::new(ContextOptimizerConfig {
            model: "gpt-5.4-mini".to_string(),
            preserve_recent_messages: 2,
            context_window_tokens_override: Some(100),
            max_instruction_tokens: 12,
            max_output_tokens: 20,
            auto_compact_buffer_tokens: 10,
            message_collapse_chars: 80,
            max_tool_result_chars: 80,
            max_tool_results_per_message_chars: 160,
        });
        let session = Session::from_messages(vec![
            ConversationMessage::user("one"),
            ConversationMessage::user("two"),
            ConversationMessage::user("three"),
        ]);
        let prep = optimizer.prepare_session(&session);
        let selected = optimizer.select(
            &session,
            &[
                InstructionSource {
                    label: "critical".to_string(),
                    content: "critical".to_string(),
                    priority: 10,
                },
                InstructionSource {
                    label: "large".to_string(),
                    content: "this is definitely too large for the tiny budget".to_string(),
                    priority: 1,
                },
            ],
            &prep,
        );
        assert_eq!(selected.instructions.len(), 1);
        assert_eq!(selected.instructions[0].label, "critical");
        assert_eq!(selected.recent_messages.len(), 2);
    }

    #[test]
    fn snips_before_compaction_when_old_history_is_too_large() {
        let optimizer = ContextOptimizer::new(ContextOptimizerConfig {
            model: "gpt-5.4-mini".to_string(),
            preserve_recent_messages: 2,
            context_window_tokens_override: Some(200),
            max_instruction_tokens: 64,
            max_output_tokens: 20,
            auto_compact_buffer_tokens: 10,
            message_collapse_chars: 1_000,
            max_tool_result_chars: 1_000,
            max_tool_results_per_message_chars: 2_000,
        });
        let session = Session::from_messages(vec![
            ConversationMessage::user("A".repeat(400)),
            ConversationMessage::assistant(
                vec![crate::ContentBlock::Text {
                    text: "B".repeat(400),
                }],
                None,
            ),
            ConversationMessage::user("recent-user"),
            ConversationMessage::assistant(
                vec![crate::ContentBlock::Text {
                    text: "recent-assistant".to_string(),
                }],
                None,
            ),
        ]);

        let prep = optimizer.prepare_session(&session);
        assert_eq!(prep.session.messages.len(), 2);
        assert!(matches!(
            &prep.session.messages[0].blocks[0],
            crate::ContentBlock::Text { text } if text == "recent-user"
        ));
        assert!(!prep.compacted);
    }
}

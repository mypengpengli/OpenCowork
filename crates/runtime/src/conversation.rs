use std::collections::BTreeMap;

use serde_json::Value;

use crate::compact::{
    budget_session_tool_results, compact_session, estimate_session_tokens, CompactConfig,
    CompactResult, ToolResultBudgetConfig,
};
use crate::hooks::HookRunner;
use crate::permissions::{
    PermissionDecision, PermissionMode, PermissionPolicy, PermissionPrompter,
};
use crate::session::{ContentBlock, ConversationMessage, Session};
use crate::usage::{TokenUsage, UsageTracker};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub required_permission: PermissionMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiRequest {
    pub system_prompt: Vec<String>,
    pub messages: Vec<ConversationMessage>,
    pub tools: Vec<RuntimeToolDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantEvent {
    ToolCallDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments_delta: String,
    },
    ReasoningDelta {
        text: String,
        field: String,
        provider: String,
        model: String,
    },
    Finish {
        reason: String,
    },
    TextDelta(String),
    ToolUse {
        id: String,
        name: String,
        input: String,
        required_permission: PermissionMode,
    },
    Usage(TokenUsage),
    MessageStop,
}

pub trait ApiClient {
    fn summarize(
        &mut self,
        _messages: &[ConversationMessage],
        _input_tokens: usize,
    ) -> Result<Option<(String, TokenUsage)>, RuntimeError> {
        Ok(None)
    }
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError>;

    fn stream_with_observer(
        &mut self,
        request: ApiRequest,
        on_event: &mut dyn FnMut(&AssistantEvent),
    ) -> Result<Vec<AssistantEvent>, RuntimeError> {
        let events = self.stream(request)?;
        for event in &events {
            on_event(event);
        }
        Ok(events)
    }
}

pub trait ToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError>;

    fn definitions(&self) -> Vec<RuntimeToolDefinition> {
        Vec::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePromptUpdate {
    pub label: String,
    pub content: String,
}

pub trait RuntimePromptAugmenter {
    fn on_tool_result(
        &mut self,
        session: &Session,
        tool_result: &ConversationMessage,
    ) -> Vec<RuntimePromptUpdate>;
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("{message}")]
pub struct RuntimeError {
    message: String,
}

impl RuntimeError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("{message}")]
pub struct ToolError {
    message: String,
}

impl ToolError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnSummary {
    pub assistant_messages: Vec<ConversationMessage>,
    pub tool_results: Vec<ConversationMessage>,
    pub iterations: usize,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TurnLimits {
    pub max_iterations: usize,
    pub max_tokens: u32,
    pub max_seconds: u64,
    pub repeated_results: usize,
}
impl Default for TurnLimits {
    fn default() -> Self {
        Self {
            // Zero leaves model-loop iterations unlimited. Callers can opt in
            // to a finite cap without disabling the no-progress guard.
            max_iterations: 0,
            max_tokens: 250_000,
            max_seconds: 900,
            repeated_results: 3,
        }
    }
}
impl TurnLimits {
    pub fn from_settings(settings: &Value) -> Self {
        let mut v: Self =
            serde_json::from_value(settings.get("execution").cloned().unwrap_or(Value::Null))
                .unwrap_or_default();
        v.max_iterations = v.max_iterations.min(1000);
        v.max_tokens = v.max_tokens.clamp(100, 10_000_000);
        v.max_seconds = v.max_seconds.clamp(1, 86400);
        v.repeated_results = v.repeated_results.clamp(2, 20);
        v
    }

    pub fn iteration_limit_reached(&self, completed: usize) -> bool {
        self.max_iterations != 0 && completed >= self.max_iterations
    }
}

pub struct ConversationRuntime<C, T> {
    session: Session,
    api_client: C,
    tool_executor: T,
    permission_policy: PermissionPolicy,
    system_prompt: Vec<String>,
    max_iterations: usize,
    limits: TurnLimits,
    usage_tracker: UsageTracker,
    hooks: HookRunner,
    prompt_augmenter: Option<Box<dyn RuntimePromptAugmenter>>,
    tool_result_budget: ToolResultBudgetConfig,
    user_context: Option<String>,
    context_config: Option<crate::ContextOptimizerConfig>,
}

pub trait RuntimeObserver {
    fn on_request_metrics(&mut self, _metrics: &crate::RequestMetrics) {}
    fn on_context_compacted(&mut self, _session: &Session, _before: usize, _after: usize) {}
    fn on_assistant_event(&mut self, _event: &AssistantEvent) {}

    fn on_tool_result(&mut self, _message: &ConversationMessage) {}

    fn on_user_message(&mut self, _id: &str, _text: &str) {}
}

impl<C, T> ConversationRuntime<C, T>
where
    C: ApiClient,
    T: ToolExecutor,
{
    #[must_use]
    pub fn new(
        session: Session,
        api_client: C,
        tool_executor: T,
        permission_policy: PermissionPolicy,
        system_prompt: Vec<String>,
        hooks: HookRunner,
    ) -> Self {
        let usage_tracker = UsageTracker::from_session(&session);
        Self {
            session,
            api_client,
            tool_executor,
            permission_policy,
            system_prompt,
            max_iterations: TurnLimits::default().max_iterations,
            limits: TurnLimits::default(),
            usage_tracker,
            hooks,
            prompt_augmenter: None,
            tool_result_budget: ToolResultBudgetConfig::default(),
            user_context: None,
            context_config: None,
        }
    }

    pub fn with_limits(mut self, limits: TurnLimits) -> Self {
        self.max_iterations = limits.max_iterations;
        self.limits = limits;
        self
    }

    pub fn with_context_config(mut self, config: crate::ContextOptimizerConfig) -> Self {
        self.context_config = Some(config);
        self
    }

    #[must_use]
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    #[must_use]
    pub fn with_prompt_augmenter(
        mut self,
        prompt_augmenter: Box<dyn RuntimePromptAugmenter>,
    ) -> Self {
        self.prompt_augmenter = Some(prompt_augmenter);
        self
    }

    #[must_use]
    pub fn with_tool_result_budget(mut self, config: ToolResultBudgetConfig) -> Self {
        self.tool_result_budget = config;
        self
    }

    /// Add bounded, explicitly attached reference data to the next user message.
    pub fn with_user_context(mut self, context: &str) -> Self {
        self.user_context = (!context.is_empty()).then(|| context.to_string());
        self
    }

    pub fn run_turn(
        &mut self,
        user_input: impl Into<String>,
        mut prompter: Option<&mut dyn PermissionPrompter>,
    ) -> Result<TurnSummary, RuntimeError> {
        self.run_turn_with_observer(user_input, prompter.take(), None)
    }

    pub fn run_turn_with_observer(
        &mut self,
        user_input: impl Into<String>,
        mut prompter: Option<&mut dyn PermissionPrompter>,
        mut observer: Option<&mut dyn RuntimeObserver>,
    ) -> Result<TurnSummary, RuntimeError> {
        let user_input = user_input.into();
        let session_start = self.hooks.run_session_start();
        if session_start.is_denied() {
            return Err(RuntimeError::new(join_hook_feedback(
                session_start.messages(),
                "session-start hook denied execution",
            )));
        }

        let submit = self.hooks.run_user_prompt_submit(&user_input);
        if submit.is_denied() {
            return Err(RuntimeError::new(join_hook_feedback(
                submit.messages(),
                "user-prompt hook denied execution",
            )));
        }

        self.session
            .messages
            .push(ConversationMessage::user(user_input));
        if let Some(context) = self.user_context.take() {
            self.session
                .messages
                .last_mut()
                .expect("user message")
                .blocks
                .push(ContentBlock::Text { text: context });
        }

        let mut assistant_messages = Vec::new();
        let mut tool_results = Vec::new();
        let mut iterations = 0;
        let start = std::time::Instant::now();
        let initial_tokens = self.usage_tracker.cumulative().total_tokens();
        let mut previous_result = String::new();
        let mut repeats = 0usize;
        let mut consumed = self.session.delivered_user_messages.clone();
        let mut overflow_retries = 0;

        loop {
            if let Ok(path) = std::env::var("OPENCOWORK_STEERING_FILE") {
                if let Ok(value) = std::fs::read_to_string(path).and_then(|s| {
                    serde_json::from_str::<Vec<Value>>(&s).map_err(std::io::Error::other)
                }) {
                    for item in value
                        .iter()
                        .filter(|x| x["mode"] == "steer" && x["status"] == "pending")
                    {
                        let id = item["id"].as_str().unwrap_or("");
                        let text = item["text"].as_str().unwrap_or("");
                        if !id.is_empty() && !text.is_empty() && consumed.insert(id.to_string()) {
                            self.session.delivered_user_messages.insert(id.to_string());
                            self.session.messages.push(ConversationMessage::user(text));
                            if let Some(ref mut sink) = observer {
                                sink.on_user_message(id, text);
                            }
                        }
                    }
                }
            }
            if start.elapsed().as_secs() >= self.limits.max_seconds
                || self
                    .usage_tracker
                    .cumulative()
                    .total_tokens()
                    .saturating_sub(initial_tokens)
                    >= self.limits.max_tokens
            {
                return Err(RuntimeError::new(
                    "turn_budget_reached: progress saved; adjust execution budget or resume",
                ));
            }
            iterations += 1;
            if self.max_iterations != 0 && iterations > self.max_iterations {
                return Err(RuntimeError::new(
                    "turn_budget_reached: iteration limit; progress saved",
                ));
            }

            if let Some((before, after, summary_usage)) = self.prepare_request_context(false)? {
                if let Some(ref mut observer) = observer {
                    observer.on_assistant_event(&AssistantEvent::Usage(summary_usage));
                    observer.on_context_compacted(&self.session, before, after);
                }
            }
            let request_messages =
                budget_session_tool_results(&self.session, self.tool_result_budget.clone())
                    .messages;
            let request = ApiRequest {
                system_prompt: self.system_prompt.clone(),
                messages: request_messages,
                tools: self.tool_executor.definitions(),
            };
            let request_start = std::time::Instant::now();
            let mut metrics = crate::RequestMetrics {
                id: uuid::Uuid::new_v4().to_string(),
                model: self
                    .context_config
                    .as_ref()
                    .map(|c| c.model.clone())
                    .unwrap_or_default(),
                started_at: crate::process_sessions::now_ms(),
                status: "running".into(),
                ..Default::default()
            };
            self.session.request_metrics.push(metrics.clone());
            if let Some(ref mut sink) = observer {
                sink.on_request_metrics(&metrics);
            }
            let response = self.api_client.stream_with_observer(request, &mut |event| {
                let prior = (metrics.first_event_ms, metrics.first_text_ms);
                let elapsed = request_start.elapsed().as_millis() as u64;
                if matches!(
                    event,
                    AssistantEvent::ReasoningDelta { .. }
                        | AssistantEvent::TextDelta(_)
                        | AssistantEvent::ToolCallDelta { .. }
                        | AssistantEvent::ToolUse { .. }
                ) {
                    metrics.first_event_ms.get_or_insert(elapsed);
                }
                if matches!(event, AssistantEvent::TextDelta(_)) {
                    metrics.first_text_ms.get_or_insert(elapsed);
                }
                if let AssistantEvent::Usage(usage) = event {
                    metrics.usage = *usage;
                }
                if let AssistantEvent::Finish { reason } = event {
                    if !matches!(reason.as_str(), "stop" | "tool_calls" | "function_call") {
                        metrics.status = "incomplete".into();
                    }
                }
                if let Some(ref mut observer) = observer {
                    observer.on_assistant_event(event);
                    if prior != (metrics.first_event_ms, metrics.first_text_ms) {
                        observer.on_request_metrics(&metrics);
                    }
                }
            });
            metrics.elapsed_ms = request_start.elapsed().as_millis() as u64;
            if response.is_err() {
                metrics.status = "failed".into();
            } else if metrics.status == "running" {
                metrics.status = "completed".into();
            }
            if let Some(record) = self
                .session
                .request_metrics
                .iter_mut()
                .find(|record| record.id == metrics.id)
            {
                *record = metrics.clone();
            }
            if let Some(ref mut sink) = observer {
                sink.on_request_metrics(&metrics);
            }
            let events = match response {
                Ok(events) => {
                    overflow_retries = 0;
                    events
                }
                Err(error)
                    if error.to_string().contains("provider_context_overflow")
                        && overflow_retries == 0 =>
                {
                    overflow_retries += 1;
                    if let Some((before, after, summary_usage)) =
                        self.prepare_request_context(true)?
                    {
                        if let Some(ref mut sink) = observer {
                            sink.on_assistant_event(&AssistantEvent::Usage(summary_usage));
                            sink.on_context_compacted(&self.session, before, after);
                        }
                        continue;
                    }
                    return Err(error);
                }
                Err(error) => return Err(error),
            };
            let (mut assistant_message, pending, usage) = build_assistant_message(events)?;
            assistant_message.context_generation = self.session.context_collapse_commits.len();
            assistant_message.request_overhead_tokens = Some(self.request_overhead_tokens());
            if let Some(usage) = usage {
                self.usage_tracker.record(usage);
            }
            self.session.messages.push(assistant_message.clone());
            assistant_messages.push(assistant_message);

            if pending.is_empty() {
                break;
            }

            for pending_tool in pending {
                if start.elapsed().as_secs() >= self.limits.max_seconds
                    || self
                        .usage_tracker
                        .cumulative()
                        .total_tokens()
                        .saturating_sub(initial_tokens)
                        >= self.limits.max_tokens
                {
                    return Err(RuntimeError::new(
                        "turn_budget_reached: tool not executed; progress saved",
                    ));
                }
                let operation = format!("{}:{}", pending_tool.name, pending_tool.input);
                let decision = if let Some(ref mut prompt) = prompter {
                    self.permission_policy.authorize(
                        &pending_tool.name,
                        &pending_tool.input,
                        pending_tool.required_permission,
                        Some(&mut **prompt),
                    )
                } else {
                    self.permission_policy.authorize(
                        &pending_tool.name,
                        &pending_tool.input,
                        pending_tool.required_permission,
                        None,
                    )
                };

                let message = match decision {
                    PermissionDecision::Allow => self.execute_tool_use(pending_tool),
                    PermissionDecision::Deny { reason } => {
                        let denial_hook = self.hooks.run_permission_denied(
                            &pending_tool.name,
                            &pending_tool.input,
                            &reason,
                        );
                        ConversationMessage::tool_result(
                            pending_tool.id,
                            pending_tool.name,
                            merge_feedback(reason, denial_hook.messages(), denial_hook.is_denied()),
                            true,
                        )
                    }
                };
                if let Some(ref mut observer) = observer {
                    observer.on_tool_result(&message);
                }
                let signature = format!("{}:{:?}", operation, message.blocks);
                // Tool-call IDs vary each time; only compare the action and actual output.
                let signature = match message.blocks.first() {
                    Some(ContentBlock::ToolResult {
                        output, is_error, ..
                    }) => format!("{}:{}:{}", operation, is_error, output),
                    _ => signature,
                };
                let expected_wait = match message.blocks.first() {
                    Some(ContentBlock::ToolResult {
                        tool_name,
                        output,
                        is_error: false,
                        ..
                    }) => operation
                        .strip_prefix(&format!("{tool_name}:"))
                        .is_some_and(|input| is_blocking_wait_result(tool_name, input, output)),
                    _ => false,
                };
                if expected_wait {
                    repeats = 0;
                    previous_result.clear();
                } else if signature == previous_result {
                    repeats += 1;
                } else {
                    previous_result = signature;
                    repeats = 1;
                }
                self.session.messages.push(message.clone());
                if repeats >= self.limits.repeated_results {
                    return Err(RuntimeError::new("no_progress: repeated identical action/results; inspect state before resuming"));
                }
                self.apply_prompt_updates(&message)?;
                tool_results.push(message);
            }
        }

        let summary = TurnSummary {
            assistant_messages,
            tool_results,
            iterations,
            usage: self.usage_tracker.cumulative(),
        };

        let end_message = summary
            .assistant_messages
            .last()
            .and_then(ConversationMessage::first_text);
        let session_end = self.hooks.run_session_end(end_message);
        if session_end.is_denied() {
            return Err(RuntimeError::new(join_hook_feedback(
                session_end.messages(),
                "session-end hook denied execution",
            )));
        }

        Ok(summary)
    }

    #[must_use]
    pub fn compact(&self, config: CompactConfig) -> CompactResult {
        compact_session(&self.session, config)
    }

    #[must_use]
    pub fn estimated_tokens(&self) -> usize {
        measured_context_tokens(
            &budget_session_tool_results(&self.session, self.tool_result_budget.clone()),
            self.request_overhead_tokens(),
        )
    }

    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    #[must_use]
    pub fn usage(&self) -> &UsageTracker {
        &self.usage_tracker
    }

    fn prepare_request_context(
        &mut self,
        force: bool,
    ) -> Result<Option<(usize, usize, TokenUsage)>, RuntimeError> {
        let compaction_start = std::time::Instant::now();
        let Some(config) = self.context_config.clone() else {
            return Ok(None);
        };
        let threshold = crate::get_auto_compact_threshold(
            &config.model,
            config.context_window_tokens_override,
            config.max_output_tokens,
            config.auto_compact_buffer_tokens,
        )
        .max(1);
        let threshold = if force { threshold / 2 } else { threshold };
        let overhead = self.request_overhead_tokens();
        if overhead >= threshold {
            return Err(RuntimeError::new("context_budget_exceeded: system instructions and tools exceed this model's input budget"));
        }
        let projection =
            budget_session_tool_results(&self.session, self.tool_result_budget.clone());
        let before = measured_context_tokens(&projection, overhead);
        if before < threshold {
            return Ok(None);
        }
        let budget = threshold - overhead;
        let mut preserve = 0;
        let mut recent = 0;
        for message in projection.messages.iter().rev() {
            let size = estimate_session_tokens(&Session::from_messages(vec![message.clone()]));
            if preserve > 0 && recent + size > budget / 3 {
                break;
            }
            recent += size;
            preserve += 1;
        }
        let compact_config = CompactConfig {
            preserve_recent_messages: preserve,
            max_estimated_tokens: 0,
        };
        let candidate =
            crate::compact_session_with_summary(&self.session, compact_config, Some(""));
        if candidate.removed_message_count == 0 {
            return Err(RuntimeError::new("context_budget_exceeded: no safe history range can be compacted; reduce the current input"));
        }
        let end =
            crate::compacted_summary_prefix_len(&self.session) + candidate.removed_message_count;
        let generated = self
            .api_client
            .summarize(&self.session.messages[..end], budget / 3)?;
        let mut summary_usage = TokenUsage::default();
        let mut next = if let Some((summary, usage)) = generated {
            summary_usage = usage;
            self.usage_tracker.record(usage);
            crate::compact_session_with_summary(&self.session, compact_config, Some(&summary))
        } else {
            compact_session(&self.session, compact_config)
        };
        let after = measured_context_tokens(
            &budget_session_tool_results(&next.compacted_session, self.tool_result_budget.clone()),
            overhead,
        );
        if after >= threshold || after >= before {
            return Err(RuntimeError::new("compaction_failed: checkpoint did not fit the model budget; original history retained"));
        }
        if let Some(commit) = next.compacted_session.context_collapse_commits.last_mut() {
            commit.metrics = Some(crate::CompactionMetrics {
                elapsed_ms: compaction_start.elapsed().as_millis() as u64,
                before_tokens: before,
                after_tokens: after,
                usage: summary_usage,
            });
        }
        self.session = next.compacted_session;
        Ok(Some((before, after, summary_usage)))
    }

    fn request_overhead_tokens(&self) -> usize {
        let text = self
            .system_prompt
            .iter()
            .map(|s| crate::estimate_text_tokens(s))
            .sum::<usize>()
            + self
                .tool_executor
                .definitions()
                .iter()
                .map(|t| {
                    crate::estimate_text_tokens(&t.description)
                        + crate::estimate_text_tokens(&t.input_schema.to_string())
                })
                .sum::<usize>();
        let screenshot = self
            .session
            .messages
            .iter()
            .rev()
            .take_while(|m| m.role != crate::MessageRole::User)
            .flat_map(|m| m.blocks.iter())
            .find_map(|block| {
                if let ContentBlock::ToolResult {
                    tool_name,
                    output,
                    is_error,
                    ..
                } = block
                {
                    if matches!(tool_name.as_str(), "Computer" | "Browser") {
                        return Some(
                            !is_error
                                && serde_json::from_str::<Value>(output)
                                    .ok()
                                    .is_some_and(|v| v.get("screenshotPath").is_some()),
                        );
                    }
                }
                None
            })
            .unwrap_or(false);
        text + if screenshot { 4096 } else { 0 }
    }

    fn execute_tool_use(&mut self, pending_tool: PendingToolUse) -> ConversationMessage {
        let pre = self
            .hooks
            .run_pre_tool_use(&pending_tool.name, &pending_tool.input);
        if pre.is_denied() {
            return ConversationMessage::tool_result(
                pending_tool.id,
                pending_tool.name,
                join_hook_feedback(pre.messages(), "pre-tool hook denied execution"),
                true,
            );
        }

        let (mut output, mut is_error) = match self
            .tool_executor
            .execute(&pending_tool.name, &pending_tool.input)
        {
            Ok(output) => (output, false),
            Err(error) => (error.to_string(), true),
        };
        output = merge_feedback(output, pre.messages(), false);

        let post = self.hooks.run_post_tool_use(
            &pending_tool.name,
            &pending_tool.input,
            &output,
            is_error,
        );
        if post.is_denied() {
            is_error = true;
        }
        output = merge_feedback(output, post.messages(), post.is_denied());

        ConversationMessage::tool_result(pending_tool.id, pending_tool.name, output, is_error)
    }

    fn apply_prompt_updates(&mut self, message: &ConversationMessage) -> Result<(), RuntimeError> {
        let Some(prompt_augmenter) = self.prompt_augmenter.as_mut() else {
            return Ok(());
        };
        let updates = prompt_augmenter.on_tool_result(&self.session, message);
        if updates.is_empty() {
            return Ok(());
        }

        let labels = updates
            .iter()
            .map(|update| update.label.clone())
            .collect::<Vec<_>>();
        self.system_prompt.extend(
            updates
                .into_iter()
                .map(|update| update.content)
                .filter(|content| !content.trim().is_empty()),
        );

        let instructions_loaded = self.hooks.run_instructions_loaded(&labels);
        if instructions_loaded.is_denied() {
            return Err(RuntimeError::new(join_hook_feedback(
                instructions_loaded.messages(),
                "instructions-loaded hook denied execution",
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingToolUse {
    id: String,
    name: String,
    input: String,
    required_permission: PermissionMode,
}

fn build_assistant_message(
    events: Vec<AssistantEvent>,
) -> Result<(ConversationMessage, Vec<PendingToolUse>, Option<TokenUsage>), RuntimeError> {
    let mut blocks = Vec::new();
    let mut text = String::new();
    let mut pending = Vec::new();
    let mut usage = None;
    let mut saw_stop = false;
    let mut finish_reason = None;

    for event in events {
        match event {
            AssistantEvent::ToolCallDelta {
                index,
                id,
                name,
                arguments_delta,
            } => {
                flush_text(&mut text, &mut blocks);
                if let Some(ContentBlock::ToolCallDraft { id: previous_id, name: previous_name, input, .. }) = blocks.iter_mut().find(|b| matches!(b, ContentBlock::ToolCallDraft { index: current, .. } if *current == index)) {
                    if id.is_some() { *previous_id = id; }
                    if name.is_some() { *previous_name = name; }
                    input.push_str(&arguments_delta);
                } else {
                    blocks.push(ContentBlock::ToolCallDraft { index, id, name, input: arguments_delta });
                }
            }
            AssistantEvent::ReasoningDelta {
                text: delta,
                field,
                provider,
                model,
            } => {
                flush_text(&mut text, &mut blocks);
                if let Some(ContentBlock::Reasoning {
                    text,
                    field: previous,
                    ..
                }) = blocks.last_mut()
                {
                    if *previous == field {
                        text.push_str(&delta);
                        continue;
                    }
                }
                blocks.push(ContentBlock::Reasoning {
                    text: delta,
                    field,
                    provider,
                    model,
                });
            }
            AssistantEvent::Finish { reason } => {
                finish_reason = Some(reason);
            }
            AssistantEvent::TextDelta(delta) => text.push_str(&delta),
            AssistantEvent::ToolUse {
                id,
                name,
                input,
                required_permission,
            } => {
                flush_text(&mut text, &mut blocks);
                let completed = ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                };
                if let Some(draft) = blocks.iter_mut().find(|block| matches!(block, ContentBlock::ToolCallDraft { id: Some(draft_id), .. } if *draft_id == id)) {
                    *draft = completed;
                } else { blocks.push(completed); }
                pending.push(PendingToolUse {
                    id,
                    name,
                    input,
                    required_permission,
                });
            }
            AssistantEvent::Usage(value) => usage = Some(value),
            AssistantEvent::MessageStop => saw_stop = true,
        }
    }
    flush_text(&mut text, &mut blocks);

    if !saw_stop {
        return Err(RuntimeError::new(
            "assistant stream ended without a stop event",
        ));
    }
    if blocks.is_empty() {
        return Err(RuntimeError::new("assistant stream produced no content"));
    }
    if blocks
        .iter()
        .any(|block| matches!(block, ContentBlock::ToolCallDraft { .. }))
    {
        return Err(RuntimeError::new(
            "provider_output_incomplete: unfinished tool parameters; received output saved",
        ));
    }
    if finish_reason
        .as_deref()
        .is_some_and(|reason| !matches!(reason, "stop" | "tool_calls" | "function_call"))
    {
        return Err(RuntimeError::new(format!(
            "provider_output_incomplete: {}; received output saved",
            finish_reason.as_deref().unwrap_or("unknown")
        )));
    }
    let mut message = ConversationMessage::assistant(blocks, usage);
    message.finish_reason = finish_reason;
    Ok((message, pending, usage))
}

fn flush_text(text: &mut String, blocks: &mut Vec<ContentBlock>) {
    if !text.is_empty() {
        blocks.push(ContentBlock::Text {
            text: std::mem::take(text),
        });
    }
}

#[cfg(test)]
mod response_context_regressions {
    use super::*;

    #[test]
    fn expected_process_waits_preserve_the_guard_against_unproductive_polling() {
        struct WaitApi {
            calls: usize,
            wait_ms: u64,
        }
        impl ApiClient for WaitApi {
            fn stream(&mut self, _: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
                self.calls += 1;
                Ok(if self.calls <= 4 {
                    vec![
                        AssistantEvent::ToolUse {
                            id: format!("wait-{}", self.calls),
                            name: "Process".into(),
                            input: format!(r#"{{"action":"inspect","waitMs":{}}}"#, self.wait_ms),
                            required_permission: PermissionMode::ReadOnly,
                        },
                        AssistantEvent::MessageStop,
                    ]
                } else {
                    vec![
                        AssistantEvent::TextDelta("completed waiting".into()),
                        AssistantEvent::MessageStop,
                    ]
                })
            }
        }
        for (wait_ms, success) in [(2000, true), (0, false)] {
            let mut runtime = ConversationRuntime::new(
                Session::new(),
                WaitApi { calls: 0, wait_ms },
                StaticToolExecutor::new()
                    .register("Process", |_| Ok(r#"{"status":"running"}"#.into())),
                PermissionPolicy::new(PermissionMode::ReadOnly),
                vec![],
                HookRunner::default(),
            );
            assert_eq!(
                runtime.run_turn("Wait for the owned process", None).is_ok(),
                success
            );
        }
    }

    #[test]
    fn compaction_invalidates_measured_usage_from_the_previous_projection() {
        let mut assistant = ConversationMessage::assistant(
            vec![ContentBlock::Text { text: "ok".into() }],
            Some(TokenUsage {
                input_tokens: 10000,
                ..Default::default()
            }),
        );
        assistant.request_overhead_tokens = Some(20);
        let session = Session::from_messages(vec![
            ConversationMessage::user("old constraint"),
            assistant,
            ConversationMessage::user("latest"),
        ]);
        assert!(measured_context_tokens(&session, 20) >= 10000);
        let compacted = crate::compact_session_with_summary(
            &session,
            CompactConfig {
                preserve_recent_messages: 2,
                max_estimated_tokens: 0,
            },
            Some("Keep the constraint"),
        );
        assert!(measured_context_tokens(&compacted.compacted_session, 20) < 1000);
    }

    struct LongTask {
        calls: usize,
        summaries: usize,
    }
    impl ApiClient for LongTask {
        fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            assert!(estimate_session_tokens(&Session::from_messages(request.messages)) < 2300);
            self.calls += 1;
            if self.calls <= 5 {
                Ok(vec![
                    AssistantEvent::ToolUse {
                        id: format!("call-{}", self.calls),
                        name: "read".into(),
                        input: self.calls.to_string(),
                        required_permission: PermissionMode::ReadOnly,
                    },
                    AssistantEvent::MessageStop,
                ])
            } else {
                Ok(vec![
                    AssistantEvent::TextDelta("done".into()),
                    AssistantEvent::MessageStop,
                ])
            }
        }
        fn summarize(
            &mut self,
            messages: &[ConversationMessage],
            _: usize,
        ) -> Result<Option<(String, TokenUsage)>, RuntimeError> {
            assert!(!messages.is_empty());
            self.summaries += 1;
            Ok(Some((
                "User constraint: keep KEEP_ME. File reads verified; continue remaining work."
                    .into(),
                TokenUsage::default(),
            )))
        }
    }
    struct LargeRead;
    impl ToolExecutor for LargeRead {
        fn execute(&mut self, _: &str, _: &str) -> Result<String, ToolError> {
            Ok("data ".repeat(1000))
        }
    }

    #[test]
    fn compacts_between_tools_and_preserves_archived_sources() {
        let mut runtime = ConversationRuntime::new(
            Session::new(),
            LongTask {
                calls: 0,
                summaries: 0,
            },
            LargeRead,
            PermissionPolicy::new(PermissionMode::ReadOnly),
            vec![],
            HookRunner::default(),
        )
        .with_context_config(crate::ContextOptimizerConfig {
            context_window_tokens_override: Some(2300),
            max_output_tokens: 100,
            auto_compact_buffer_tokens: 50,
            ..Default::default()
        });
        runtime
            .run_turn("KEEP_ME is a mandatory user constraint", None)
            .unwrap();
        assert!(runtime.api_client.summaries >= 3);
        assert!(runtime
            .session
            .context_collapse_archive
            .iter()
            .any(|m| m.first_text().is_some_and(|text| text.contains("KEEP_ME"))));
        assert!(!runtime.session.context_collapse_commits.is_empty());
    }

    #[test]
    fn failed_summary_keeps_the_original_session_and_archive() {
        struct FailedSummary;
        impl ApiClient for FailedSummary {
            fn stream(&mut self, _: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
                panic!("Oversized history must be compacted before requesting an answer")
            }
            fn summarize(
                &mut self,
                _: &[ConversationMessage],
                _: usize,
            ) -> Result<Option<(String, TokenUsage)>, RuntimeError> {
                Err(RuntimeError::new("summary unavailable"))
            }
        }
        let session = Session::from_messages(vec![
            ConversationMessage::user("KEEP_ME ".repeat(200)),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "old result".repeat(200),
                }],
                None,
            ),
        ]);
        let mut runtime = ConversationRuntime::new(
            session.clone(),
            FailedSummary,
            LargeRead,
            PermissionPolicy::new(PermissionMode::ReadOnly),
            vec![],
            HookRunner::default(),
        )
        .with_context_config(crate::ContextOptimizerConfig {
            context_window_tokens_override: Some(800),
            max_output_tokens: 100,
            auto_compact_buffer_tokens: 50,
            ..Default::default()
        });
        assert!(runtime
            .run_turn("latest correction", None)
            .unwrap_err()
            .to_string()
            .contains("summary unavailable"));
        assert_eq!(
            &runtime.session.messages[..session.messages.len()],
            &session.messages
        );
        assert!(runtime.session.context_collapse_archive.is_empty());
        assert!(runtime.session.context_collapse_commits.is_empty());
    }

    #[test]
    fn truncated_response_cannot_dispatch_tools() {
        let events = vec![
            AssistantEvent::ToolUse {
                id: "call".into(),
                name: "write".into(),
                input: "{}".into(),
                required_permission: PermissionMode::WorkspaceWrite,
            },
            AssistantEvent::Finish {
                reason: "length".into(),
            },
            AssistantEvent::MessageStop,
        ];
        assert!(build_assistant_message(events)
            .unwrap_err()
            .to_string()
            .contains("provider_output_incomplete"));
    }
}

fn join_hook_feedback(messages: &[String], fallback: &str) -> String {
    if messages.is_empty() {
        return fallback.to_string();
    }
    messages.join("\n")
}

pub fn is_blocking_wait_result(tool: &str, input: &str, output: &str) -> bool {
    let (Ok(input), Ok(output)) = (
        serde_json::from_str::<Value>(input),
        serde_json::from_str::<Value>(output),
    ) else {
        return false;
    };
    let active =
        |status: Option<&str>| matches!(status, Some("queued" | "running" | "waiting" | "ready"));
    match (tool, input["action"].as_str()) {
        ("Subagents", Some("wait")) => {
            input["timeoutMs"].as_u64().unwrap_or(10000) >= 1000
                && active(output["status"].as_str())
        }
        ("Process", Some("inspect")) => {
            input["waitMs"].as_u64().unwrap_or(0) >= 1000 && active(output["status"].as_str())
        }
        ("Process", Some("read")) => {
            input["waitMs"].as_u64().unwrap_or(0) >= 1000
                && output["readTimedOut"] == true
                && output["exited"] == false
        }
        _ => false,
    }
}

fn merge_feedback(mut output: String, messages: &[String], denied: bool) -> String {
    if messages.is_empty() {
        return output;
    }
    if !output.trim().is_empty() {
        output.push_str("\n\n");
    }
    output.push_str(if denied {
        "Hook feedback (denied):\n"
    } else {
        "Hook feedback:\n"
    });
    output.push_str(&messages.join("\n"));
    output
}

type ToolHandler = Box<dyn FnMut(&str) -> Result<String, ToolError>>;

#[derive(Default)]
pub struct StaticToolExecutor {
    handlers: BTreeMap<String, ToolHandler>,
    definitions: BTreeMap<String, RuntimeToolDefinition>,
}

impl StaticToolExecutor {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn register(
        mut self,
        tool_name: impl Into<String>,
        handler: impl FnMut(&str) -> Result<String, ToolError> + 'static,
    ) -> Self {
        let tool_name = tool_name.into();
        self.definitions.insert(
            tool_name.clone(),
            RuntimeToolDefinition {
                name: tool_name.clone(),
                description: format!("Registered tool `{tool_name}`"),
                input_schema: serde_json::json!({ "type": "string" }),
                required_permission: PermissionMode::ReadOnly,
            },
        );
        self.handlers.insert(tool_name, Box::new(handler));
        self
    }
}

impl ToolExecutor for StaticToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        self.handlers
            .get_mut(tool_name)
            .ok_or_else(|| ToolError::new(format!("unknown tool `{tool_name}`")))?(input)
    }

    fn definitions(&self) -> Vec<RuntimeToolDefinition> {
        self.definitions.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApiClient, ApiRequest, AssistantEvent, ConversationRuntime, RuntimeError,
        RuntimePromptAugmenter, RuntimePromptUpdate, StaticToolExecutor, TurnLimits,
    };
    use crate::hooks::HookRunner;
    use crate::permissions::{
        PermissionDecision, PermissionMode, PermissionPolicy, PermissionPrompter, PermissionRequest,
    };
    use crate::session::{ContentBlock, MessageRole, Session};
    use crate::usage::TokenUsage;

    struct ScriptedApi {
        calls: usize,
    }

    #[test]
    fn unlimited_turn_runs_past_the_old_cap_and_explicit_cap_still_stops() {
        struct CountingApi(usize);
        impl ApiClient for CountingApi {
            fn stream(
                &mut self,
                _request: ApiRequest,
            ) -> Result<Vec<AssistantEvent>, RuntimeError> {
                self.0 += 1;
                if self.0 > 501 {
                    return Ok(vec![
                        AssistantEvent::TextDelta("done".into()),
                        AssistantEvent::MessageStop,
                    ]);
                }
                Ok(vec![
                    AssistantEvent::ToolUse {
                        id: format!("call-{}", self.0),
                        name: "step".into(),
                        input: self.0.to_string(),
                        required_permission: PermissionMode::ReadOnly,
                    },
                    AssistantEvent::MessageStop,
                ])
            }
        }
        for (settings, expected_cap) in [
            (serde_json::json!({}), None),
            (serde_json::json!({"execution":{"maxIterations":0}}), None),
            (
                serde_json::json!({"execution":{"maxIterations":2}}),
                Some(2),
            ),
        ] {
            let mut runtime = ConversationRuntime::new(
                Session::new(),
                CountingApi(0),
                StaticToolExecutor::new().register("step", |input| Ok(input.to_string())),
                PermissionPolicy::new(PermissionMode::ReadOnly),
                vec![],
                HookRunner::default(),
            )
            .with_limits(TurnLimits::from_settings(&settings));
            let result = runtime.run_turn("complete every step", None);
            if let Some(cap) = expected_cap {
                assert!(result.unwrap_err().to_string().contains("iteration limit"));
                assert_eq!(runtime.api_client.0, cap);
            } else {
                let summary = result.unwrap();
                assert_eq!(summary.iterations, 502);
                assert_eq!(summary.tool_results.len(), 501);
            }
        }
    }

    impl ApiClient for ScriptedApi {
        fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            self.calls += 1;
            match self.calls {
                1 => {
                    assert!(request
                        .messages
                        .iter()
                        .any(|message| message.role == MessageRole::User));
                    assert_eq!(request.tools.len(), 2);
                    Ok(vec![
                        AssistantEvent::TextDelta("Checking.".to_string()),
                        AssistantEvent::ToolUse {
                            id: "tool-1".to_string(),
                            name: "add".to_string(),
                            input: "2,2".to_string(),
                            required_permission: PermissionMode::ReadOnly,
                        },
                        AssistantEvent::Usage(TokenUsage {
                            input_tokens: 10,
                            output_tokens: 4,
                            cache_creation_input_tokens: 0,
                            cache_read_input_tokens: 1,
                        }),
                        AssistantEvent::MessageStop,
                    ])
                }
                2 => Ok(vec![
                    AssistantEvent::ToolUse {
                        id: "tool-2".to_string(),
                        name: "noop".to_string(),
                        input: "{}".to_string(),
                        required_permission: PermissionMode::ReadOnly,
                    },
                    AssistantEvent::MessageStop,
                ]),
                3 => Ok(vec![
                    AssistantEvent::TextDelta("Answer: 4".to_string()),
                    AssistantEvent::MessageStop,
                ]),
                _ => Err(RuntimeError::new("too many calls")),
            }
        }
    }

    struct RejectPrompt;

    struct OneShotPromptAugmenter;

    impl RuntimePromptAugmenter for OneShotPromptAugmenter {
        fn on_tool_result(
            &mut self,
            _session: &Session,
            tool_result: &crate::session::ConversationMessage,
        ) -> Vec<RuntimePromptUpdate> {
            match &tool_result.blocks[0] {
                ContentBlock::ToolResult { tool_name, .. } if tool_name == "add" => {
                    vec![RuntimePromptUpdate {
                        label: "conditional-skill:math".to_string(),
                        content: "Dynamic math guidance".to_string(),
                    }]
                }
                _ => Vec::new(),
            }
        }
    }

    impl PermissionPrompter for RejectPrompt {
        fn decide(&mut self, request: &PermissionRequest) -> PermissionDecision {
            assert_eq!(request.tool_name, "write_file");
            PermissionDecision::Deny {
                reason: "rejected".to_string(),
            }
        }
    }

    #[test]
    fn runs_tool_loop_and_tracks_usage() {
        let api = ScriptedApi { calls: 0 };
        let tools = StaticToolExecutor::new().register("add", |input| {
            let total = input
                .split(',')
                .map(|part| part.parse::<i32>().expect("integer"))
                .sum::<i32>();
            Ok(total.to_string())
        });
        let mut runtime = ConversationRuntime::new(
            Session::new(),
            api,
            tools.register("noop", |_input| Ok("ok".to_string())),
            PermissionPolicy::new(PermissionMode::ReadOnly),
            vec!["system".to_string()],
            HookRunner::default(),
        )
        .with_prompt_augmenter(Box::new(OneShotPromptAugmenter));

        let summary = runtime.run_turn("2+2", None).expect("turn succeeds");
        assert_eq!(summary.iterations, 3);
        assert_eq!(summary.tool_results.len(), 2);
        assert_eq!(summary.usage.total_tokens(), 15);
        assert!(matches!(
            runtime.session().messages[1].blocks[1],
            ContentBlock::ToolUse { .. }
        ));
        assert!(runtime
            .system_prompt
            .iter()
            .any(|value| value.contains("Dynamic math guidance")));
    }

    #[test]
    fn denied_permission_produces_tool_error_message() {
        struct OneShotApi;
        impl ApiClient for OneShotApi {
            fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
                if request
                    .messages
                    .iter()
                    .any(|message| message.role == MessageRole::Tool)
                {
                    return Ok(vec![
                        AssistantEvent::TextDelta("done".to_string()),
                        AssistantEvent::MessageStop,
                    ]);
                }
                Ok(vec![
                    AssistantEvent::ToolUse {
                        id: "tool-1".to_string(),
                        name: "write_file".to_string(),
                        input: "{}".to_string(),
                        required_permission: PermissionMode::WorkspaceWrite,
                    },
                    AssistantEvent::MessageStop,
                ])
            }
        }

        let mut runtime = ConversationRuntime::new(
            Session::new(),
            OneShotApi,
            StaticToolExecutor::new(),
            PermissionPolicy::new(PermissionMode::ReadOnly),
            vec!["system".to_string()],
            HookRunner::default(),
        );
        let summary = runtime
            .run_turn("write", Some(&mut RejectPrompt))
            .expect("turn succeeds");
        assert!(matches!(
            &summary.tool_results[0].blocks[0],
            ContentBlock::ToolResult { is_error: true, output, .. } if output == "rejected"
        ));
    }
}

fn measured_context_tokens(session: &Session, overhead: usize) -> usize {
    let estimate = estimate_session_tokens(session).saturating_add(overhead);
    let generation = session.context_collapse_commits.len();
    let previous = session
        .messages
        .iter()
        .enumerate()
        .rev()
        .find(|(_, message)| {
            message.usage.is_some()
                && message.context_generation == generation
                && message.request_overhead_tokens.is_some()
        });
    if let Some((index, message)) = previous {
        let measured = message.usage.unwrap().total_tokens() as usize;
        let trailing = estimate_session_tokens(&Session::from_messages(
            session.messages[index + 1..].to_vec(),
        ));
        estimate.max(
            measured.saturating_add(trailing).saturating_add(
                overhead.saturating_sub(message.request_overhead_tokens.unwrap_or(0)),
            ),
        )
    } else {
        estimate
    }
}

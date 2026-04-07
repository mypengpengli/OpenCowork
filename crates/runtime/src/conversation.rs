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

pub struct ConversationRuntime<C, T> {
    session: Session,
    api_client: C,
    tool_executor: T,
    permission_policy: PermissionPolicy,
    system_prompt: Vec<String>,
    max_iterations: usize,
    usage_tracker: UsageTracker,
    hooks: HookRunner,
    prompt_augmenter: Option<Box<dyn RuntimePromptAugmenter>>,
    tool_result_budget: ToolResultBudgetConfig,
}

pub trait RuntimeObserver {
    fn on_assistant_event(&mut self, _event: &AssistantEvent) {}

    fn on_tool_result(&mut self, _message: &ConversationMessage) {}
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
            max_iterations: usize::MAX,
            usage_tracker,
            hooks,
            prompt_augmenter: None,
            tool_result_budget: ToolResultBudgetConfig::default(),
        }
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

        let mut assistant_messages = Vec::new();
        let mut tool_results = Vec::new();
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > self.max_iterations {
                return Err(RuntimeError::new(
                    "tool loop exceeded the configured iteration cap",
                ));
            }

            let request_messages =
                budget_session_tool_results(&self.session, self.tool_result_budget.clone())
                    .messages;
            let request = ApiRequest {
                system_prompt: self.system_prompt.clone(),
                messages: request_messages,
                tools: self.tool_executor.definitions(),
            };
            let events = if let Some(ref mut observer) = observer {
                self.api_client
                    .stream_with_observer(request, &mut |event| {
                        observer.on_assistant_event(event);
                    })?
            } else {
                self.api_client.stream(request)?
            };
            let (assistant_message, pending, usage) = build_assistant_message(events)?;
            if let Some(usage) = usage {
                self.usage_tracker.record(usage);
            }
            self.session.messages.push(assistant_message.clone());
            assistant_messages.push(assistant_message);

            if pending.is_empty() {
                break;
            }

            for pending_tool in pending {
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
                self.session.messages.push(message.clone());
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
        estimate_session_tokens(&self.session)
    }

    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    #[must_use]
    pub fn usage(&self) -> &UsageTracker {
        &self.usage_tracker
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

    for event in events {
        match event {
            AssistantEvent::TextDelta(delta) => text.push_str(&delta),
            AssistantEvent::ToolUse {
                id,
                name,
                input,
                required_permission,
            } => {
                flush_text(&mut text, &mut blocks);
                blocks.push(ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });
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
    Ok((
        ConversationMessage::assistant(blocks, usage),
        pending,
        usage,
    ))
}

fn flush_text(text: &mut String, blocks: &mut Vec<ContentBlock>) {
    if !text.is_empty() {
        blocks.push(ContentBlock::Text {
            text: std::mem::take(text),
        });
    }
}

fn join_hook_feedback(messages: &[String], fallback: &str) -> String {
    if messages.is_empty() {
        return fallback.to_string();
    }
    messages.join("\n")
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
        RuntimePromptAugmenter, RuntimePromptUpdate, StaticToolExecutor,
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

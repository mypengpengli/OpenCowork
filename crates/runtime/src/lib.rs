mod compact;
mod config;
mod context;
mod conversation;
mod hooks;
mod memory;
mod model_context;
mod permissions;
mod process;
mod prompt;
mod provider_bridge;
mod session;
mod session_memory;
mod session_store;
mod usage;

pub use compact::{
    budget_message_tool_results, budget_session_tool_results, collapse_session_messages,
    compact_session, compacted_summary_prefix_len, estimate_session_tokens,
    microcompact_session_messages, should_compact, snip_session_messages, CompactConfig,
    CompactResult, MessageCollapseConfig, MicrocompactResult, SnipConfig, SnipResult,
    ToolResultBudgetConfig, MICROCOMPACT_CLEARED_MESSAGE,
};
pub use config::{
    default_config_home, ConfigEntry, ConfigLoader, ConfigSource, RuntimeConfig,
    RuntimeContextConfig, RuntimeFeatureConfig, RuntimeHookCommand, RuntimeHookConfig,
    RuntimeHookEvent, RuntimeLspConfig, RuntimeLspServerConfig, RuntimePluginConfig,
    RuntimeProviderConfig, RuntimeProviderKind, RuntimeTeamMemorySyncConfig,
};
pub use context::{ContextOptimizer, ContextOptimizerConfig, ContextSelection, SessionPreparation};
pub use conversation::{
    ApiClient, ApiRequest, AssistantEvent, ConversationRuntime, RuntimeError, RuntimeObserver,
    RuntimePromptAugmenter, RuntimePromptUpdate, RuntimeToolDefinition, StaticToolExecutor,
    ToolError, ToolExecutor, TurnSummary,
};
pub use hooks::{HookOutcome, HookRunner};
pub use memory::{
    discover_project_memory_source, discover_relevant_memory_sources,
    discover_relevant_memory_sources_with_provider, discover_team_memory_source,
    hydrate_current_session_memory, load_current_session_memory_source, project_memory_entrypoint,
    project_memory_root, project_session_memory_path, project_session_memory_state_path,
    project_team_memory_entrypoint, project_team_memory_root, refresh_current_session_memory,
    refresh_current_session_memory_with_provider, should_report_instruction_file,
    touched_paths_from_message, touched_paths_from_session,
};
pub use model_context::{
    calculate_context_thresholds, get_auto_compact_threshold, get_auto_tool_search_char_threshold,
    get_auto_tool_search_percentage, get_context_window_for_model,
    get_effective_context_window_size, get_skill_char_budget, ContextThresholds,
    AUTOCOMPACT_BUFFER_TOKENS, COMPACT_MAX_OUTPUT_TOKENS,
    DEFAULT_MAX_TOOL_RESULTS_PER_MESSAGE_CHARS, DEFAULT_MAX_TOOL_RESULT_CHARS,
    DEFAULT_SKILL_CHAR_BUDGET, DEFAULT_TOOL_RESULT_PREVIEW_CHARS, ERROR_THRESHOLD_BUFFER_TOKENS,
    MANUAL_COMPACT_BUFFER_TOKENS, MAX_OUTPUT_TOKENS_DEFAULT, MAX_SKILL_LISTING_DESC_CHARS,
    SKILL_BUDGET_CHARS_PER_TOKEN, SKILL_BUDGET_CONTEXT_PERCENT, WARNING_THRESHOLD_BUFFER_TOKENS,
};
pub use permissions::{
    PermissionDecision, PermissionMode, PermissionPolicy, PermissionPrompter, PermissionRequest,
};
pub use process::ProcessTree;
pub use prompt::{
    discover_instruction_sources, discover_nested_instruction_sources,
    skill_instruction_source_from_output, skill_instruction_sources_from_session,
    InstructionSource, ProjectContext, PromptBundle, PromptComposer, PromptLayer,
};
pub use provider_bridge::ProviderBackedApiClient;
pub use session::{
    ContentBlock, ContextCollapseCommit, ContextCollapseSnapshot, ConversationMessage, MessageRole,
    Session, SessionError, SessionMemoryState,
};
pub use session_memory::{
    maybe_refresh_session_memory, maybe_refresh_session_memory_with_config,
    maybe_schedule_session_memory_refresh, maybe_schedule_session_memory_refresh_with_task,
    should_refresh_session_memory, wait_for_session_memory_refresh, SessionMemoryConfig,
};
pub use session_store::{SessionDescriptor, SessionStore};
pub use usage::{TokenUsage, UsageTracker};

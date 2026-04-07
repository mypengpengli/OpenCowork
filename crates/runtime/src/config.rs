use opencowork_api::{DEFAULT_OPENAI_BASE_URL, DEFAULT_XAI_BASE_URL};
use opencowork_mcp::{
    McpAuthConfig, McpOAuthConfig, McpServerDefinition, McpToolDefinition, McpToolPermission,
    McpTransport,
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{
    AUTOCOMPACT_BUFFER_TOKENS, COMPACT_MAX_OUTPUT_TOKENS,
    DEFAULT_MAX_TOOL_RESULTS_PER_MESSAGE_CHARS, DEFAULT_MAX_TOOL_RESULT_CHARS,
    ERROR_THRESHOLD_BUFFER_TOKENS, MANUAL_COMPACT_BUFFER_TOKENS, MAX_OUTPUT_TOKENS_DEFAULT,
    WARNING_THRESHOLD_BUFFER_TOKENS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfigSource {
    User,
    Project,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEntry {
    pub source: ConfigSource,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeHookConfig {
    entries: BTreeMap<RuntimeHookEvent, Vec<RuntimeHookCommand>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuntimeHookEvent {
    PreToolUse,
    PostToolUse,
    PostToolUseFailure,
    SessionStart,
    SessionEnd,
    UserPromptSubmit,
    PermissionDenied,
    InstructionsLoaded,
    FileChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeHookCommand {
    command: String,
    matcher: Option<String>,
    once: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimePluginConfig {
    enabled_plugins: BTreeMap<String, bool>,
    external_directories: Vec<String>,
    install_root: Option<String>,
    bundled_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeProviderKind {
    OpenAiCompatible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeProviderConfig {
    kind: RuntimeProviderKind,
    name: String,
    api_key_env: String,
    base_url: String,
    base_url_env: Option<String>,
    timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeContextConfig {
    preserve_recent_messages: usize,
    context_window_tokens: Option<usize>,
    max_instruction_tokens: usize,
    max_output_tokens: usize,
    auto_compact_buffer_tokens: usize,
    warning_buffer_tokens: usize,
    error_buffer_tokens: usize,
    manual_compact_buffer_tokens: usize,
    message_collapse_chars: usize,
    max_tool_result_chars: usize,
    max_tool_results_per_message_chars: usize,
    instruction_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeLspConfig {
    servers: BTreeMap<String, RuntimeLspServerConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTeamMemorySyncConfig {
    enabled: bool,
    endpoint: Option<String>,
    token_env: Option<String>,
    repo: Option<String>,
    timeout_ms: u64,
    debounce_ms: u64,
    poll_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeLspServerConfig {
    name: String,
    command: String,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    workspace_root: Option<String>,
    initialization_options: Option<Value>,
    extension_to_language: BTreeMap<String, String>,
}

impl Default for RuntimeContextConfig {
    fn default() -> Self {
        Self {
            preserve_recent_messages: 8,
            context_window_tokens: None,
            max_instruction_tokens: 12_000,
            max_output_tokens: MAX_OUTPUT_TOKENS_DEFAULT,
            auto_compact_buffer_tokens: AUTOCOMPACT_BUFFER_TOKENS,
            warning_buffer_tokens: WARNING_THRESHOLD_BUFFER_TOKENS,
            error_buffer_tokens: ERROR_THRESHOLD_BUFFER_TOKENS,
            manual_compact_buffer_tokens: MANUAL_COMPACT_BUFFER_TOKENS,
            message_collapse_chars: 8_000,
            max_tool_result_chars: DEFAULT_MAX_TOOL_RESULT_CHARS,
            max_tool_results_per_message_chars: DEFAULT_MAX_TOOL_RESULTS_PER_MESSAGE_CHARS,
            instruction_files: vec![
                "CLAUDE.md".to_string(),
                "TODOLIST.md".to_string(),
                "README.md".to_string(),
            ],
        }
    }
}

impl Default for RuntimeTeamMemorySyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: std::env::var("OPENCOWORK_TEAM_MEMORY_SYNC_URL").ok(),
            token_env: std::env::var("OPENCOWORK_TEAM_MEMORY_SYNC_TOKEN_ENV")
                .ok()
                .or_else(|| Some("OPENCOWORK_TEAM_MEMORY_SYNC_TOKEN".to_string())),
            repo: std::env::var("OPENCOWORK_TEAM_MEMORY_SYNC_REPO").ok(),
            timeout_ms: 30_000,
            debounce_ms: 2_000,
            poll_ms: 2_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RuntimeFeatureConfig {
    hooks: RuntimeHookConfig,
    plugins: RuntimePluginConfig,
    mcp_servers: BTreeMap<String, McpServerDefinition>,
    model: Option<String>,
    provider: Option<RuntimeProviderConfig>,
    context: RuntimeContextConfig,
    lsp: RuntimeLspConfig,
    team_memory_sync: RuntimeTeamMemorySyncConfig,
    permission_mode: Option<crate::permissions::PermissionMode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeConfig {
    merged: Value,
    loaded_entries: Vec<ConfigEntry>,
    feature_config: RuntimeFeatureConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigLoader {
    cwd: PathBuf,
    config_home: PathBuf,
}

impl ConfigLoader {
    #[must_use]
    pub fn new(cwd: impl Into<PathBuf>, config_home: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            config_home: config_home.into(),
        }
    }

    #[must_use]
    pub fn default_for(cwd: impl Into<PathBuf>) -> Self {
        Self::new(cwd, default_config_home())
    }

    #[must_use]
    pub fn discover(&self) -> Vec<ConfigEntry> {
        vec![
            ConfigEntry {
                source: ConfigSource::User,
                path: self.config_home.join("settings.json"),
            },
            ConfigEntry {
                source: ConfigSource::Project,
                path: self.cwd.join(".opencowork.json"),
            },
            ConfigEntry {
                source: ConfigSource::Project,
                path: self.cwd.join(".opencowork").join("settings.json"),
            },
            ConfigEntry {
                source: ConfigSource::Local,
                path: self.cwd.join(".opencowork").join("settings.local.json"),
            },
        ]
    }

    pub fn load(&self) -> Result<RuntimeConfig, ConfigError> {
        let mut merged = Value::Object(Map::new());
        let mut loaded_entries = Vec::new();

        for entry in self.discover() {
            let Some(value) = read_optional_object(&entry.path)? else {
                continue;
            };
            deep_merge(&mut merged, &value);
            loaded_entries.push(entry);
        }

        let feature_config = RuntimeFeatureConfig {
            hooks: parse_hooks(&merged)?,
            plugins: parse_plugins(&merged)?,
            mcp_servers: parse_mcp_servers(&merged)?,
            model: parse_model(&merged),
            provider: parse_provider(&merged)?,
            context: parse_context(&merged)?,
            lsp: parse_lsp(&merged)?,
            team_memory_sync: parse_team_memory_sync(&merged)?,
            permission_mode: parse_permission_mode(&merged)?,
        };

        Ok(RuntimeConfig {
            merged,
            loaded_entries,
            feature_config,
        })
    }
}

impl RuntimeConfig {
    #[must_use]
    pub fn merged(&self) -> &Value {
        &self.merged
    }

    #[must_use]
    pub fn loaded_entries(&self) -> &[ConfigEntry] {
        &self.loaded_entries
    }

    #[must_use]
    pub fn feature_config(&self) -> &RuntimeFeatureConfig {
        &self.feature_config
    }

    #[must_use]
    pub fn hooks(&self) -> &RuntimeHookConfig {
        &self.feature_config.hooks
    }

    #[must_use]
    pub fn plugins(&self) -> &RuntimePluginConfig {
        &self.feature_config.plugins
    }

    #[must_use]
    pub fn permission_mode(&self) -> Option<crate::permissions::PermissionMode> {
        self.feature_config.permission_mode
    }

    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.feature_config.model.as_deref()
    }

    #[must_use]
    pub fn provider(&self) -> Option<&RuntimeProviderConfig> {
        self.feature_config.provider.as_ref()
    }

    #[must_use]
    pub fn context(&self) -> &RuntimeContextConfig {
        &self.feature_config.context
    }

    #[must_use]
    pub fn mcp_servers(&self) -> &BTreeMap<String, McpServerDefinition> {
        &self.feature_config.mcp_servers
    }

    #[must_use]
    pub fn lsp(&self) -> &RuntimeLspConfig {
        &self.feature_config.lsp
    }

    #[must_use]
    pub fn team_memory_sync(&self) -> &RuntimeTeamMemorySyncConfig {
        &self.feature_config.team_memory_sync
    }
}

impl RuntimeFeatureConfig {
    #[must_use]
    pub fn hooks(&self) -> &RuntimeHookConfig {
        &self.hooks
    }

    #[must_use]
    pub fn plugins(&self) -> &RuntimePluginConfig {
        &self.plugins
    }

    #[must_use]
    pub fn permission_mode(&self) -> Option<crate::permissions::PermissionMode> {
        self.permission_mode
    }

    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    #[must_use]
    pub fn provider(&self) -> Option<&RuntimeProviderConfig> {
        self.provider.as_ref()
    }

    #[must_use]
    pub fn context(&self) -> &RuntimeContextConfig {
        &self.context
    }

    #[must_use]
    pub fn mcp_servers(&self) -> &BTreeMap<String, McpServerDefinition> {
        &self.mcp_servers
    }

    #[must_use]
    pub fn lsp(&self) -> &RuntimeLspConfig {
        &self.lsp
    }

    #[must_use]
    pub fn team_memory_sync(&self) -> &RuntimeTeamMemorySyncConfig {
        &self.team_memory_sync
    }
}

impl RuntimeHookConfig {
    #[must_use]
    pub fn with_commands(entries: BTreeMap<RuntimeHookEvent, Vec<RuntimeHookCommand>>) -> Self {
        Self { entries }
    }

    #[must_use]
    pub fn commands(&self, event: RuntimeHookEvent) -> &[RuntimeHookCommand] {
        self.entries.get(&event).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn entries(&self) -> &BTreeMap<RuntimeHookEvent, Vec<RuntimeHookCommand>> {
        &self.entries
    }

    pub fn extend_command_strings(
        &mut self,
        event: RuntimeHookEvent,
        commands: impl IntoIterator<Item = String>,
    ) {
        self.entries
            .entry(event)
            .or_default()
            .extend(commands.into_iter().map(RuntimeHookCommand::command_only));
    }
}

impl RuntimeHookEvent {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::PreToolUse => "PreToolUse",
            Self::PostToolUse => "PostToolUse",
            Self::PostToolUseFailure => "PostToolUseFailure",
            Self::SessionStart => "SessionStart",
            Self::SessionEnd => "SessionEnd",
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::PermissionDenied => "PermissionDenied",
            Self::InstructionsLoaded => "InstructionsLoaded",
            Self::FileChanged => "FileChanged",
        }
    }

    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::PreToolUse,
            Self::PostToolUse,
            Self::PostToolUseFailure,
            Self::SessionStart,
            Self::SessionEnd,
            Self::UserPromptSubmit,
            Self::PermissionDenied,
            Self::InstructionsLoaded,
            Self::FileChanged,
        ]
    }
}

impl RuntimeHookCommand {
    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }

    #[must_use]
    pub fn matcher(&self) -> Option<&str> {
        self.matcher.as_deref()
    }

    #[must_use]
    pub const fn once(&self) -> bool {
        self.once
    }

    #[must_use]
    pub fn command_only(command: String) -> Self {
        Self {
            command,
            matcher: None,
            once: false,
        }
    }

    #[must_use]
    pub fn new(command: impl Into<String>, matcher: Option<String>, once: bool) -> Self {
        Self {
            command: command.into(),
            matcher,
            once,
        }
    }
}

impl RuntimePluginConfig {
    #[must_use]
    pub fn enabled_plugins(&self) -> &BTreeMap<String, bool> {
        &self.enabled_plugins
    }

    #[must_use]
    pub fn external_directories(&self) -> &[String] {
        &self.external_directories
    }

    #[must_use]
    pub fn install_root(&self) -> Option<&str> {
        self.install_root.as_deref()
    }

    #[must_use]
    pub fn bundled_root(&self) -> Option<&str> {
        self.bundled_root.as_deref()
    }
}

impl RuntimeProviderConfig {
    #[must_use]
    pub fn kind(&self) -> &RuntimeProviderKind {
        &self.kind
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn api_key_env(&self) -> &str {
        &self.api_key_env
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    #[must_use]
    pub fn base_url_env(&self) -> Option<&str> {
        self.base_url_env.as_deref()
    }

    #[must_use]
    pub const fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }
}

impl RuntimeContextConfig {
    #[must_use]
    pub const fn preserve_recent_messages(&self) -> usize {
        self.preserve_recent_messages
    }

    #[must_use]
    pub const fn context_window_tokens_override(&self) -> Option<usize> {
        self.context_window_tokens
    }

    #[must_use]
    pub fn max_prompt_tokens(&self) -> usize {
        self.context_window_tokens.unwrap_or(200_000)
    }

    #[must_use]
    pub const fn max_instruction_tokens(&self) -> usize {
        self.max_instruction_tokens
    }

    #[must_use]
    pub const fn max_output_tokens(&self) -> usize {
        self.max_output_tokens
    }

    #[must_use]
    pub fn compact_reserve_tokens(&self) -> usize {
        self.max_output_tokens.min(COMPACT_MAX_OUTPUT_TOKENS)
    }

    #[must_use]
    pub const fn auto_compact_buffer_tokens(&self) -> usize {
        self.auto_compact_buffer_tokens
    }

    #[must_use]
    pub const fn warning_buffer_tokens(&self) -> usize {
        self.warning_buffer_tokens
    }

    #[must_use]
    pub const fn error_buffer_tokens(&self) -> usize {
        self.error_buffer_tokens
    }

    #[must_use]
    pub const fn manual_compact_buffer_tokens(&self) -> usize {
        self.manual_compact_buffer_tokens
    }

    #[must_use]
    pub const fn message_collapse_chars(&self) -> usize {
        self.message_collapse_chars
    }

    #[must_use]
    pub const fn max_tool_result_chars(&self) -> usize {
        self.max_tool_result_chars
    }

    #[must_use]
    pub const fn max_tool_results_per_message_chars(&self) -> usize {
        self.max_tool_results_per_message_chars
    }

    #[must_use]
    pub const fn tool_result_soft_chars(&self) -> usize {
        self.max_tool_result_chars
    }

    #[must_use]
    pub const fn tool_result_hard_chars(&self) -> usize {
        self.max_tool_results_per_message_chars
    }

    #[must_use]
    pub fn instruction_files(&self) -> &[String] {
        &self.instruction_files
    }
}

impl RuntimeTeamMemorySyncConfig {
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    #[must_use]
    pub fn token_env(&self) -> Option<&str> {
        self.token_env.as_deref()
    }

    #[must_use]
    pub fn repo(&self) -> Option<&str> {
        self.repo.as_deref()
    }

    #[must_use]
    pub const fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    #[must_use]
    pub const fn debounce_ms(&self) -> u64 {
        self.debounce_ms
    }

    #[must_use]
    pub const fn poll_ms(&self) -> u64 {
        self.poll_ms
    }

    #[must_use]
    pub fn is_configured(&self) -> bool {
        self.enabled && self.endpoint.is_some()
    }
}

impl RuntimeLspConfig {
    #[must_use]
    pub fn servers(&self) -> &BTreeMap<String, RuntimeLspServerConfig> {
        &self.servers
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        !self.servers.is_empty()
    }
}

impl RuntimeLspServerConfig {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn command(&self) -> &str {
        &self.command
    }

    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }

    #[must_use]
    pub fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    #[must_use]
    pub fn workspace_root(&self) -> Option<&str> {
        self.workspace_root.as_deref()
    }

    #[must_use]
    pub fn initialization_options(&self) -> Option<&Value> {
        self.initialization_options.as_ref()
    }

    #[must_use]
    pub fn extension_to_language(&self) -> &BTreeMap<String, String> {
        &self.extension_to_language
    }
}

#[must_use]
pub fn default_config_home() -> PathBuf {
    std::env::var_os("OPENCOWORK_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".opencowork")))
        .or_else(|| {
            std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".opencowork"))
        })
        .unwrap_or_else(|| PathBuf::from(".opencowork"))
}

fn read_optional_object(path: &Path) -> Result<Option<Value>, ConfigError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ConfigError::Io(error)),
    };
    let parsed: Value = serde_json::from_str(&contents)?;
    if !parsed.is_object() {
        return Err(ConfigError::Invalid(format!(
            "{} must contain a JSON object",
            path.display()
        )));
    }
    Ok(Some(parsed))
}

fn deep_merge(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Object(target), Value::Object(source)) => {
            for (key, value) in source {
                deep_merge(target.entry(key).or_insert(Value::Null), value);
            }
        }
        (target, source) => *target = source.clone(),
    }
}

fn parse_hooks(root: &Value) -> Result<RuntimeHookConfig, ConfigError> {
    let Some(hooks) = root.get("hooks") else {
        return Ok(RuntimeHookConfig::default());
    };
    let Some(object) = hooks.as_object() else {
        return Err(ConfigError::Invalid("hooks must be an object".to_string()));
    };
    let mut config = RuntimeHookConfig::default();
    for event in RuntimeHookEvent::all() {
        if object.contains_key(event.label()) {
            let commands = parse_hook_commands(
                object.get(event.label()),
                &format!("hooks.{}", event.label()),
            )?;
            if !commands.is_empty() {
                config.entries.insert(*event, commands);
            }
        }
    }
    Ok(config)
}

fn parse_hook_commands(
    value: Option<&Value>,
    context: &str,
) -> Result<Vec<RuntimeHookCommand>, ConfigError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(array) = value.as_array() else {
        return Err(ConfigError::Invalid(format!("{context} must be an array")));
    };
    array
        .iter()
        .enumerate()
        .map(|(index, item)| match item {
            Value::String(command) => Ok(RuntimeHookCommand::command_only(command.clone())),
            Value::Object(object) => {
                if let Some(kind) = object.get("type").and_then(Value::as_str) {
                    if kind != "command" {
                        return Err(ConfigError::Invalid(format!(
                            "{context}[{index}].type must be `command`"
                        )));
                    }
                }
                let command = object
                    .get("command")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        ConfigError::Invalid(format!("{context}[{index}].command must be a string"))
                    })?
                    .to_string();
                let matcher = object
                    .get("matcher")
                    .and_then(Value::as_str)
                    .or_else(|| object.get("if").and_then(Value::as_str))
                    .map(ToOwned::to_owned);
                let once = object.get("once").and_then(Value::as_bool).unwrap_or(false);
                Ok(RuntimeHookCommand {
                    command,
                    matcher,
                    once,
                })
            }
            _ => Err(ConfigError::Invalid(format!(
                "{context}[{index}] must be a string or object"
            ))),
        })
        .collect()
}

fn parse_plugins(root: &Value) -> Result<RuntimePluginConfig, ConfigError> {
    let mut config = RuntimePluginConfig::default();
    if let Some(enabled) = root.get("enabledPlugins") {
        config.enabled_plugins = parse_bool_map(enabled, "enabledPlugins")?;
    }
    let Some(plugins) = root.get("plugins") else {
        return Ok(config);
    };
    let Some(object) = plugins.as_object() else {
        return Err(ConfigError::Invalid(
            "plugins must be an object".to_string(),
        ));
    };
    if let Some(enabled) = object.get("enabled") {
        config.enabled_plugins = parse_bool_map(enabled, "plugins.enabled")?;
    }
    config.external_directories = parse_string_array(
        object.get("externalDirectories"),
        "plugins.externalDirectories",
    )?;
    config.install_root = object
        .get("installRoot")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    config.bundled_root = object
        .get("bundledRoot")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok(config)
}

fn parse_permission_mode(
    root: &Value,
) -> Result<Option<crate::permissions::PermissionMode>, ConfigError> {
    let raw = root
        .get("permissionMode")
        .and_then(Value::as_str)
        .or_else(|| {
            root.get("permissions")
                .and_then(Value::as_object)
                .and_then(|object| object.get("defaultMode"))
                .and_then(Value::as_str)
        });

    raw.map(parse_permission_label).transpose()
}

fn parse_model(root: &Value) -> Option<String> {
    root.get("model")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn parse_provider(root: &Value) -> Result<Option<RuntimeProviderConfig>, ConfigError> {
    let Some(value) = root.get("provider") else {
        return Ok(None);
    };

    if let Some(label) = value.as_str() {
        return provider_from_kind(label).map(Some);
    }

    let Some(object) = value.as_object() else {
        return Err(ConfigError::Invalid(
            "provider must be a string or object".to_string(),
        ));
    };
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("openai-compatible");
    let mut provider = provider_from_kind(kind)?;
    if let Some(name) = object.get("name").and_then(Value::as_str) {
        provider.name = name.to_string();
    }
    if let Some(api_key_env) = object.get("apiKeyEnv").and_then(Value::as_str) {
        provider.api_key_env = api_key_env.to_string();
    }
    if let Some(base_url) = object.get("baseUrl").and_then(Value::as_str) {
        provider.base_url = base_url.to_string();
    }
    if let Some(base_url_env) = object.get("baseUrlEnv").and_then(Value::as_str) {
        provider.base_url_env = Some(base_url_env.to_string());
    }
    if let Some(timeout_ms) = object.get("timeoutMs").and_then(Value::as_u64) {
        provider.timeout_ms = timeout_ms;
    }
    Ok(Some(provider))
}

fn parse_mcp_servers(root: &Value) -> Result<BTreeMap<String, McpServerDefinition>, ConfigError> {
    let Some(servers) = root.get("mcpServers") else {
        return Ok(BTreeMap::new());
    };
    let Some(object) = servers.as_object() else {
        return Err(ConfigError::Invalid(
            "mcpServers must be an object".to_string(),
        ));
    };

    object
        .iter()
        .map(|(name, value)| {
            let Some(server) = value.as_object() else {
                return Err(ConfigError::Invalid(format!(
                    "mcpServers.{name} must be an object"
                )));
            };
            let transport = match server.get("type").and_then(Value::as_str) {
                None | Some("stdio") => McpTransport::Stdio,
                Some("http") => McpTransport::Http,
                Some("sse") => McpTransport::Sse,
                Some("ws") => McpTransport::Ws,
                Some(other) => {
                    return Err(ConfigError::Invalid(format!(
                        "unsupported mcp transport `{other}` for {name}"
                    )))
                }
            };

            Ok((
                name.clone(),
                McpServerDefinition {
                    name: name.clone(),
                    transport,
                    command: server
                        .get("command")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    args: parse_string_array(
                        server.get("args"),
                        &format!("mcpServers.{name}.args"),
                    )?,
                    endpoint: server
                        .get("url")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    env: server
                        .get("env")
                        .map(|value| parse_string_map(value, &format!("mcpServers.{name}.env")))
                        .transpose()?
                        .unwrap_or_default(),
                    headers: server
                        .get("headers")
                        .map(|value| parse_string_map(value, &format!("mcpServers.{name}.headers")))
                        .transpose()?
                        .unwrap_or_default(),
                    auth: parse_mcp_auth(server, name)?,
                    timeout_ms: server.get("timeoutMs").and_then(Value::as_u64),
                    tools: parse_mcp_tool_definitions(name, server.get("tools"))?,
                },
            ))
        })
        .collect()
}

fn parse_context(root: &Value) -> Result<RuntimeContextConfig, ConfigError> {
    let Some(value) = root.get("context") else {
        return Ok(RuntimeContextConfig::default());
    };
    let Some(object) = value.as_object() else {
        return Err(ConfigError::Invalid(
            "context must be an object".to_string(),
        ));
    };
    let mut config = RuntimeContextConfig::default();
    if let Some(value) = object.get("preserveRecentMessages").and_then(Value::as_u64) {
        config.preserve_recent_messages = usize::try_from(value).map_err(|_| {
            ConfigError::Invalid("context.preserveRecentMessages is too large".to_string())
        })?;
    }
    if let Some(value) = object
        .get("contextWindowTokens")
        .or_else(|| object.get("maxPromptTokens"))
        .and_then(Value::as_u64)
    {
        config.context_window_tokens = Some(usize::try_from(value).map_err(|_| {
            ConfigError::Invalid(
                "context.contextWindowTokens/context.maxPromptTokens is too large".to_string(),
            )
        })?);
    }
    if let Some(value) = object.get("maxInstructionTokens").and_then(Value::as_u64) {
        config.max_instruction_tokens = usize::try_from(value).map_err(|_| {
            ConfigError::Invalid("context.maxInstructionTokens is too large".to_string())
        })?;
    }
    if let Some(value) = object
        .get("maxOutputTokens")
        .or_else(|| object.get("compactReserveTokens"))
        .and_then(Value::as_u64)
    {
        config.max_output_tokens = usize::try_from(value).map_err(|_| {
            ConfigError::Invalid(
                "context.maxOutputTokens/context.compactReserveTokens is too large".to_string(),
            )
        })?;
    }
    if let Some(value) = object
        .get("autoCompactBufferTokens")
        .and_then(Value::as_u64)
    {
        config.auto_compact_buffer_tokens = usize::try_from(value).map_err(|_| {
            ConfigError::Invalid("context.autoCompactBufferTokens is too large".to_string())
        })?;
    }
    if let Some(value) = object.get("warningBufferTokens").and_then(Value::as_u64) {
        config.warning_buffer_tokens = usize::try_from(value).map_err(|_| {
            ConfigError::Invalid("context.warningBufferTokens is too large".to_string())
        })?;
    }
    if let Some(value) = object.get("errorBufferTokens").and_then(Value::as_u64) {
        config.error_buffer_tokens = usize::try_from(value).map_err(|_| {
            ConfigError::Invalid("context.errorBufferTokens is too large".to_string())
        })?;
    }
    if let Some(value) = object
        .get("manualCompactBufferTokens")
        .and_then(Value::as_u64)
    {
        config.manual_compact_buffer_tokens = usize::try_from(value).map_err(|_| {
            ConfigError::Invalid("context.manualCompactBufferTokens is too large".to_string())
        })?;
    }
    if let Some(value) = object.get("messageCollapseChars").and_then(Value::as_u64) {
        config.message_collapse_chars = usize::try_from(value).map_err(|_| {
            ConfigError::Invalid("context.messageCollapseChars is too large".to_string())
        })?;
    }
    if let Some(value) = object
        .get("maxToolResultChars")
        .or_else(|| object.get("toolResultSoftChars"))
        .and_then(Value::as_u64)
    {
        config.max_tool_result_chars = usize::try_from(value).map_err(|_| {
            ConfigError::Invalid(
                "context.maxToolResultChars/context.toolResultSoftChars is too large".to_string(),
            )
        })?;
    }
    if let Some(value) = object
        .get("maxToolResultsPerMessageChars")
        .or_else(|| object.get("toolResultHardChars"))
        .and_then(Value::as_u64)
    {
        config.max_tool_results_per_message_chars = usize::try_from(value).map_err(|_| {
            ConfigError::Invalid(
                "context.maxToolResultsPerMessageChars/context.toolResultHardChars is too large"
                    .to_string(),
            )
        })?;
    }
    if object.contains_key("instructionFiles") {
        config.instruction_files =
            parse_string_array(object.get("instructionFiles"), "context.instructionFiles")?;
    }
    if config.max_tool_result_chars >= config.max_tool_results_per_message_chars {
        return Err(ConfigError::Invalid(
            "context.maxToolResultChars must be smaller than context.maxToolResultsPerMessageChars"
                .to_string(),
        ));
    }
    Ok(config)
}

fn parse_team_memory_sync(root: &Value) -> Result<RuntimeTeamMemorySyncConfig, ConfigError> {
    let mut config = RuntimeTeamMemorySyncConfig::default();
    let Some(value) = root.get("teamMemorySync") else {
        return Ok(config);
    };
    let Some(object) = value.as_object() else {
        return Err(ConfigError::Invalid(
            "teamMemorySync must be an object".to_string(),
        ));
    };

    if let Some(enabled) = object.get("enabled").and_then(Value::as_bool) {
        config.enabled = enabled;
    }
    if let Some(endpoint) = object.get("endpoint").and_then(Value::as_str) {
        config.endpoint = Some(endpoint.to_string());
    }
    if let Some(token_env) = object.get("tokenEnv").and_then(Value::as_str) {
        config.token_env = Some(token_env.to_string());
    }
    if let Some(repo) = object.get("repo").and_then(Value::as_str) {
        config.repo = Some(repo.to_string());
    }
    if let Some(timeout_ms) = object.get("timeoutMs").and_then(Value::as_u64) {
        config.timeout_ms = timeout_ms;
    }
    if let Some(debounce_ms) = object.get("debounceMs").and_then(Value::as_u64) {
        config.debounce_ms = debounce_ms;
    }
    if let Some(poll_ms) = object.get("pollMs").and_then(Value::as_u64) {
        config.poll_ms = poll_ms;
    }

    Ok(config)
}

fn parse_lsp(root: &Value) -> Result<RuntimeLspConfig, ConfigError> {
    let Some(value) = root.get("lspServers") else {
        return Ok(RuntimeLspConfig::default());
    };
    let Some(object) = value.as_object() else {
        return Err(ConfigError::Invalid(
            "lspServers must be an object".to_string(),
        ));
    };

    let mut servers = BTreeMap::new();
    for (name, value) in object {
        let Some(server) = value.as_object() else {
            return Err(ConfigError::Invalid(format!(
                "lspServers.{name} must be an object"
            )));
        };
        let command = server
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ConfigError::Invalid(format!("lspServers.{name}.command must be a string"))
            })?
            .to_string();
        let extension_to_language = server
            .get("languages")
            .or_else(|| server.get("extensionToLanguage"))
            .map(|value| parse_string_map(value, &format!("lspServers.{name}.languages")))
            .transpose()?
            .unwrap_or_default();
        if extension_to_language.is_empty() {
            return Err(ConfigError::Invalid(format!(
                "lspServers.{name}.languages must not be empty"
            )));
        }

        servers.insert(
            name.clone(),
            RuntimeLspServerConfig {
                name: name.clone(),
                command,
                args: parse_string_array(server.get("args"), &format!("lspServers.{name}.args"))?,
                env: server
                    .get("env")
                    .map(|value| parse_string_map(value, &format!("lspServers.{name}.env")))
                    .transpose()?
                    .unwrap_or_default(),
                workspace_root: server
                    .get("workspaceRoot")
                    .or_else(|| server.get("cwd"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                initialization_options: server.get("initializationOptions").cloned(),
                extension_to_language,
            },
        );
    }

    Ok(RuntimeLspConfig { servers })
}

fn provider_from_kind(kind: &str) -> Result<RuntimeProviderConfig, ConfigError> {
    match kind {
        "openai" | "openai-compatible" => Ok(RuntimeProviderConfig {
            kind: RuntimeProviderKind::OpenAiCompatible,
            name: "OpenAI".to_string(),
            api_key_env: "OPENAI_API_KEY".to_string(),
            base_url: DEFAULT_OPENAI_BASE_URL.to_string(),
            base_url_env: Some("OPENAI_BASE_URL".to_string()),
            timeout_ms: 90_000,
        }),
        "xai" => Ok(RuntimeProviderConfig {
            kind: RuntimeProviderKind::OpenAiCompatible,
            name: "xAI".to_string(),
            api_key_env: "XAI_API_KEY".to_string(),
            base_url: DEFAULT_XAI_BASE_URL.to_string(),
            base_url_env: Some("XAI_BASE_URL".to_string()),
            timeout_ms: 90_000,
        }),
        other => Err(ConfigError::Invalid(format!(
            "unsupported provider kind `{other}`"
        ))),
    }
}

fn parse_permission_label(value: &str) -> Result<crate::permissions::PermissionMode, ConfigError> {
    match value {
        "read-only" | "plan" | "default" => Ok(crate::permissions::PermissionMode::ReadOnly),
        "workspace-write" | "auto" | "acceptEdits" => {
            Ok(crate::permissions::PermissionMode::WorkspaceWrite)
        }
        "danger-full-access" | "dontAsk" => {
            Ok(crate::permissions::PermissionMode::DangerFullAccess)
        }
        other => Err(ConfigError::Invalid(format!(
            "unsupported permission mode `{other}`"
        ))),
    }
}

fn parse_string_array(value: Option<&Value>, context: &str) -> Result<Vec<String>, ConfigError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(array) = value.as_array() else {
        return Err(ConfigError::Invalid(format!("{context} must be an array")));
    };
    array
        .iter()
        .map(|item| {
            item.as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| ConfigError::Invalid(format!("{context} must contain only strings")))
        })
        .collect()
}

fn parse_mcp_tool_definitions(
    server_name: &str,
    value: Option<&Value>,
) -> Result<Vec<McpToolDefinition>, ConfigError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(array) = value.as_array() else {
        return Err(ConfigError::Invalid(format!(
            "mcpServers.{server_name}.tools must be an array"
        )));
    };

    array
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let Some(object) = item.as_object() else {
                return Err(ConfigError::Invalid(format!(
                    "mcpServers.{server_name}.tools[{index}] must be an object"
                )));
            };
            let tool_name = object
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ConfigError::Invalid(format!(
                        "mcpServers.{server_name}.tools[{index}].name must be a string"
                    ))
                })?
                .to_string();
            let description = object
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("MCP tool")
                .to_string();
            let input_schema = object
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({ "type": "object" }));
            let required_permission = match object
                .get("requiredPermission")
                .and_then(Value::as_str)
                .unwrap_or("read-only")
            {
                "read-only" => McpToolPermission::ReadOnly,
                "workspace-write" => McpToolPermission::WorkspaceWrite,
                "danger-full-access" => McpToolPermission::DangerFullAccess,
                other => {
                    return Err(ConfigError::Invalid(format!(
                        "unsupported mcp permission `{other}` for {server_name}.{tool_name}"
                    )))
                }
            };

            Ok(McpToolDefinition {
                server_name: server_name.to_string(),
                tool_name,
                description,
                input_schema,
                required_permission,
            })
        })
        .collect()
}

fn parse_mcp_auth(
    server: &serde_json::Map<String, Value>,
    server_name: &str,
) -> Result<McpAuthConfig, ConfigError> {
    if let Some(auth) = server.get("auth") {
        let Some(object) = auth.as_object() else {
            return Err(ConfigError::Invalid(format!(
                "mcpServers.{server_name}.auth must be an object"
            )));
        };
        let kind = object.get("type").and_then(Value::as_str).unwrap_or("none");
        return match kind {
            "none" => Ok(McpAuthConfig::None),
            "bearer-env" => {
                let token_env =
                    object
                        .get("tokenEnv")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            ConfigError::Invalid(format!(
                                "mcpServers.{server_name}.auth.tokenEnv must be a string"
                            ))
                        })?;
                Ok(McpAuthConfig::BearerEnv {
                    token_env: token_env.to_string(),
                })
            }
            "bearer-file" => {
                let token_path =
                    object
                        .get("tokenPath")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            ConfigError::Invalid(format!(
                                "mcpServers.{server_name}.auth.tokenPath must be a string"
                            ))
                        })?;
                Ok(McpAuthConfig::BearerFile {
                    token_path: token_path.to_string(),
                })
            }
            "oauth" => Ok(McpAuthConfig::OAuth {
                oauth: parse_oauth_config_object(
                    object,
                    &format!("mcpServers.{server_name}.auth"),
                )?,
            }),
            other => Err(ConfigError::Invalid(format!(
                "unsupported mcp auth type `{other}` for {server_name}"
            ))),
        };
    }

    if let Some(oauth) = server.get("oauth") {
        let Some(object) = oauth.as_object() else {
            return Err(ConfigError::Invalid(format!(
                "mcpServers.{server_name}.oauth must be an object"
            )));
        };
        return Ok(McpAuthConfig::OAuth {
            oauth: parse_oauth_config_object(object, &format!("mcpServers.{server_name}.oauth"))?,
        });
    }

    Ok(McpAuthConfig::None)
}

fn parse_oauth_config_object(
    object: &serde_json::Map<String, Value>,
    context: &str,
) -> Result<McpOAuthConfig, ConfigError> {
    Ok(McpOAuthConfig {
        client_id: object
            .get("clientId")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        callback_port: object
            .get("callbackPort")
            .and_then(Value::as_u64)
            .map(|value| {
                u16::try_from(value).map_err(|_| {
                    ConfigError::Invalid(format!("{context}.callbackPort must fit in u16"))
                })
            })
            .transpose()?,
        auth_server_metadata_url: object
            .get("authServerMetadataUrl")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        scopes: parse_string_array(object.get("scopes"), &format!("{context}.scopes"))?,
        authorize_url: object
            .get("authorizeUrl")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        token_url: object
            .get("tokenUrl")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        manual_redirect_url: object
            .get("manualRedirectUrl")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        xaa: object.get("xaa").and_then(Value::as_bool),
    })
}

fn parse_bool_map(value: &Value, context: &str) -> Result<BTreeMap<String, bool>, ConfigError> {
    let Some(object) = value.as_object() else {
        return Err(ConfigError::Invalid(format!("{context} must be an object")));
    };
    object
        .iter()
        .map(|(key, value)| {
            value
                .as_bool()
                .map(|enabled| (key.clone(), enabled))
                .ok_or_else(|| ConfigError::Invalid(format!("{context}.{key} must be a boolean")))
        })
        .collect()
}

fn parse_string_map(value: &Value, context: &str) -> Result<BTreeMap<String, String>, ConfigError> {
    let Some(object) = value.as_object() else {
        return Err(ConfigError::Invalid(format!("{context} must be an object")));
    };
    object
        .iter()
        .map(|(key, value)| {
            value
                .as_str()
                .map(|text| (key.clone(), text.to_string()))
                .ok_or_else(|| ConfigError::Invalid(format!("{context}.{key} must be a string")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{ConfigLoader, ConfigSource, RuntimeHookEvent};
    use crate::permissions::PermissionMode;
    use opencowork_mcp::{McpAuthConfig, McpTransport};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("opencowork-config-{stamp}"))
    }

    #[test]
    fn loads_and_merges_settings_in_precedence_order() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".opencowork");
        fs::create_dir_all(cwd.join(".opencowork")).expect("project config dir");
        fs::create_dir_all(&home).expect("user config dir");

        fs::write(
            home.join("settings.json"),
            r#"{"hooks":{"PreToolUse":["base"]},"enabledPlugins":{"guard":true}}"#,
        )
        .expect("write user settings");
        fs::write(
            cwd.join(".opencowork.json"),
            r#"{"plugins":{"externalDirectories":["./plugins"]}}"#,
        )
        .expect("write project compat");
        fs::write(
            cwd.join(".opencowork").join("settings.local.json"),
            r#"{"permissionMode":"workspace-write","hooks":{"PostToolUse":["local"]},"mcpServers":{"github":{"type":"http","url":"https://example.test/mcp","tools":[{"name":"search_issues","description":"Search issues","inputSchema":{"type":"object"},"requiredPermission":"read-only"}]}}}"#,
        )
        .expect("write local settings");

        let loaded = ConfigLoader::new(&cwd, &home).load().expect("load config");
        assert_eq!(loaded.loaded_entries().len(), 3);
        assert_eq!(loaded.loaded_entries()[0].source, ConfigSource::User);
        assert_eq!(
            loaded.permission_mode(),
            Some(PermissionMode::WorkspaceWrite)
        );
        assert_eq!(
            loaded.hooks().commands(RuntimeHookEvent::PreToolUse).len(),
            1
        );
        assert_eq!(
            loaded.hooks().commands(RuntimeHookEvent::PostToolUse).len(),
            1
        );
        assert_eq!(
            loaded.plugins().external_directories(),
            &["./plugins".to_string()]
        );
        assert_eq!(loaded.plugins().enabled_plugins().get("guard"), Some(&true));
        assert_eq!(loaded.context().preserve_recent_messages(), 8);
        assert_eq!(
            loaded
                .mcp_servers()
                .get("github")
                .expect("github mcp server")
                .transport,
            McpTransport::Http
        );
        assert_eq!(
            loaded
                .mcp_servers()
                .get("github")
                .expect("github mcp server")
                .tools
                .len(),
            1
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_context_provider_timeouts_and_mcp_auth() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".opencowork");
        fs::create_dir_all(cwd.join(".opencowork")).expect("project config dir");
        fs::create_dir_all(&home).expect("user config dir");

        fs::write(
            cwd.join(".opencowork.json"),
            r#"{
                "provider": {
                    "kind": "openai-compatible",
                    "name": "Local Gateway",
                    "timeoutMs": 12345
                },
                "context": {
                    "preserveRecentMessages": 4,
                    "maxPromptTokens": 2048,
                    "maxInstructionTokens": 512,
                    "compactReserveTokens": 256,
                    "messageCollapseChars": 1024,
                    "toolResultSoftChars": 4096,
                    "toolResultHardChars": 8192,
                    "instructionFiles": ["CLAUDE.md", "docs/guide.md"]
                },
                "lspServers": {
                    "rust-analyzer": {
                        "command": "rust-analyzer",
                        "args": ["--stdio"],
                        "workspaceRoot": ".",
                        "languages": {
                            ".rs": "rust"
                        }
                    }
                },
                "mcpServers": {
                    "github": {
                        "type": "http",
                        "url": "https://example.test/mcp",
                        "timeoutMs": 7777,
                        "auth": {
                            "type": "bearer-env",
                            "tokenEnv": "GITHUB_TOKEN"
                        }
                    }
                }
            }"#,
        )
        .expect("write project config");

        let loaded = ConfigLoader::new(&cwd, &home).load().expect("load config");
        assert_eq!(loaded.provider().expect("provider").timeout_ms(), 12_345);
        assert_eq!(loaded.context().preserve_recent_messages(), 4);
        assert_eq!(loaded.context().max_prompt_tokens(), 2_048);
        assert_eq!(loaded.context().max_instruction_tokens(), 512);
        assert_eq!(loaded.context().compact_reserve_tokens(), 256);
        assert_eq!(loaded.context().message_collapse_chars(), 1_024);
        assert_eq!(loaded.context().tool_result_soft_chars(), 4_096);
        assert_eq!(loaded.context().tool_result_hard_chars(), 8_192);
        assert!(loaded.lsp().is_enabled());
        assert_eq!(
            loaded
                .lsp()
                .servers()
                .get("rust-analyzer")
                .expect("lsp server")
                .command(),
            "rust-analyzer"
        );
        assert_eq!(
            loaded.context().instruction_files(),
            &["CLAUDE.md".to_string(), "docs/guide.md".to_string()]
        );
        let github = loaded.mcp_servers().get("github").expect("github");
        assert_eq!(github.timeout_ms, Some(7_777));
        assert!(matches!(
            github.auth,
            McpAuthConfig::BearerEnv { ref token_env } if token_env == "GITHUB_TOKEN"
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_richer_hook_objects() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".opencowork");
        fs::create_dir_all(cwd.join(".opencowork")).expect("project config dir");
        fs::create_dir_all(&home).expect("user config dir");

        fs::write(
            cwd.join(".opencowork.json"),
            r#"{
                "hooks": {
                    "PreToolUse": [
                        {"type":"command","command":"echo guard","matcher":"read_*","once":true}
                    ],
                    "SessionStart": ["echo start"],
                    "PostToolUseFailure": [{"command":"echo failure"}]
                }
            }"#,
        )
        .expect("write project config");

        let loaded = ConfigLoader::new(&cwd, &home).load().expect("load config");
        assert_eq!(
            loaded.hooks().commands(RuntimeHookEvent::PreToolUse).len(),
            1
        );
        assert_eq!(
            loaded
                .hooks()
                .commands(RuntimeHookEvent::SessionStart)
                .len(),
            1
        );
        assert_eq!(
            loaded
                .hooks()
                .commands(RuntimeHookEvent::PostToolUseFailure)
                .len(),
            1
        );

        let _ = fs::remove_dir_all(root);
    }
}

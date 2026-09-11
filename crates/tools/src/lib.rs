mod browser;
mod computer;
#[cfg(windows)]
mod computer_capture;
pub mod file_history;

/// Internal one-frame capture entrypoint for shell/CLI child processes.
pub fn computer_capture_worker() -> Result<(), String> {
    #[cfg(windows)]
    {
        computer_capture::worker()
    }
    #[cfg(not(windows))]
    {
        Err("Computer capture requires Windows".into())
    }
}
mod process;
mod task_plan;
mod team_memory;
mod team_memory_sync;

use glob::glob;
use lsp_types::Position;
use opencowork_lsp::{LspContextEnrichment, LspManager, LspServerConfig};
use opencowork_mcp::{mcp_tool_name, McpExecutor, McpToolDefinition, McpToolPermission};
use opencowork_plugins::PluginTool;
use opencowork_runtime::{
    get_auto_tool_search_char_threshold, PermissionMode, RuntimeLspServerConfig,
    RuntimeToolDefinition, ToolError, ToolExecutor,
};
use opencowork_skills::SkillCatalog;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use team_memory::guard_team_memory_write;
use team_memory_sync::notify_team_memory_write_if_needed;
pub use team_memory_sync::{
    force_pull_team_memory, force_push_team_memory, notify_team_memory_write,
    start_team_memory_sync, stop_team_memory_sync, team_memory_sync_status, TeamMemorySyncStatus,
};
use tokio::runtime::Runtime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExposure {
    Core,
    Deferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSearchMode {
    DeferredAlways,
    DeferredAuto,
    Standard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSearchSettings {
    pub mode: ToolSearchMode,
    pub model: String,
    pub context_window_tokens_override: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSearchDiagnostics {
    pub mode: ToolSearchMode,
    pub deferred_description_chars: usize,
    pub auto_threshold_chars: usize,
    pub deferred_tools_inline: bool,
}

impl ToolSearchSettings {
    #[must_use]
    pub fn from_env(
        model: impl Into<String>,
        context_window_tokens_override: Option<usize>,
    ) -> Self {
        Self {
            mode: tool_search_mode_from_env(),
            model: model.into(),
            context_window_tokens_override,
        }
    }
}

impl Default for ToolSearchSettings {
    fn default() -> Self {
        Self::from_env("gpt-5.4-mini", None)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub input_schema: Value,
    pub required_permission: PermissionMode,
    pub exposure: ToolExposure,
}

pub struct GlobalToolRegistry {
    plugin_tools: Vec<PluginTool>,
    mcp_tools: Vec<McpToolDefinition>,
    mcp_executor: Option<Box<dyn McpExecutor>>,
    skill_catalog: SkillCatalog,
    lsp_servers: Vec<LspServerConfig>,
    lsp_service: Option<LazyLspService>,
    active_deferred_tools: BTreeSet<String>,
    tool_search: ToolSearchSettings,
    browser: Option<browser::BrowserSession>,
    computer: Option<computer::ComputerHelper>,
}

struct LazyLspService {
    runtime: Runtime,
    manager: LspManager,
}

impl GlobalToolRegistry {
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            plugin_tools: Vec::new(),
            mcp_tools: Vec::new(),
            mcp_executor: None,
            skill_catalog: SkillCatalog::default(),
            lsp_servers: Vec::new(),
            lsp_service: None,
            active_deferred_tools: BTreeSet::new(),
            tool_search: ToolSearchSettings::default(),
            browser: None,
            computer: None,
        }
    }

    pub fn with_plugin_tools(
        plugin_tools: Vec<PluginTool>,
        skill_catalog: SkillCatalog,
    ) -> Result<Self, String> {
        Self::with_extensions(plugin_tools, Vec::new(), None, skill_catalog, Vec::new())
    }

    pub fn with_extensions(
        plugin_tools: Vec<PluginTool>,
        mcp_tools: Vec<McpToolDefinition>,
        mcp_executor: Option<Box<dyn McpExecutor>>,
        skill_catalog: SkillCatalog,
        lsp_servers: Vec<LspServerConfig>,
    ) -> Result<Self, String> {
        let builtin = builtin_specs()
            .iter()
            .map(|spec| spec.name.to_string())
            .collect::<BTreeSet<_>>();
        let mut seen = BTreeSet::new();

        for tool in &plugin_tools {
            if builtin.contains(tool.name()) {
                return Err(format!(
                    "plugin tool `{}` conflicts with a builtin tool",
                    tool.name()
                ));
            }
            if !seen.insert(tool.name().to_string()) {
                return Err(format!("duplicate plugin tool `{}`", tool.name()));
            }
        }

        for tool in &mcp_tools {
            let canonical = mcp_tool_name(&tool.server_name, &tool.tool_name);
            if builtin.contains(&canonical) {
                return Err(format!(
                    "MCP tool `{canonical}` conflicts with a builtin tool"
                ));
            }
            if !seen.insert(canonical.clone()) {
                return Err(format!("duplicate tool `{canonical}`"));
            }
        }

        Ok(Self {
            plugin_tools,
            mcp_tools,
            mcp_executor,
            skill_catalog,
            lsp_servers,
            lsp_service: None,
            active_deferred_tools: BTreeSet::new(),
            tool_search: ToolSearchSettings::default(),
            browser: None,
            computer: None,
        })
    }

    #[must_use]
    pub fn with_tool_search_settings(mut self, settings: ToolSearchSettings) -> Self {
        self.tool_search = settings;
        self
    }

    #[must_use]
    pub fn definitions(&self) -> Vec<(String, String, PermissionMode, Value)> {
        let deferred_tools_inline = self.deferred_tools_inline();
        let builtin = builtin_specs()
            .into_iter()
            .filter(|spec| {
                (spec.name != "Computer" || computer::enabled())
                    && (spec.name != "Browser" || browser::enabled())
                    && (spec.exposure == ToolExposure::Core
                        || deferred_tools_inline
                        || self.active_deferred_tools.contains(spec.name))
            })
            .map(|spec| {
                (
                    spec.name.to_string(),
                    spec.description.to_string(),
                    spec.required_permission,
                    spec.input_schema,
                )
            })
            .collect::<Vec<_>>();
        let plugin = self
            .plugin_tools
            .iter()
            .map(|tool| {
                (
                    tool.name().to_string(),
                    tool.description().to_string(),
                    tool.required_permission(),
                    tool.input_schema().clone(),
                )
            })
            .collect::<Vec<_>>();
        let mcp = self
            .mcp_tools
            .iter()
            .map(|tool| {
                (
                    mcp_tool_name(&tool.server_name, &tool.tool_name),
                    tool.description.clone(),
                    permission_mode_from_mcp(tool.required_permission),
                    tool.input_schema.clone(),
                )
            })
            .collect::<Vec<_>>();
        builtin.into_iter().chain(plugin).chain(mcp).collect()
    }

    #[must_use]
    pub fn tool_search_diagnostics(&self) -> ToolSearchDiagnostics {
        let deferred_description_chars = deferred_builtin_description_chars();
        let auto_threshold_chars = get_auto_tool_search_char_threshold(
            &self.tool_search.model,
            self.tool_search.context_window_tokens_override,
        );
        ToolSearchDiagnostics {
            mode: self.tool_search.mode,
            deferred_description_chars,
            auto_threshold_chars,
            deferred_tools_inline: self.deferred_tools_inline_with_threshold(
                deferred_description_chars,
                auto_threshold_chars,
            ),
        }
    }

    #[must_use]
    pub fn runtime_definitions(&self) -> Vec<RuntimeToolDefinition> {
        self.definitions()
            .into_iter()
            .map(
                |(name, description, required_permission, input_schema)| RuntimeToolDefinition {
                    name,
                    description,
                    input_schema,
                    required_permission,
                },
            )
            .collect()
    }

    fn deferred_tools_inline(&self) -> bool {
        let diagnostics = self.tool_search_diagnostics();
        diagnostics.deferred_tools_inline
    }

    fn deferred_tools_inline_with_threshold(
        &self,
        deferred_description_chars: usize,
        auto_threshold_chars: usize,
    ) -> bool {
        match self.tool_search.mode {
            ToolSearchMode::Standard => true,
            ToolSearchMode::DeferredAlways => false,
            ToolSearchMode::DeferredAuto => deferred_description_chars <= auto_threshold_chars,
        }
    }

    fn deferred_tool_available(&self, name: &str) -> bool {
        self.deferred_tools_inline() || self.active_deferred_tools.contains(name)
    }

    pub fn execute(&mut self, name: &str, input: &Value) -> Result<String, ToolError> {
        if builtin_specs().iter().any(|spec| spec.name == name) {
            return execute_builtin(self, name, input);
        }
        if let Some(tool) = self.plugin_tools.iter().find(|tool| tool.name() == name) {
            return tool
                .execute(input)
                .map_err(|error| ToolError::new(error.to_string()));
        }
        if let Some(tool) = self
            .mcp_tools
            .iter()
            .find(|tool| mcp_tool_name(&tool.server_name, &tool.tool_name) == name)
        {
            let executor = self.mcp_executor.as_ref().ok_or_else(|| {
                ToolError::new(format!("MCP executor not configured for `{name}`"))
            })?;
            return executor
                .execute(
                    &opencowork_mcp::McpToolBinding {
                        server_name: tool.server_name.clone(),
                        tool_name: tool.tool_name.clone(),
                    },
                    input,
                )
                .map_err(ToolError::new);
        }
        Err(ToolError::new(format!("unknown tool `{name}`")))
    }

    fn activate_deferred_tools(&mut self, requested: &[String]) -> Result<Vec<String>, ToolError> {
        let aliases = deferred_tool_aliases();
        let mut activated = Vec::new();
        for item in requested {
            let normalized = normalize_tool_name(item);
            let canonical = aliases.get(&normalized).ok_or_else(|| {
                ToolError::new(format!("unsupported deferred tool selection `{item}`"))
            })?;
            if self.active_deferred_tools.insert(canonical.clone()) {
                activated.push(canonical.clone());
            }
        }
        Ok(activated)
    }

    fn tool_search(&mut self, input: ToolSearchInput) -> Result<String, ToolError> {
        let query = input.query.trim().to_string();
        if query.is_empty() {
            return Err(ToolError::new("query must not be empty"));
        }

        let normalized_query = normalize_search_query(&query);
        let mut activated = Vec::new();
        if let Some(selection) = query.strip_prefix("select:") {
            let requested = selection
                .split(|ch: char| ch == ',' || ch.is_whitespace())
                .filter(|value| !value.trim().is_empty())
                .map(|value| value.trim().to_string())
                .collect::<Vec<_>>();
            activated = self.activate_deferred_tools(&requested)?;
        }

        let max_results = input.max_results.unwrap_or(5).max(1);
        let matches = search_deferred_tools(&query, max_results);
        let diagnostics = self.tool_search_diagnostics();
        Ok(json!(ToolSearchOutput {
            query,
            normalized_query,
            total_deferred_tools: deferred_builtin_specs().len(),
            activated,
            mode: tool_search_mode_label(diagnostics.mode).to_string(),
            deferred_tools_inline: diagnostics.deferred_tools_inline,
            deferred_description_chars: diagnostics.deferred_description_chars,
            auto_threshold_chars: diagnostics.auto_threshold_chars,
            matches,
        })
        .to_string())
    }

    fn load_skill(&self, input: SkillToolInput) -> Result<String, ToolError> {
        if !self.deferred_tool_available("Skill") {
            return Err(ToolError::new(
                "tool `Skill` is deferred; activate it first with ToolSearch select:Skill",
            ));
        }
        let definition = self
            .skill_catalog
            .load(&input.skill)
            .map_err(ToolError::new)?;
        Ok(json!({
            "name": definition.summary.name,
            "description": definition.summary.description,
            "path": definition.summary.path.display().to_string(),
            "source_root": definition.summary.source_root.display().to_string(),
            "origin": match definition.summary.origin {
                opencowork_skills::SkillOrigin::SkillsDir => "skills",
                opencowork_skills::SkillOrigin::LegacyCommandsDir => "commands",
            },
            "when_to_use": definition.summary.when_to_use,
            "argument_hint": definition.summary.argument_hint,
            "allowed_tools": definition.summary.allowed_tools,
            "paths": definition.summary.paths,
            "execution_context": definition.summary.execution_context.map(|value| match value {
                opencowork_skills::SkillExecutionContext::Current => "current",
                opencowork_skills::SkillExecutionContext::Fork => "fork",
            }),
            "version": definition.summary.version,
            "agent": definition.summary.agent,
            "model": definition.summary.model,
            "effort": definition.summary.effort,
            "args": input.args,
            "instruction": definition.content,
        })
        .to_string())
    }

    fn load_lsp_context(&mut self, input: LspContextToolInput) -> Result<String, ToolError> {
        if !self.deferred_tool_available("LspContext") {
            return Err(ToolError::new(
                "tool `LspContext` is deferred; activate it first with ToolSearch select:LspContext",
            ));
        }

        let path = PathBuf::from(&input.path);
        let position = Position::new(
            input.line.unwrap_or(1).saturating_sub(1),
            input.character.unwrap_or(1).saturating_sub(1),
        );
        let enrichment = self
            .ensure_lsp_service()?
            .context_for_path(&path, position)
            .map_err(ToolError::new)?;

        Ok(json!({
            "path": path.display().to_string(),
            "line": position.line + 1,
            "character": position.character + 1,
            "diagnostic_count": enrichment.diagnostics.total_diagnostics(),
            "definition_count": enrichment.definitions.len(),
            "reference_count": enrichment.references.len(),
            "prompt_section": enrichment.render_prompt_section(),
        })
        .to_string())
    }

    fn ensure_lsp_service(&mut self) -> Result<&mut LazyLspService, ToolError> {
        if self.lsp_service.is_none() {
            if self.lsp_servers.is_empty() {
                return Err(ToolError::new(
                    "no LSP servers configured; add lspServers to settings before using LspContext",
                ));
            }
            self.lsp_service = Some(
                LazyLspService::new(self.lsp_servers.clone())
                    .map_err(|error| ToolError::new(error.to_string()))?,
            );
        }

        self.lsp_service
            .as_mut()
            .ok_or_else(|| ToolError::new("failed to initialize LSP service"))
    }
}

impl LazyLspService {
    fn new(servers: Vec<LspServerConfig>) -> Result<Self, String> {
        let runtime = Runtime::new().map_err(|error| error.to_string())?;
        let manager = LspManager::new(servers).map_err(|error| error.to_string())?;
        Ok(Self { runtime, manager })
    }

    fn context_for_path(
        &mut self,
        path: &Path,
        position: Position,
    ) -> Result<LspContextEnrichment, String> {
        self.runtime
            .block_on(async {
                self.manager.sync_document_from_disk(path).await?;
                self.manager.context_enrichment(path, position).await
            })
            .map_err(|error| error.to_string())
    }
}

impl ToolExecutor for GlobalToolRegistry {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        let payload = if input.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(input)
                .map_err(|error| ToolError::new(format!("tool input is not valid JSON: {error}")))?
        };
        GlobalToolRegistry::execute(self, tool_name, &payload)
    }

    fn definitions(&self) -> Vec<RuntimeToolDefinition> {
        self.runtime_definitions()
    }
}

#[must_use]
pub fn lsp_servers_from_runtime(
    cwd: &Path,
    configs: impl IntoIterator<Item = RuntimeLspServerConfig>,
) -> Vec<LspServerConfig> {
    configs
        .into_iter()
        .map(|server| LspServerConfig {
            name: server.name().to_string(),
            command: server.command().to_string(),
            args: server.args().to_vec(),
            env: server.env().clone(),
            workspace_root: server
                .workspace_root()
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .unwrap_or_else(|| {
                    cwd.join(
                        server
                            .workspace_root()
                            .map(PathBuf::from)
                            .unwrap_or_else(|| PathBuf::from(".")),
                    )
                }),
            initialization_options: server.initialization_options().cloned(),
            extension_to_language: server.extension_to_language().clone(),
        })
        .collect()
}

#[must_use]
pub fn builtin_specs() -> Vec<ToolSpec> {
    vec![
        computer::spec(),
        browser::spec(),
        task_plan::spec(),
        ToolSpec {
            name: "bash",
            aliases: &["BashTool"],
            description: "Run a shell command in the current workspace.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "timeoutMs": { "type": "integer", "minimum": 100, "maximum": 600000, "default": 120000 }
                },
                "required": ["command"]
            }),
            required_permission: PermissionMode::DangerFullAccess,
            exposure: ToolExposure::Core,
        },
        ToolSpec {
            name: "read_file",
            aliases: &["FileReadTool", "read"],
            description: "Read a text file from disk.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
            required_permission: PermissionMode::ReadOnly,
            exposure: ToolExposure::Core,
        },
        ToolSpec {
            name: "write_file",
            aliases: &["FileWriteTool", "write"],
            description: "Write a text file to disk.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" },
                    "expectedVersion": { "type": "string", "description":"Version from read_file, or missing for a new file" }
                },
                "required": ["path", "content"]
            }),
            required_permission: PermissionMode::WorkspaceWrite,
            exposure: ToolExposure::Core,
        },
        ToolSpec {
            name: "edit_file",
            aliases: &["FileEditTool", "edit"],
            description: "Replace one string in a text file.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old_string": { "type": "string" },
                    "new_string": { "type": "string" },
                    "expectedVersion": { "type": "string", "description":"Version returned by read_file" }
                },
                "required": ["path", "old_string", "new_string"]
            }),
            required_permission: PermissionMode::WorkspaceWrite,
            exposure: ToolExposure::Core,
        },
        ToolSpec {
            name: "glob_search",
            aliases: &["GlobTool", "glob"],
            description: "Search for files by glob pattern.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" }
                },
                "required": ["pattern"]
            }),
            required_permission: PermissionMode::ReadOnly,
            exposure: ToolExposure::Core,
        },
        ToolSpec {
            name: "grep_search",
            aliases: &["GrepTool", "grep"],
            description: "Search for a string or regex in files.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" }
                },
                "required": ["pattern"]
            }),
            required_permission: PermissionMode::ReadOnly,
            exposure: ToolExposure::Core,
        },
        ToolSpec {
            name: "ToolSearch",
            aliases: &["ToolSearchTool"],
            description: "Search deferred or specialized tools and optionally activate them with select:ToolName.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "max_results": { "type": "integer", "minimum": 1 }
                },
                "required": ["query"]
            }),
            required_permission: PermissionMode::ReadOnly,
            exposure: ToolExposure::Core,
        },
        ToolSpec {
            name: "Skill",
            aliases: &["SkillTool"],
            description: "Load a local skill definition and its instructions.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "skill": { "type": "string" },
                    "args": { "type": "string" }
                },
                "required": ["skill"]
            }),
            required_permission: PermissionMode::ReadOnly,
            exposure: ToolExposure::Deferred,
        },
        ToolSpec {
            name: "LspContext",
            aliases: &["LSPTool", "lsp_context"],
            description: "Load semantic diagnostics, definitions, and references for a file through a lazily started LSP server.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "line": { "type": "integer", "minimum": 1 },
                    "character": { "type": "integer", "minimum": 1 }
                },
                "required": ["path"]
            }),
            required_permission: PermissionMode::ReadOnly,
            exposure: ToolExposure::Deferred,
        },
    ]
}

#[must_use]
pub fn deferred_builtin_specs() -> Vec<ToolSpec> {
    builtin_specs()
        .into_iter()
        .filter(|spec| spec.exposure == ToolExposure::Deferred)
        .collect()
}

fn deferred_tool_aliases() -> BTreeMap<String, String> {
    deferred_builtin_specs()
        .into_iter()
        .flat_map(|spec| {
            std::iter::once(spec.name)
                .chain(spec.aliases.iter().copied())
                .map(move |value| (normalize_tool_name(value), spec.name.to_string()))
        })
        .collect()
}

fn deferred_builtin_description_chars() -> usize {
    deferred_builtin_specs()
        .into_iter()
        .map(|spec| {
            spec.name.len()
                + spec.description.len()
                + serde_json::to_string(&spec.input_schema).map_or(0, |schema| schema.len())
        })
        .sum()
}

fn search_deferred_tools(query: &str, max_results: usize) -> Vec<ToolSearchMatch> {
    let normalized_query = normalize_search_query(query);
    let mut matches = deferred_builtin_specs()
        .into_iter()
        .filter_map(|spec| {
            let haystacks = std::iter::once(spec.name)
                .chain(spec.aliases.iter().copied())
                .chain(std::iter::once(spec.description))
                .collect::<Vec<_>>();
            let score = haystacks
                .iter()
                .map(|value| match_score(&normalized_query, value))
                .max()
                .unwrap_or(0);
            (score > 0).then(|| {
                (
                    score,
                    ToolSearchMatch {
                        name: spec.name.to_string(),
                        description: spec.description.to_string(),
                        required_permission: format!("{:?}", spec.required_permission),
                        aliases: spec
                            .aliases
                            .iter()
                            .map(|value| (*value).to_string())
                            .collect(),
                    },
                )
            })
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.name.cmp(&right.1.name))
    });
    matches
        .into_iter()
        .take(max_results)
        .map(|(_, item)| item)
        .collect()
}

fn match_score(query: &str, candidate: &str) -> usize {
    let normalized_candidate = normalize_search_query(candidate);
    if normalized_candidate == query {
        return 100;
    }
    if normalized_candidate.contains(query) {
        return 50;
    }
    query
        .split_whitespace()
        .filter(|token| normalized_candidate.contains(token))
        .count()
}

fn normalize_search_query(query: &str) -> String {
    query
        .trim()
        .replace([':', ',', '-', '_'], " ")
        .to_ascii_lowercase()
}

fn normalize_tool_name(value: &str) -> String {
    value.trim().replace(['-', ' '], "_").to_ascii_lowercase()
}

fn tool_search_mode_from_env() -> ToolSearchMode {
    let Some(value) = std::env::var("ENABLE_TOOL_SEARCH").ok() else {
        return ToolSearchMode::DeferredAlways;
    };
    let normalized = value.trim().to_ascii_lowercase();
    if normalized == "auto" {
        return ToolSearchMode::DeferredAuto;
    }
    if let Some(percent) = normalized.strip_prefix("auto:") {
        return match percent
            .parse::<usize>()
            .ok()
            .map(|value| value.clamp(0, 100))
        {
            Some(0) => ToolSearchMode::DeferredAlways,
            Some(100) => ToolSearchMode::Standard,
            Some(_) => ToolSearchMode::DeferredAuto,
            None => ToolSearchMode::DeferredAlways,
        };
    }
    match normalized.as_str() {
        "false" | "0" | "no" | "off" => ToolSearchMode::Standard,
        "true" | "1" | "yes" | "on" => ToolSearchMode::DeferredAlways,
        _ => ToolSearchMode::DeferredAlways,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ToolSearchInput {
    query: String,
    #[serde(default)]
    max_results: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SkillToolInput {
    skill: String,
    #[serde(default)]
    args: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LspContextToolInput {
    path: String,
    #[serde(default)]
    line: Option<u32>,
    #[serde(default)]
    character: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ToolSearchMatch {
    name: String,
    description: String,
    required_permission: String,
    aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ToolSearchOutput {
    query: String,
    normalized_query: String,
    total_deferred_tools: usize,
    activated: Vec<String>,
    mode: String,
    deferred_tools_inline: bool,
    deferred_description_chars: usize,
    auto_threshold_chars: usize,
    matches: Vec<ToolSearchMatch>,
}

fn tool_search_mode_label(mode: ToolSearchMode) -> &'static str {
    match mode {
        ToolSearchMode::DeferredAlways => "tst",
        ToolSearchMode::DeferredAuto => "tst-auto",
        ToolSearchMode::Standard => "standard",
    }
}

fn permission_mode_from_mcp(permission: McpToolPermission) -> PermissionMode {
    match permission {
        McpToolPermission::ReadOnly => PermissionMode::ReadOnly,
        McpToolPermission::WorkspaceWrite => PermissionMode::WorkspaceWrite,
        McpToolPermission::DangerFullAccess => PermissionMode::DangerFullAccess,
    }
}

fn execute_builtin(
    registry: &mut GlobalToolRegistry,
    name: &str,
    input: &Value,
) -> Result<String, ToolError> {
    match name {
        "Computer" => computer::execute(&mut registry.computer, input),
        "Browser" => browser::execute(&mut registry.browser, input),
        "UpdatePlan" => task_plan::execute(input),
        "bash" => run_bash(input),
        "read_file" => read_file(input),
        "write_file" => write_file(input),
        "edit_file" => edit_file(input),
        "glob_search" => glob_search(input),
        "grep_search" => grep_search(input),
        "ToolSearch" => registry.tool_search(
            serde_json::from_value(input.clone())
                .map_err(|error| ToolError::new(format!("invalid ToolSearch input: {error}")))?,
        ),
        "Skill" => registry.load_skill(
            serde_json::from_value(input.clone())
                .map_err(|error| ToolError::new(format!("invalid Skill input: {error}")))?,
        ),
        "LspContext" => registry.load_lsp_context(
            serde_json::from_value(input.clone())
                .map_err(|error| ToolError::new(format!("invalid LspContext input: {error}")))?,
        ),
        other => Err(ToolError::new(format!(
            "unsupported builtin tool `{other}`"
        ))),
    }
}

fn run_bash(input: &Value) -> Result<String, ToolError> {
    let command = required_string(input, "command")?;
    let timeout_ms = input
        .get("timeoutMs")
        .and_then(Value::as_u64)
        .unwrap_or(120_000);
    if !(100..=600_000).contains(&timeout_ms) {
        return Err(ToolError::new("timeoutMs must be between 100 and 600000"));
    }
    process::run_shell(command, std::time::Duration::from_millis(timeout_ms))
        .map(|output| output.to_string())
        .map_err(|error| ToolError::new(error.to_string()))
}

fn read_file(input: &Value) -> Result<String, ToolError> {
    let path = PathBuf::from(required_string(input, "path")?);
    let content = fs::read_to_string(&path).map_err(|error| ToolError::new(error.to_string()))?;
    Ok(json!({
        "path": path.display().to_string(),
        "content": content,
        "version": file_history::version(content.as_bytes())
    })
    .to_string())
}

fn write_file(input: &Value) -> Result<String, ToolError> {
    let path = PathBuf::from(required_string(input, "path")?);
    let content = required_string(input, "content")?;
    guard_team_memory_write(&path.display().to_string(), content).map_err(ToolError::new)?;
    let output = file_history::write(&path, content, input["expectedVersion"].as_str())
        .map_err(ToolError::new)?;
    notify_team_memory_write_if_needed(&path.display().to_string()).map_err(ToolError::new)?;
    Ok(output.to_string())
}

fn edit_file(input: &Value) -> Result<String, ToolError> {
    let path = PathBuf::from(required_string(input, "path")?);
    let old_string = required_string(input, "old_string")?;
    let new_string = required_string(input, "new_string")?;
    let content = fs::read_to_string(&path).map_err(|error| ToolError::new(error.to_string()))?;
    if old_string.is_empty() || content.matches(old_string).count() != 1 {
        return Err(ToolError::new(
            "old_string must match exactly once; include more context",
        ));
    }
    let updated = content.replacen(old_string, new_string, 1);
    guard_team_memory_write(&path.display().to_string(), &updated).map_err(ToolError::new)?;
    let read_version = file_history::version(content.as_bytes());
    let result = file_history::write(
        &path,
        &updated,
        Some(input["expectedVersion"].as_str().unwrap_or(&read_version)),
    )
    .map_err(ToolError::new)?;
    notify_team_memory_write_if_needed(&path.display().to_string()).map_err(ToolError::new)?;
    Ok(json!({
        "path": path.display().to_string(),
        "updated": true,
        "version": result["version"],
        "snapshotId": result["snapshotId"]
    })
    .to_string())
}

fn glob_search(input: &Value) -> Result<String, ToolError> {
    let pattern = required_string(input, "pattern")?;
    let matches = glob(pattern)
        .map_err(|error| ToolError::new(error.to_string()))?
        .filter_map(Result::ok)
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    Ok(json!({
        "pattern": pattern,
        "matches": matches
    })
    .to_string())
}

fn grep_search(input: &Value) -> Result<String, ToolError> {
    let pattern = required_string(input, "pattern")?;
    let regex = regex::Regex::new(pattern).map_err(|error| ToolError::new(error.to_string()))?;
    let root = input
        .get("path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut matches = Vec::new();
    visit_files(&root, &mut |path| {
        if let Ok(content) = fs::read_to_string(path) {
            for (index, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    matches.push(json!({
                        "path": path.display().to_string(),
                        "line": index + 1,
                        "content": line,
                    }));
                }
            }
        }
    })?;

    Ok(json!({
        "pattern": pattern,
        "matches": matches
    })
    .to_string())
}

fn visit_files(root: &Path, visitor: &mut impl FnMut(&Path)) -> Result<(), ToolError> {
    if root.is_file() {
        visitor(root);
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(|error| ToolError::new(error.to_string()))? {
        let entry = entry.map_err(|error| ToolError::new(error.to_string()))?;
        let path = entry.path();
        if path.is_dir() {
            visit_files(&path, visitor)?;
        } else {
            visitor(&path);
        }
    }
    Ok(())
}

fn required_string<'a>(input: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    input
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::new(format!("missing string field `{key}`")))
}

#[cfg(test)]
mod tests {
    use super::{
        execute_builtin, GlobalToolRegistry, SkillCatalog, ToolSearchMode, ToolSearchSettings,
    };
    use opencowork_mcp::{sample_tool_definition, StaticMcpExecutor};
    use serde_json::json;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("opencowork-tools-{stamp}"))
    }

    #[test]
    fn file_tools_cover_basic_read_write_edit() {
        let root = temp_dir();
        fs::create_dir_all(&root).expect("create root");
        let path = root.join("demo.txt");
        execute_builtin(
            &mut GlobalToolRegistry::builtin(),
            "write_file",
            &json!({"path": path.display().to_string(), "content": "alpha\nbeta"}),
        )
        .expect("write");
        let read = execute_builtin(
            &mut GlobalToolRegistry::builtin(),
            "read_file",
            &json!({"path": path.display().to_string()}),
        )
        .expect("read");
        assert!(read.contains("alpha"));

        execute_builtin(
            &mut GlobalToolRegistry::builtin(),
            "edit_file",
            &json!({
                "path": path.display().to_string(),
                "old_string": "alpha",
                "new_string": "omega"
            }),
        )
        .expect("edit");
        let edited = fs::read_to_string(&path).expect("read file");
        assert!(edited.contains("omega"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn registry_exposes_builtin_and_mcp_definitions() {
        let mut registry = GlobalToolRegistry::with_extensions(
            Vec::new(),
            vec![sample_tool_definition("demo", "status")],
            Some(Box::new(StaticMcpExecutor::new().register(
                "demo",
                "status",
                |_input| Ok("{\"ok\":true}".to_string()),
            ))),
            SkillCatalog::default(),
            Vec::new(),
        )
        .expect("registry");
        assert!(registry
            .definitions()
            .iter()
            .any(|definition| definition.0 == "bash"));
        assert!(registry
            .definitions()
            .iter()
            .all(|definition| definition.0 != "Skill"));
        assert!(registry
            .execute("mcp__demo__status", &json!({}))
            .expect("mcp execute")
            .contains("true"));
    }

    #[test]
    fn tool_search_can_activate_deferred_skill_tool() {
        let root = temp_dir();
        let skill_dir = root.join("planner");
        fs::create_dir_all(&skill_dir).expect("skill dir");
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: \"Planning guidance\"\n---\nUse a plan.\n",
        )
        .expect("skill");
        let skill_catalog = SkillCatalog::discover(std::slice::from_ref(&root));
        let mut registry = GlobalToolRegistry::with_extensions(
            Vec::new(),
            Vec::new(),
            None,
            skill_catalog,
            Vec::new(),
        )
        .expect("registry");

        let searched = registry
            .execute("ToolSearch", &json!({"query": "skill"}))
            .expect("tool search");
        assert!(searched.contains("Skill"));
        assert!(registry
            .definitions()
            .iter()
            .all(|definition| definition.0 != "Skill"));

        let activated = registry
            .execute("ToolSearch", &json!({"query": "select:Skill"}))
            .expect("activate");
        assert!(activated.contains("Skill"));
        assert!(registry
            .definitions()
            .iter()
            .any(|definition| definition.0 == "Skill"));

        let loaded = registry
            .execute("Skill", &json!({"skill": "planner"}))
            .expect("load skill");
        assert!(loaded.contains("Use a plan."));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tool_search_can_activate_deferred_lsp_tool() {
        let mut registry = GlobalToolRegistry::builtin();

        let searched = registry
            .execute("ToolSearch", &json!({"query": "lsp"}))
            .expect("tool search");
        assert!(searched.contains("LspContext"));
        assert!(registry
            .definitions()
            .iter()
            .all(|definition| definition.0 != "LspContext"));

        let activated = registry
            .execute("ToolSearch", &json!({"query": "select:LspContext"}))
            .expect("activate");
        assert!(activated.contains("LspContext"));
        assert!(registry
            .definitions()
            .iter()
            .any(|definition| definition.0 == "LspContext"));
    }

    #[test]
    fn tool_search_standard_mode_inlines_deferred_tools() {
        let registry =
            GlobalToolRegistry::builtin().with_tool_search_settings(ToolSearchSettings {
                mode: ToolSearchMode::Standard,
                model: "gpt-5.4-mini".to_string(),
                context_window_tokens_override: Some(200_000),
            });

        assert!(registry
            .definitions()
            .iter()
            .any(|definition| definition.0 == "Skill"));
        let diagnostics = registry.tool_search_diagnostics();
        assert!(diagnostics.deferred_tools_inline);
    }

    #[test]
    fn tool_search_auto_mode_can_inline_small_deferred_pool() {
        let registry =
            GlobalToolRegistry::builtin().with_tool_search_settings(ToolSearchSettings {
                mode: ToolSearchMode::DeferredAuto,
                model: "gpt-5.4-mini".to_string(),
                context_window_tokens_override: Some(1_000_000),
            });

        let diagnostics = registry.tool_search_diagnostics();
        assert!(diagnostics.deferred_tools_inline);
        assert!(registry
            .definitions()
            .iter()
            .any(|definition| definition.0 == "LspContext"));
    }
}

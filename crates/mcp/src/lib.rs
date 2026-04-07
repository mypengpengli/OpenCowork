mod transport;

pub use transport::TransportMcpExecutor;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    Stdio,
    Http,
    Sse,
    Ws,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct McpOAuthConfig {
    pub client_id: Option<String>,
    pub callback_port: Option<u16>,
    pub auth_server_metadata_url: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub authorize_url: Option<String>,
    pub token_url: Option<String>,
    pub manual_redirect_url: Option<String>,
    pub xaa: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum McpAuthConfig {
    #[default]
    None,
    BearerEnv {
        token_env: String,
    },
    BearerFile {
        token_path: String,
    },
    OAuth {
        #[serde(flatten)]
        oauth: McpOAuthConfig,
    },
}

impl McpAuthConfig {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::BearerEnv { .. } => "bearer-env",
            Self::BearerFile { .. } => "bearer-file",
            Self::OAuth { .. } => "oauth",
        }
    }

    #[must_use]
    pub const fn requires_credentials(&self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum McpToolPermission {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub server_name: String,
    pub tool_name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    #[serde(rename = "requiredPermission", default = "default_tool_permission")]
    pub required_permission: McpToolPermission,
}

fn default_tool_permission() -> McpToolPermission {
    McpToolPermission::ReadOnly
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResourceDefinition {
    pub server_name: String,
    pub uri: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpResourceContent {
    pub server_name: String,
    pub uri: String,
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpCredentialRecord {
    pub access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_unix_ms: Option<u128>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpCredentialStore {
    root: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum McpCredentialStoreError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

impl McpCredentialStore {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn load(
        &self,
        server_name: &str,
    ) -> Result<Option<McpCredentialRecord>, McpCredentialStoreError> {
        let path = self.path_for(server_name);
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        Ok(Some(serde_json::from_str(&contents)?))
    }

    pub fn save(
        &self,
        server_name: &str,
        record: &McpCredentialRecord,
    ) -> Result<(), McpCredentialStoreError> {
        fs::create_dir_all(&self.root)?;
        fs::write(
            self.path_for(server_name),
            serde_json::to_string_pretty(record)?,
        )?;
        Ok(())
    }

    pub fn clear(&self, server_name: &str) -> Result<(), McpCredentialStoreError> {
        let path = self.path_for(server_name);
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn path_for(&self, server_name: &str) -> PathBuf {
        self.root
            .join(format!("{}.json", normalize_name_for_mcp(server_name)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerDefinition {
    pub name: String,
    pub transport: McpTransport,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub endpoint: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub auth: McpAuthConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub tools: Vec<McpToolDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolBinding {
    pub server_name: String,
    pub tool_name: String,
}

pub trait McpExecutor {
    fn execute(&self, binding: &McpToolBinding, input: &Value) -> Result<String, String>;
}

type McpHandler = Box<dyn Fn(&Value) -> Result<String, String> + Send + Sync>;

#[derive(Default)]
pub struct StaticMcpExecutor {
    handlers: BTreeMap<String, McpHandler>,
}

impl StaticMcpExecutor {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn register(
        mut self,
        server_name: impl Into<String>,
        tool_name: impl Into<String>,
        handler: impl Fn(&Value) -> Result<String, String> + Send + Sync + 'static,
    ) -> Self {
        self.handlers.insert(
            mcp_tool_name(&server_name.into(), &tool_name.into()),
            Box::new(handler),
        );
        self
    }
}

impl McpExecutor for StaticMcpExecutor {
    fn execute(&self, binding: &McpToolBinding, input: &Value) -> Result<String, String> {
        self.handlers
            .get(&mcp_tool_name(&binding.server_name, &binding.tool_name))
            .ok_or_else(|| {
                format!(
                    "no MCP executor bound for {}",
                    mcp_tool_name(&binding.server_name, &binding.tool_name)
                )
            })?(input)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct McpCatalog {
    servers: BTreeMap<String, McpServerDefinition>,
}

impl McpCatalog {
    #[must_use]
    pub fn new(servers: BTreeMap<String, McpServerDefinition>) -> Self {
        Self { servers }
    }

    #[must_use]
    pub fn servers(&self) -> &BTreeMap<String, McpServerDefinition> {
        &self.servers
    }

    #[must_use]
    pub fn tool_binding(&self, server_name: &str, tool_name: &str) -> McpToolBinding {
        McpToolBinding {
            server_name: server_name.to_string(),
            tool_name: tool_name.to_string(),
        }
    }

    #[must_use]
    pub fn tool_definitions(&self) -> Vec<McpToolDefinition> {
        self.servers
            .values()
            .flat_map(|server| server.tools.clone())
            .collect()
    }
}

#[must_use]
pub fn normalize_name_for_mcp(name: &str) -> String {
    name.chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' => ch,
            _ => '_',
        })
        .collect::<String>()
}

#[must_use]
pub fn mcp_tool_prefix(server_name: &str) -> String {
    format!("mcp__{}__", normalize_name_for_mcp(server_name))
}

#[must_use]
pub fn mcp_tool_name(server_name: &str, tool_name: &str) -> String {
    format!(
        "{}{}",
        mcp_tool_prefix(server_name),
        normalize_name_for_mcp(tool_name)
    )
}

#[must_use]
pub fn mcp_server_signature(server: &McpServerDefinition) -> String {
    let auth = server.auth.label();
    let timeout = server
        .timeout_ms
        .map_or_else(String::new, |value| format!(":{value}"));
    match server.transport {
        McpTransport::Stdio => format!(
            "stdio:{}:{}:{auth}{timeout}",
            server.command.as_deref().unwrap_or_default(),
            server.args.join("|")
        ),
        McpTransport::Http | McpTransport::Sse | McpTransport::Ws => {
            format!(
                "remote:{}:{auth}{timeout}",
                server.endpoint.as_deref().unwrap_or_default()
            )
        }
    }
}

#[must_use]
pub fn sample_tool_definition(server_name: &str, tool_name: &str) -> McpToolDefinition {
    McpToolDefinition {
        server_name: server_name.to_string(),
        tool_name: tool_name.to_string(),
        description: format!("MCP tool `{tool_name}` exposed by `{server_name}`"),
        input_schema: json!({ "type": "object" }),
        required_permission: McpToolPermission::ReadOnly,
    }
}

#[must_use]
pub fn merge_tool_definitions(
    configured: Vec<McpToolDefinition>,
    discovered: Vec<McpToolDefinition>,
) -> Vec<McpToolDefinition> {
    let mut merged = BTreeMap::new();
    for tool in configured.into_iter().chain(discovered) {
        merged.insert(mcp_tool_name(&tool.server_name, &tool.tool_name), tool);
    }
    merged.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::{
        mcp_server_signature, mcp_tool_name, normalize_name_for_mcp, sample_tool_definition,
        McpCatalog, McpCredentialRecord, McpCredentialStore, McpExecutor, McpServerDefinition,
        McpTransport, StaticMcpExecutor,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("opencowork-mcp-{label}-{stamp}"))
    }

    #[test]
    fn normalizes_server_and_tool_names() {
        assert_eq!(normalize_name_for_mcp("github.com"), "github_com");
        assert_eq!(
            mcp_tool_name("demo server", "weather-tool"),
            "mcp__demo_server__weather-tool"
        );
    }

    #[test]
    fn computes_stable_signatures() {
        let stdio = McpServerDefinition {
            name: "demo".to_string(),
            transport: McpTransport::Stdio,
            command: Some("uvx".to_string()),
            args: vec!["mcp-server".to_string()],
            endpoint: None,
            env: BTreeMap::new(),
            headers: BTreeMap::new(),
            auth: super::McpAuthConfig::None,
            timeout_ms: None,
            tools: vec![sample_tool_definition("demo", "status")],
        };
        assert!(mcp_server_signature(&stdio).starts_with("stdio:uvx"));
    }

    #[test]
    fn exposes_tools_and_executes_static_handler() {
        let catalog = McpCatalog::new(BTreeMap::from([(
            "demo".to_string(),
            McpServerDefinition {
                name: "demo".to_string(),
                transport: McpTransport::Http,
                command: None,
                args: Vec::new(),
                endpoint: Some("https://example.test/mcp".to_string()),
                env: BTreeMap::new(),
                headers: BTreeMap::new(),
                auth: super::McpAuthConfig::None,
                timeout_ms: None,
                tools: vec![sample_tool_definition("demo", "status")],
            },
        )]));
        assert_eq!(catalog.tool_definitions().len(), 1);

        let executor = StaticMcpExecutor::new().register("demo", "status", |_input| {
            Ok(json!({"ok": true}).to_string())
        });
        let output = executor
            .execute(&catalog.tool_binding("demo", "status"), &json!({}))
            .expect("mcp execute");
        assert!(output.contains("true"));
    }

    #[test]
    fn persists_credentials_by_server_name() {
        let root = temp_dir("creds");
        let store = McpCredentialStore::new(&root);
        store
            .save(
                "github.com",
                &McpCredentialRecord {
                    access_token: "secret".to_string(),
                    refresh_token: Some("refresh".to_string()),
                    expires_at_unix_ms: Some(123),
                    scopes: vec!["repo".to_string()],
                },
            )
            .expect("save creds");
        let loaded = store.load("github.com").expect("load creds");
        assert_eq!(loaded.expect("record").access_token, "secret");
        store.clear("github.com").expect("clear creds");
        assert_eq!(store.load("github.com").expect("load cleared"), None);
        let _ = fs::remove_dir_all(root);
    }
}

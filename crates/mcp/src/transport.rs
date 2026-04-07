use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Cursor, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;
use tungstenite::client::IntoClientRequest;
use tungstenite::Message;

use crate::{
    McpAuthConfig, McpCatalog, McpCredentialStore, McpExecutor, McpResourceContent,
    McpResourceDefinition, McpServerDefinition, McpToolBinding, McpToolDefinition,
    McpToolPermission, McpTransport,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
enum JsonRpcId {
    Number(u64),
    String(String),
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct JsonRpcRequest<T = Value> {
    jsonrpc: String,
    id: JsonRpcId,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<T>,
}

impl<T> JsonRpcRequest<T> {
    fn new(id: JsonRpcId, method: impl Into<String>, params: Option<T>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct JsonRpcResponse<T = Value> {
    jsonrpc: String,
    id: JsonRpcId,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct McpInitializeParams {
    protocol_version: String,
    capabilities: Value,
    client_info: McpInitializeClientInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct McpInitializeClientInfo {
    name: String,
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct McpListToolsParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct McpRemoteTool {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(rename = "inputSchema", skip_serializing_if = "Option::is_none")]
    input_schema: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct McpListToolsResult {
    tools: Vec<McpRemoteTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct McpListResourcesParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct McpRemoteResource {
    uri: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct McpListResourcesResult {
    resources: Vec<McpRemoteResource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct McpReadResourceParams {
    uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct McpRemoteResourceContent {
    uri: String,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blob: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct McpReadResourceResult {
    #[serde(default)]
    contents: Vec<McpRemoteResourceContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct McpToolCallParams {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct McpToolCallResult {
    #[serde(default)]
    content: Vec<Value>,
    #[serde(default)]
    structured_content: Option<Value>,
    #[serde(default)]
    is_error: Option<bool>,
}

#[derive(Debug)]
struct ManagedMcpServer {
    definition: McpServerDefinition,
    stdio: Option<McpStdioProcess>,
    initialized: bool,
}

impl ManagedMcpServer {
    fn new(definition: McpServerDefinition) -> Self {
        Self {
            definition,
            stdio: None,
            initialized: false,
        }
    }
}

#[derive(Debug, Default)]
struct TransportState {
    servers: BTreeMap<String, ManagedMcpServer>,
    next_request_id: u64,
}

impl TransportState {
    fn take_request_id(&mut self) -> JsonRpcId {
        self.next_request_id = self.next_request_id.saturating_add(1).max(1);
        JsonRpcId::Number(self.next_request_id)
    }
}

pub struct TransportMcpExecutor {
    catalog: McpCatalog,
    http: reqwest::blocking::Client,
    credential_store: Option<McpCredentialStore>,
    state: Mutex<TransportState>,
}

impl TransportMcpExecutor {
    #[must_use]
    pub fn new(catalog: McpCatalog) -> Self {
        Self::with_credential_store(catalog, None::<PathBuf>)
    }

    #[must_use]
    pub fn with_credential_store(
        catalog: McpCatalog,
        credential_root: impl Into<Option<PathBuf>>,
    ) -> Self {
        let servers = catalog
            .servers()
            .iter()
            .map(|(name, definition)| (name.clone(), ManagedMcpServer::new(definition.clone())))
            .collect::<BTreeMap<_, _>>();
        Self {
            catalog,
            http: reqwest::blocking::Client::new(),
            credential_store: credential_root.into().map(McpCredentialStore::new),
            state: Mutex::new(TransportState {
                servers,
                next_request_id: 0,
            }),
        }
    }

    pub fn discover_tools(&self) -> Result<Vec<McpToolDefinition>, String> {
        let server_names = self.catalog.servers().keys().cloned().collect::<Vec<_>>();
        let mut state = self
            .state
            .lock()
            .map_err(|_| "failed to lock MCP transport state".to_string())?;
        let mut discovered = Vec::new();

        for server_name in server_names {
            let mut cursor = None;
            loop {
                let request_id = state.take_request_id();
                let result: McpListToolsResult = match server_transport(&state, &server_name)? {
                    McpTransport::Stdio => {
                        let server = ensure_stdio_ready(&mut state, &server_name)?;
                        let process = server.stdio.as_mut().ok_or_else(|| {
                            format!("MCP stdio process missing for `{server_name}`")
                        })?;
                        read_jsonrpc_result(
                            process.request(
                                request_id,
                                "tools/list",
                                Some(McpListToolsParams {
                                    cursor: cursor.clone(),
                                }),
                            )?,
                            &server_name,
                            "tools/list",
                        )?
                    }
                    McpTransport::Http | McpTransport::Sse | McpTransport::Ws => {
                        let definition = state
                            .servers
                            .get_mut(&server_name)
                            .ok_or_else(|| format!("unknown MCP server `{server_name}`"))?;
                        if !definition.initialized {
                            initialize_remote_server(
                                &self.http,
                                &mut definition.definition,
                                self.credential_store.as_ref(),
                            )?;
                            definition.initialized = true;
                        }
                        read_jsonrpc_result(
                            remote_jsonrpc_request(
                                &self.http,
                                &definition.definition,
                                self.credential_store.as_ref(),
                                JsonRpcRequest::new(
                                    request_id,
                                    "tools/list",
                                    Some(McpListToolsParams {
                                        cursor: cursor.clone(),
                                    }),
                                ),
                            )?,
                            &server_name,
                            "tools/list",
                        )?
                    }
                };

                for tool in result.tools {
                    discovered.push(McpToolDefinition {
                        server_name: server_name.clone(),
                        tool_name: tool.name.clone(),
                        description: tool.description.unwrap_or_else(|| {
                            format!("MCP tool `{}` from `{}`", tool.name, server_name)
                        }),
                        input_schema: tool
                            .input_schema
                            .unwrap_or_else(|| json!({"type": "object"})),
                        required_permission: McpToolPermission::ReadOnly,
                    });
                }

                if let Some(next_cursor) = result.next_cursor {
                    cursor = Some(next_cursor);
                } else {
                    break;
                }
            }
        }

        Ok(discovered)
    }

    pub fn discover_resources(&self) -> Result<Vec<McpResourceDefinition>, String> {
        let server_names = self.catalog.servers().keys().cloned().collect::<Vec<_>>();
        let mut state = self
            .state
            .lock()
            .map_err(|_| "failed to lock MCP transport state".to_string())?;
        let mut discovered = Vec::new();

        for server_name in server_names {
            let mut cursor = None;
            loop {
                let request_id = state.take_request_id();
                let result: McpListResourcesResult = match server_transport(&state, &server_name)? {
                    McpTransport::Stdio => {
                        let server = ensure_stdio_ready(&mut state, &server_name)?;
                        let process = server.stdio.as_mut().ok_or_else(|| {
                            format!("MCP stdio process missing for `{server_name}`")
                        })?;
                        read_jsonrpc_result(
                            process.request(
                                request_id,
                                "resources/list",
                                Some(McpListResourcesParams {
                                    cursor: cursor.clone(),
                                }),
                            )?,
                            &server_name,
                            "resources/list",
                        )?
                    }
                    McpTransport::Http | McpTransport::Sse | McpTransport::Ws => {
                        let definition = state
                            .servers
                            .get_mut(&server_name)
                            .ok_or_else(|| format!("unknown MCP server `{server_name}`"))?;
                        if !definition.initialized {
                            initialize_remote_server(
                                &self.http,
                                &mut definition.definition,
                                self.credential_store.as_ref(),
                            )?;
                            definition.initialized = true;
                        }
                        read_jsonrpc_result(
                            remote_jsonrpc_request(
                                &self.http,
                                &definition.definition,
                                self.credential_store.as_ref(),
                                JsonRpcRequest::new(
                                    request_id,
                                    "resources/list",
                                    Some(McpListResourcesParams {
                                        cursor: cursor.clone(),
                                    }),
                                ),
                            )?,
                            &server_name,
                            "resources/list",
                        )?
                    }
                };

                for resource in result.resources {
                    discovered.push(McpResourceDefinition {
                        server_name: server_name.clone(),
                        uri: resource.uri,
                        name: resource.name,
                        description: resource
                            .description
                            .unwrap_or_else(|| format!("MCP resource from `{server_name}`")),
                        mime_type: resource.mime_type,
                    });
                }

                if let Some(next_cursor) = result.next_cursor {
                    cursor = Some(next_cursor);
                } else {
                    break;
                }
            }
        }

        Ok(discovered)
    }

    pub fn read_resource(
        &self,
        server_name: &str,
        uri: &str,
    ) -> Result<Vec<McpResourceContent>, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "failed to lock MCP transport state".to_string())?;
        let request_id = state.take_request_id();
        let result: McpReadResourceResult = match server_transport(&state, server_name)? {
            McpTransport::Stdio => {
                let server = ensure_stdio_ready(&mut state, server_name)?;
                let process = server
                    .stdio
                    .as_mut()
                    .ok_or_else(|| format!("MCP stdio process missing for `{server_name}`"))?;
                read_jsonrpc_result(
                    process.request(
                        request_id,
                        "resources/read",
                        Some(McpReadResourceParams {
                            uri: uri.to_string(),
                        }),
                    )?,
                    server_name,
                    "resources/read",
                )?
            }
            McpTransport::Http | McpTransport::Sse | McpTransport::Ws => {
                let server = state
                    .servers
                    .get_mut(server_name)
                    .ok_or_else(|| format!("unknown MCP server `{server_name}`"))?;
                if !server.initialized {
                    initialize_remote_server(
                        &self.http,
                        &mut server.definition,
                        self.credential_store.as_ref(),
                    )?;
                    server.initialized = true;
                }
                read_jsonrpc_result(
                    remote_jsonrpc_request(
                        &self.http,
                        &server.definition,
                        self.credential_store.as_ref(),
                        JsonRpcRequest::new(
                            request_id,
                            "resources/read",
                            Some(McpReadResourceParams {
                                uri: uri.to_string(),
                            }),
                        ),
                    )?,
                    server_name,
                    "resources/read",
                )?
            }
        };

        Ok(result
            .contents
            .into_iter()
            .map(|content| McpResourceContent {
                server_name: server_name.to_string(),
                uri: content.uri,
                mime_type: content.mime_type,
                text: content.text,
                blob: content.blob,
            })
            .collect())
    }
}

impl McpExecutor for TransportMcpExecutor {
    fn execute(&self, binding: &McpToolBinding, input: &Value) -> Result<String, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "failed to lock MCP transport state".to_string())?;
        let request_id = state.take_request_id();
        let result: McpToolCallResult = match server_transport(&state, &binding.server_name)? {
            McpTransport::Stdio => {
                let server = ensure_stdio_ready(&mut state, &binding.server_name)?;
                let process = server.stdio.as_mut().ok_or_else(|| {
                    format!("MCP stdio process missing for `{}`", binding.server_name)
                })?;
                read_jsonrpc_result(
                    process.request(
                        request_id,
                        "tools/call",
                        Some(McpToolCallParams {
                            name: binding.tool_name.clone(),
                            arguments: Some(input.clone()),
                        }),
                    )?,
                    &binding.server_name,
                    "tools/call",
                )?
            }
            McpTransport::Http | McpTransport::Sse | McpTransport::Ws => {
                let server = state
                    .servers
                    .get_mut(&binding.server_name)
                    .ok_or_else(|| format!("unknown MCP server `{}`", binding.server_name))?;
                if !server.initialized {
                    initialize_remote_server(
                        &self.http,
                        &mut server.definition,
                        self.credential_store.as_ref(),
                    )?;
                    server.initialized = true;
                }
                read_jsonrpc_result(
                    remote_jsonrpc_request(
                        &self.http,
                        &server.definition,
                        self.credential_store.as_ref(),
                        JsonRpcRequest::new(
                            request_id,
                            "tools/call",
                            Some(McpToolCallParams {
                                name: binding.tool_name.clone(),
                                arguments: Some(input.clone()),
                            }),
                        ),
                    )?,
                    &binding.server_name,
                    "tools/call",
                )?
            }
        };

        let output = summarize_tool_result(&result);
        if result.is_error.unwrap_or(false) {
            Err(output)
        } else {
            Ok(output)
        }
    }
}

impl Drop for TransportMcpExecutor {
    fn drop(&mut self) {
        if let Ok(state) = self.state.get_mut() {
            for server in state.servers.values_mut() {
                if let Some(process) = server.stdio.as_mut() {
                    let _ = process.shutdown();
                }
            }
        }
    }
}

fn server_transport(state: &TransportState, server_name: &str) -> Result<McpTransport, String> {
    state
        .servers
        .get(server_name)
        .map(|server| server.definition.transport)
        .ok_or_else(|| format!("unknown MCP server `{server_name}`"))
}

fn ensure_stdio_ready<'a>(
    state: &'a mut TransportState,
    server_name: &str,
) -> Result<&'a mut ManagedMcpServer, String> {
    let transport = state
        .servers
        .get(server_name)
        .ok_or_else(|| format!("unknown MCP server `{server_name}`"))?;
    if transport.definition.transport != McpTransport::Stdio {
        return Err(format!(
            "MCP server `{server_name}` is not configured for stdio transport"
        ));
    }
    let needs_spawn = state
        .servers
        .get(server_name)
        .is_some_and(|server| server.stdio.is_none());
    if needs_spawn {
        let server = state
            .servers
            .get_mut(server_name)
            .ok_or_else(|| format!("unknown MCP server `{server_name}`"))?;
        server.stdio = Some(McpStdioProcess::spawn(&server.definition)?);
        server.initialized = false;
    }
    let needs_initialize = state
        .servers
        .get(server_name)
        .is_some_and(|server| !server.initialized);
    if needs_initialize {
        let request_id = state.take_request_id();
        let response: JsonRpcResponse<Value> = {
            let server = state
                .servers
                .get_mut(server_name)
                .ok_or_else(|| format!("unknown MCP server `{server_name}`"))?;
            let process = server
                .stdio
                .as_mut()
                .ok_or_else(|| format!("MCP stdio process missing for `{server_name}`"))?;
            process.request(request_id, "initialize", Some(default_initialize_params()))?
        };
        read_jsonrpc_result(response, server_name, "initialize")?;
        let server = state
            .servers
            .get_mut(server_name)
            .ok_or_else(|| format!("unknown MCP server `{server_name}`"))?;
        server.initialized = true;
    }
    state
        .servers
        .get_mut(server_name)
        .ok_or_else(|| format!("unknown MCP server `{server_name}`"))
}

fn initialize_remote_server(
    http: &reqwest::blocking::Client,
    definition: &mut McpServerDefinition,
    credential_store: Option<&McpCredentialStore>,
) -> Result<(), String> {
    let response: JsonRpcResponse<Value> = remote_jsonrpc_request(
        http,
        definition,
        credential_store,
        JsonRpcRequest::new(
            JsonRpcId::Number(1),
            "initialize",
            Some(default_initialize_params()),
        ),
    )?;
    let server_name = definition.name.clone();
    read_jsonrpc_result(response, &server_name, "initialize")?;
    Ok(())
}

fn read_jsonrpc_result<T>(
    response: JsonRpcResponse<T>,
    server_name: &str,
    method: &str,
) -> Result<T, String> {
    if let Some(error) = response.error {
        return Err(format!(
            "MCP server `{server_name}` returned JSON-RPC error for {method}: {} ({})",
            error.message, error.code
        ));
    }
    response
        .result
        .ok_or_else(|| format!("MCP server `{server_name}` returned no result for {method}"))
}

fn remote_jsonrpc_request<TParams: Serialize, TResult: DeserializeOwned>(
    http: &reqwest::blocking::Client,
    definition: &McpServerDefinition,
    credential_store: Option<&McpCredentialStore>,
    request: JsonRpcRequest<TParams>,
) -> Result<JsonRpcResponse<TResult>, String> {
    match definition.transport {
        McpTransport::Http => http_jsonrpc_request(http, definition, credential_store, request),
        McpTransport::Sse => sse_jsonrpc_request(http, definition, credential_store, request),
        McpTransport::Ws => websocket_jsonrpc_request(definition, credential_store, request),
        McpTransport::Stdio => Err(format!(
            "remote JSON-RPC requested for stdio server `{}`",
            definition.name
        )),
    }
}

fn http_jsonrpc_request<TParams: Serialize, TResult: DeserializeOwned>(
    http: &reqwest::blocking::Client,
    definition: &McpServerDefinition,
    credential_store: Option<&McpCredentialStore>,
    request: JsonRpcRequest<TParams>,
) -> Result<JsonRpcResponse<TResult>, String> {
    let endpoint = definition
        .endpoint
        .as_deref()
        .ok_or_else(|| format!("MCP server `{}` is missing endpoint", definition.name))?;
    let mut builder = http
        .post(endpoint)
        .header("content-type", "application/json");
    for (key, value) in &definition.headers {
        builder = builder.header(key, value);
    }
    builder = apply_remote_auth(builder, definition, credential_store)?;
    if let Some(timeout_ms) = definition.timeout_ms {
        builder = builder.timeout(Duration::from_millis(timeout_ms));
    }

    let response = builder
        .json(&request)
        .send()
        .map_err(|error| error.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(format!(
            "MCP HTTP request to `{}` failed with {}: {}",
            endpoint,
            status,
            body.trim()
        ));
    }
    response
        .json::<JsonRpcResponse<TResult>>()
        .map_err(|error| error.to_string())
}

fn sse_jsonrpc_request<TParams: Serialize, TResult: DeserializeOwned>(
    http: &reqwest::blocking::Client,
    definition: &McpServerDefinition,
    credential_store: Option<&McpCredentialStore>,
    request: JsonRpcRequest<TParams>,
) -> Result<JsonRpcResponse<TResult>, String> {
    let endpoint = definition
        .endpoint
        .as_deref()
        .ok_or_else(|| format!("MCP server `{}` is missing endpoint", definition.name))?;
    let mut builder = http
        .post(endpoint)
        .header("content-type", "application/json")
        .header("accept", "text/event-stream, application/json");
    for (key, value) in &definition.headers {
        builder = builder.header(key, value);
    }
    builder = apply_remote_auth(builder, definition, credential_store)?;
    if let Some(timeout_ms) = definition.timeout_ms {
        builder = builder.timeout(Duration::from_millis(timeout_ms));
    }

    let response = builder
        .json(&request)
        .send()
        .map_err(|error| error.to_string())?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(format!(
            "MCP SSE request to `{}` failed with {}: {}",
            endpoint,
            status,
            body.trim()
        ));
    }

    let is_sse = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("text/event-stream"));
    let body = response.bytes().map_err(|error| error.to_string())?;
    if is_sse {
        parse_sse_jsonrpc_response(body.as_ref())
    } else {
        serde_json::from_slice::<JsonRpcResponse<TResult>>(body.as_ref())
            .map_err(|error| error.to_string())
    }
}

fn websocket_jsonrpc_request<TParams: Serialize, TResult: DeserializeOwned>(
    definition: &McpServerDefinition,
    credential_store: Option<&McpCredentialStore>,
    request: JsonRpcRequest<TParams>,
) -> Result<JsonRpcResponse<TResult>, String> {
    let endpoint = definition
        .endpoint
        .as_deref()
        .ok_or_else(|| format!("MCP server `{}` is missing endpoint", definition.name))?;
    let mut ws_request = endpoint
        .into_client_request()
        .map_err(|error| error.to_string())?;
    for (key, value) in &definition.headers {
        let name = tungstenite::http::header::HeaderName::from_bytes(key.as_bytes())
            .map_err(|error| error.to_string())?;
        let value = tungstenite::http::header::HeaderValue::from_str(value)
            .map_err(|error| error.to_string())?;
        ws_request.headers_mut().insert(name, value);
    }
    if let Some(token) = resolve_auth_token(definition, credential_store)? {
        let value = tungstenite::http::header::HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|error| error.to_string())?;
        ws_request
            .headers_mut()
            .insert(tungstenite::http::header::AUTHORIZATION, value);
    }

    let (mut socket, _response) =
        tungstenite::connect(ws_request).map_err(|error| error.to_string())?;
    let payload = serde_json::to_string(&request).map_err(|error| error.to_string())?;
    socket
        .send(Message::Text(payload))
        .map_err(|error| error.to_string())?;

    loop {
        let message = socket.read().map_err(|error| error.to_string())?;
        match message {
            Message::Text(text) => {
                let response = serde_json::from_str::<JsonRpcResponse<TResult>>(&text)
                    .map_err(|error| error.to_string())?;
                return Ok(response);
            }
            Message::Binary(data) => {
                let response = serde_json::from_slice::<JsonRpcResponse<TResult>>(&data)
                    .map_err(|error| error.to_string())?;
                return Ok(response);
            }
            Message::Close(frame) => {
                return Err(format!(
                    "MCP websocket closed before response: {}",
                    frame
                        .as_ref()
                        .map(|frame| frame.reason.to_string())
                        .unwrap_or_else(|| "no close reason".to_string())
                ));
            }
            _ => {}
        }
    }
}

fn apply_remote_auth(
    builder: reqwest::blocking::RequestBuilder,
    definition: &McpServerDefinition,
    credential_store: Option<&McpCredentialStore>,
) -> Result<reqwest::blocking::RequestBuilder, String> {
    let Some(token) = resolve_auth_token(definition, credential_store)? else {
        return Ok(builder);
    };
    Ok(builder.bearer_auth(token))
}

fn resolve_auth_token(
    definition: &McpServerDefinition,
    credential_store: Option<&McpCredentialStore>,
) -> Result<Option<String>, String> {
    match &definition.auth {
        McpAuthConfig::None => Ok(None),
        McpAuthConfig::BearerEnv { token_env } => {
            let token = std::env::var(token_env).map_err(|_| {
                format!(
                    "MCP server `{}` requires auth from env `{token_env}`, but it is missing",
                    definition.name
                )
            })?;
            if token.trim().is_empty() {
                return Err(format!(
                    "MCP server `{}` requires auth from env `{token_env}`, but it is empty",
                    definition.name
                ));
            }
            Ok(Some(token))
        }
        McpAuthConfig::BearerFile { token_path } => {
            let token = fs::read_to_string(token_path).map_err(|error| {
                format!(
                    "failed to read MCP token file for `{}` at `{}`: {error}",
                    definition.name, token_path
                )
            })?;
            let token = token.trim().to_string();
            if token.is_empty() {
                return Err(format!(
                    "MCP token file for `{}` at `{}` is empty",
                    definition.name, token_path
                ));
            }
            Ok(Some(token))
        }
        McpAuthConfig::OAuth { .. } => {
            let Some(store) = credential_store else {
                return Err(format!(
                    "MCP server `{}` requires OAuth credentials, but no credential store is configured",
                    definition.name
                ));
            };
            let record = store
                .load(&definition.name)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| {
                    format!(
                        "MCP server `{}` requires OAuth credentials, but none are saved locally",
                        definition.name
                    )
                })?;
            if record.access_token.trim().is_empty() {
                return Err(format!(
                    "saved OAuth credentials for `{}` are empty",
                    definition.name
                ));
            }
            Ok(Some(record.access_token))
        }
    }
}

fn parse_sse_jsonrpc_response<TResult: DeserializeOwned>(
    body: &[u8],
) -> Result<JsonRpcResponse<TResult>, String> {
    let mut lines = Vec::new();
    let mut payloads = Vec::new();
    for line in BufReader::new(Cursor::new(body)).lines() {
        let line = line.map_err(|error| error.to_string())?;
        if line.is_empty() {
            flush_sse_jsonrpc_event(&mut lines, &mut payloads)?;
            continue;
        }
        lines.push(line);
    }
    flush_sse_jsonrpc_event(&mut lines, &mut payloads)?;

    let value = payloads
        .into_iter()
        .last()
        .ok_or_else(|| "SSE response did not contain a JSON-RPC payload".to_string())?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

fn flush_sse_jsonrpc_event(
    lines: &mut Vec<String>,
    payloads: &mut Vec<Value>,
) -> Result<(), String> {
    if lines.is_empty() {
        return Ok(());
    }

    let payload = lines
        .iter()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    lines.clear();

    if payload.is_empty() || payload == "[DONE]" {
        return Ok(());
    }

    payloads.push(
        serde_json::from_str::<Value>(&payload)
            .map_err(|error| format!("invalid MCP SSE payload: {error}"))?,
    );
    Ok(())
}

fn summarize_tool_result(result: &McpToolCallResult) -> String {
    if let Some(structured) = &result.structured_content {
        return structured.to_string();
    }

    let text_fragments = result
        .content
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>();
    if !text_fragments.is_empty() {
        return text_fragments.join("\n");
    }

    if result.content.is_empty() {
        "{}".to_string()
    } else {
        json!({ "content": result.content }).to_string()
    }
}

#[derive(Debug)]
struct McpStdioProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl McpStdioProcess {
    fn spawn(definition: &McpServerDefinition) -> Result<Self, String> {
        let command_name = definition
            .command
            .as_deref()
            .ok_or_else(|| format!("MCP stdio server `{}` is missing command", definition.name))?;
        let mut command = Command::new(command_name);
        command
            .args(&definition.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        for (key, value) in &definition.env {
            command.env(key, value);
        }

        let mut child = command.spawn().map_err(|error| error.to_string())?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "stdio MCP process missing stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "stdio MCP process missing stdout".to_string())?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    fn request<TParams: Serialize, TResult: DeserializeOwned>(
        &mut self,
        id: JsonRpcId,
        method: impl Into<String>,
        params: Option<TParams>,
    ) -> Result<JsonRpcResponse<TResult>, String> {
        let payload = serde_json::to_vec(&JsonRpcRequest::new(id, method, params))
            .map_err(|error| error.to_string())?;
        let header = format!("Content-Length: {}\r\n\r\n", payload.len());
        self.stdin
            .write_all(header.as_bytes())
            .and_then(|_| self.stdin.write_all(&payload))
            .and_then(|_| self.stdin.flush())
            .map_err(|error| error.to_string())?;

        let frame = self.read_frame()?;
        serde_json::from_slice(&frame).map_err(|error| error.to_string())
    }

    fn read_frame(&mut self) -> Result<Vec<u8>, String> {
        let mut content_length = None;
        loop {
            let mut line = String::new();
            let read = self
                .stdout
                .read_line(&mut line)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                return Err("MCP stdio stream closed while reading headers".to_string());
            }
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length:") {
                let parsed = value
                    .trim()
                    .parse::<usize>()
                    .map_err(|error| error.to_string())?;
                content_length = Some(parsed);
            }
        }

        let content_length =
            content_length.ok_or_else(|| "missing Content-Length header".to_string())?;
        let mut payload = vec![0_u8; content_length];
        self.stdout
            .read_exact(&mut payload)
            .map_err(|error| error.to_string())?;
        Ok(payload)
    }

    fn shutdown(&mut self) -> Result<(), String> {
        if self
            .child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_none()
        {
            self.child.kill().map_err(|error| error.to_string())?;
        }
        let _ = self.child.wait().map_err(|error| error.to_string())?;
        Ok(())
    }
}

fn default_initialize_params() -> McpInitializeParams {
    McpInitializeParams {
        protocol_version: "2025-03-26".to_string(),
        capabilities: Value::Object(serde_json::Map::new()),
        client_info: McpInitializeClientInfo {
            name: "opencowork-runtime".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::{merge_tool_definitions, McpCatalog, McpServerDefinition, McpTransport};

    use super::TransportMcpExecutor;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("opencowork-{label}-{stamp}"))
    }

    #[test]
    fn merge_prefers_discovered_tool_inventory() {
        let merged = merge_tool_definitions(
            vec![crate::sample_tool_definition("demo", "status")],
            vec![crate::McpToolDefinition {
                server_name: "demo".to_string(),
                tool_name: "status".to_string(),
                description: "live".to_string(),
                input_schema: json!({"type": "object"}),
                required_permission: crate::McpToolPermission::ReadOnly,
            }],
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].description, "live");
    }

    #[test]
    fn stdio_executor_discovers_and_executes_tools() {
        let root = temp_dir("mcp-stdio");
        fs::create_dir_all(&root).expect("temp root");
        let script = root.join("mock-mcp.py");
        fs::write(
            &script,
            [
                "import json, sys",
                "def read_frame():",
                "    length = None",
                "    while True:",
                "        line = sys.stdin.buffer.readline()",
                "        if not line:",
                "            raise SystemExit(0)",
                "        if line == b'\\r\\n':",
                "            break",
                "        if line.startswith(b'Content-Length:'):",
                "            length = int(line.split(b':', 1)[1].strip())",
                "    return json.loads(sys.stdin.buffer.read(length))",
                "def write_frame(payload):",
                "    data = json.dumps(payload).encode()",
                "    sys.stdout.buffer.write(f'Content-Length: {len(data)}\\r\\n\\r\\n'.encode() + data)",
                "    sys.stdout.buffer.flush()",
                "while True:",
                "    request = read_frame()",
                "    method = request['method']",
                "    if method == 'initialize':",
                "        write_frame({'jsonrpc':'2.0','id':request['id'],'result':{'ok':True}})",
                "    elif method == 'tools/list':",
                "        write_frame({'jsonrpc':'2.0','id':request['id'],'result':{'tools':[{'name':'status','description':'status','inputSchema':{'type':'object'}}]}})",
                "    elif method == 'tools/call':",
                "        args = request.get('params', {}).get('arguments', {})",
                "        write_frame({'jsonrpc':'2.0','id':request['id'],'result':{'content':[{'text':json.dumps({'ok':True,'args':args})}]}})",
                "    elif method == 'resources/list':",
                "        write_frame({'jsonrpc':'2.0','id':request['id'],'result':{'resources':[{'uri':'file:///README.md','name':'README','description':'Readme','mimeType':'text/markdown'}]}})",
                "    elif method == 'resources/read':",
                "        uri = request.get('params', {}).get('uri', '')",
                "        write_frame({'jsonrpc':'2.0','id':request['id'],'result':{'contents':[{'uri':uri,'mimeType':'text/plain','text':'hello from resource'}]}})",
            ]
            .join("\n"),
        )
        .expect("write script");

        let catalog = McpCatalog::new(BTreeMap::from([(
            "demo".to_string(),
            McpServerDefinition {
                name: "demo".to_string(),
                transport: McpTransport::Stdio,
                command: Some("python".to_string()),
                args: vec![script.display().to_string()],
                endpoint: None,
                env: BTreeMap::new(),
                headers: BTreeMap::new(),
                auth: crate::McpAuthConfig::None,
                timeout_ms: None,
                tools: Vec::new(),
            },
        )]));

        let executor = TransportMcpExecutor::new(catalog.clone());
        let discovered = executor.discover_tools().expect("discover");
        assert_eq!(discovered.len(), 1);
        let resources = executor.discover_resources().expect("resources");
        assert_eq!(resources.len(), 1);
        let contents = executor
            .read_resource("demo", "file:///README.md")
            .expect("read resource");
        assert_eq!(contents[0].text.as_deref(), Some("hello from resource"));

        let output = crate::McpExecutor::execute(
            &executor,
            &catalog.tool_binding("demo", "status"),
            &json!({"hello": "world"}),
        )
        .expect("execute");
        assert!(output.contains("world"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_sse_jsonrpc_payloads() {
        let body = [
            "event: message",
            "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[{\"name\":\"status\"}]}}",
            "",
        ]
        .join("\n");

        let response: super::JsonRpcResponse<super::McpListToolsResult> =
            super::parse_sse_jsonrpc_response(body.as_bytes()).expect("parse sse response");
        assert_eq!(
            response
                .result
                .expect("result")
                .tools
                .first()
                .map(|tool| tool.name.as_str()),
            Some("status")
        );
    }
}

use anyhow::Context;
use axum::extract::{Path as AxumPath, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use opencowork_api::default_openai_profile_for_model;
use opencowork_app::{AppEvent, AppRuntime};
use opencowork_mcp::{McpAuthConfig, McpTransport};
use opencowork_runtime::{default_config_home, ConfigLoader, Session, SessionStore};
use opencowork_skills::{SkillCatalog, SkillExecutionContext, SkillOrigin, SkillSummary};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

const INDEX_HTML: &str = include_str!("../static/index.html");
const APP_CSS: &str = include_str!("../static/app.css");
const APP_JS: &str = include_str!("../static/app.js");

#[derive(Clone)]
struct ShellState {
    cwd: PathBuf,
    config_home: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BootstrapResponse {
    app_name: &'static str,
    cwd: String,
    settings_file: String,
    model: String,
    permission_mode: String,
    provider: ProviderSettingsView,
    team_memory_sync: serde_json::Value,
    sessions: Vec<SessionListItem>,
    skills: Vec<SkillView>,
    mcp_servers: Vec<McpServerView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderSettingsView {
    model: String,
    name: String,
    api_key_env: String,
    base_url: String,
    base_url_env: Option<String>,
    timeout_ms: u64,
    persisted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionListItem {
    id: String,
    message_count: usize,
    updated_at_unix_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillView {
    name: String,
    description: Option<String>,
    path: String,
    origin: String,
    when_to_use: Option<String>,
    argument_hint: Option<String>,
    allowed_tools: Vec<String>,
    paths: Vec<String>,
    execution_context: Option<String>,
    version: Option<String>,
    agent: Option<String>,
    model: Option<String>,
    effort: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpServerView {
    name: String,
    transport: String,
    command: Option<String>,
    args: Vec<String>,
    endpoint: Option<String>,
    timeout_ms: Option<u64>,
    auth_type: String,
    token_env: Option<String>,
    token_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatRequest {
    session_id: Option<String>,
    input: String,
    model: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatResponse {
    session_id: String,
    session: Session,
    events: Vec<AppEvent>,
    iterations: usize,
    estimated_prompt_tokens: usize,
    compacted: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderUpdateRequest {
    model: Option<String>,
    name: String,
    api_key_env: String,
    base_url: String,
    base_url_env: Option<String>,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillUpsertRequest {
    slug: Option<String>,
    name: String,
    description: Option<String>,
    when_to_use: Option<String>,
    argument_hint: Option<String>,
    allowed_tools: Vec<String>,
    paths: Vec<String>,
    execution_context: Option<String>,
    version: Option<String>,
    agent: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpUpsertRequest {
    original_name: Option<String>,
    name: String,
    transport: String,
    command: Option<String>,
    args: Vec<String>,
    endpoint: Option<String>,
    timeout_ms: Option<u64>,
    auth_type: Option<String>,
    token_env: Option<String>,
    token_path: Option<String>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({
                "error": self.message,
            })),
        )
            .into_response()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cwd = env::current_dir().context("failed to resolve current directory")?;
    let config_home = default_config_home();
    let port = env::var("OPENCOWORK_SHELL_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(33211);
    let state = ShellState { cwd, config_home };

    let app = Router::new()
        .route("/", get(index))
        .route("/app.css", get(app_css))
        .route("/app.js", get(app_js))
        .route("/api/bootstrap", get(get_bootstrap))
        .route(
            "/api/provider",
            get(get_provider).post(save_provider).delete(reset_provider),
        )
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/:id", get(get_session).delete(delete_session))
        .route("/api/chat", post(run_chat))
        .route("/api/skills", get(list_skills).post(save_skill))
        .route("/api/skills/:slug", get(get_skill).delete(delete_skill))
        .route("/api/mcp", get(list_mcp).post(save_mcp))
        .route("/api/mcp/:name", axum::routing::delete(delete_mcp))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("OpenClaw shell listening at http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn app_css() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/css; charset=utf-8"),
        )],
        APP_CSS,
    )
}

async fn app_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/javascript; charset=utf-8"),
        )],
        APP_JS,
    )
}

async fn get_bootstrap(
    State(state): State<ShellState>,
) -> Result<Json<BootstrapResponse>, ApiError> {
    let config = load_config(&state)?;
    let model = config.model().unwrap_or("gpt-5.4-mini").to_string();
    let sync_status = tokio::task::spawn_blocking({
        let cwd = state.cwd.clone();
        move || {
            AppRuntime::load(&cwd)
                .map(|app| app.team_memory_sync_status())
                .map_err(|error| error.to_string())
        }
    })
    .await
    .map_err(internal_error)?
    .map_err(ApiError::internal)?;
    Ok(Json(BootstrapResponse {
        app_name: "OpenClaw",
        cwd: state.cwd.display().to_string(),
        settings_file: shell_settings_path(&state.cwd).display().to_string(),
        model: model.clone(),
        permission_mode: format!(
            "{:?}",
            config
                .permission_mode()
                .unwrap_or(opencowork_runtime::PermissionMode::WorkspaceWrite)
        ),
        provider: provider_view(&config, &model),
        team_memory_sync: serde_json::to_value(sync_status).map_err(internal_error)?,
        sessions: session_items(&state)?,
        skills: skill_views(&state),
        mcp_servers: mcp_views(&config),
    }))
}

async fn get_provider(
    State(state): State<ShellState>,
) -> Result<Json<ProviderSettingsView>, ApiError> {
    let config = load_config(&state)?;
    let model = config.model().unwrap_or("gpt-5.4-mini").to_string();
    Ok(Json(provider_view(&config, &model)))
}

async fn save_provider(
    State(state): State<ShellState>,
    Json(payload): Json<ProviderUpdateRequest>,
) -> Result<Json<ProviderSettingsView>, ApiError> {
    if payload.name.trim().is_empty() {
        return Err(ApiError::bad_request("provider name must not be empty"));
    }
    if payload.api_key_env.trim().is_empty() {
        return Err(ApiError::bad_request("apiKeyEnv must not be empty"));
    }
    if payload.base_url.trim().is_empty() {
        return Err(ApiError::bad_request("baseUrl must not be empty"));
    }

    let mut settings = read_settings(&state.cwd)?;
    let root = object_mut(&mut settings);
    if let Some(model) = payload
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        root.insert("model".to_string(), Value::String(model.to_string()));
    }

    let mut provider = Map::new();
    provider.insert(
        "kind".to_string(),
        Value::String("openai-compatible".to_string()),
    );
    provider.insert("name".to_string(), Value::String(payload.name));
    provider.insert("apiKeyEnv".to_string(), Value::String(payload.api_key_env));
    provider.insert("baseUrl".to_string(), Value::String(payload.base_url));
    if let Some(base_url_env) = payload
        .base_url_env
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        provider.insert(
            "baseUrlEnv".to_string(),
            Value::String(base_url_env.to_string()),
        );
    }
    if let Some(timeout_ms) = payload.timeout_ms {
        provider.insert("timeoutMs".to_string(), Value::Number(timeout_ms.into()));
    }
    root.insert("provider".to_string(), Value::Object(provider));
    write_settings(&state.cwd, &settings)?;

    let config = load_config(&state)?;
    let model = config.model().unwrap_or("gpt-5.4-mini").to_string();
    Ok(Json(provider_view(&config, &model)))
}

async fn reset_provider(
    State(state): State<ShellState>,
) -> Result<Json<ProviderSettingsView>, ApiError> {
    let mut settings = read_settings(&state.cwd)?;
    let root = object_mut(&mut settings);
    root.remove("provider");
    write_settings(&state.cwd, &settings)?;

    let config = load_config(&state)?;
    let model = config.model().unwrap_or("gpt-5.4-mini").to_string();
    Ok(Json(provider_view(&config, &model)))
}

async fn list_sessions(
    State(state): State<ShellState>,
) -> Result<Json<Vec<SessionListItem>>, ApiError> {
    Ok(Json(session_items(&state)?))
}

async fn get_session(
    State(state): State<ShellState>,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<Session>, ApiError> {
    let store = SessionStore::new(state.config_home.join("sessions"));
    let session = store.load(&session_id).map_err(internal_error)?;
    Ok(Json(session))
}

async fn delete_session(
    State(state): State<ShellState>,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<Vec<SessionListItem>>, ApiError> {
    let path = state
        .config_home
        .join("sessions")
        .join(format!("{session_id}.json"));
    if !path.exists() {
        return Err(ApiError::not_found(format!(
            "session `{session_id}` was not found"
        )));
    }
    fs::remove_file(path).map_err(internal_error)?;
    Ok(Json(session_items(&state)?))
}

async fn run_chat(
    State(state): State<ShellState>,
    Json(payload): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, ApiError> {
    if payload.input.trim().is_empty() {
        return Err(ApiError::bad_request("input must not be empty"));
    }

    let response = tokio::task::spawn_blocking(move || run_chat_blocking(state, payload))
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?
        .map_err(ApiError::internal)?;
    Ok(Json(response))
}

fn run_chat_blocking(state: ShellState, payload: ChatRequest) -> Result<ChatResponse, String> {
    let app = AppRuntime::load(&state.cwd).map_err(|error| error.to_string())?;
    let model = payload.model.unwrap_or_else(|| app.model().to_string());
    let store = SessionStore::new(state.config_home.join("sessions"));
    let session = match payload.session_id.as_deref() {
        Some(session_id) => store.load(session_id).map_err(|error| error.to_string())?,
        None => Session::new(),
    };

    let mut events = Vec::new();
    let mut sink = |event| events.push(event);
    let execution = app
        .run_turn(&model, session, &payload.input, Some(&mut sink))
        .map_err(|error| error.to_string())?;
    let descriptor = match payload.session_id.as_deref() {
        Some(session_id) => store
            .save_named(session_id, &execution.session)
            .map_err(|error| error.to_string())?,
        None => store
            .save(&execution.session)
            .map_err(|error| error.to_string())?,
    };

    Ok(ChatResponse {
        session_id: descriptor.id,
        session: execution.session,
        events,
        iterations: execution.summary.iterations,
        estimated_prompt_tokens: execution.prompt.estimated_tokens,
        compacted: execution.prompt.compacted,
    })
}

async fn list_skills(State(state): State<ShellState>) -> Result<Json<Vec<SkillView>>, ApiError> {
    Ok(Json(skill_views(&state)))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkillDetailView {
    #[serde(flatten)]
    skill: SkillView,
    content: String,
}

async fn get_skill(
    State(state): State<ShellState>,
    AxumPath(slug): AxumPath<String>,
) -> Result<Json<SkillDetailView>, ApiError> {
    Ok(Json(skill_detail(&state, &slug)?))
}

async fn save_skill(
    State(state): State<ShellState>,
    Json(payload): Json<SkillUpsertRequest>,
) -> Result<Json<Vec<SkillView>>, ApiError> {
    if payload.name.trim().is_empty() {
        return Err(ApiError::bad_request("skill name must not be empty"));
    }
    if payload.content.trim().is_empty() {
        return Err(ApiError::bad_request("skill content must not be empty"));
    }

    let slug = payload
        .slug
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| slugify(&payload.name));
    let skill_root = state.cwd.join(".opencowork").join("skills").join(slug);
    fs::create_dir_all(&skill_root).map_err(internal_error)?;
    fs::write(skill_root.join("SKILL.md"), render_skill_markdown(&payload))
        .map_err(internal_error)?;

    Ok(Json(skill_views(&state)))
}

async fn delete_skill(
    State(state): State<ShellState>,
    AxumPath(slug): AxumPath<String>,
) -> Result<Json<Vec<SkillView>>, ApiError> {
    validate_slug(&slug)?;
    let skill_root = state.cwd.join(".opencowork").join("skills").join(&slug);
    if !skill_root.exists() {
        return Err(ApiError::not_found(format!(
            "project skill `{slug}` was not found"
        )));
    }
    fs::remove_dir_all(&skill_root).map_err(internal_error)?;
    Ok(Json(skill_views(&state)))
}

async fn list_mcp(State(state): State<ShellState>) -> Result<Json<Vec<McpServerView>>, ApiError> {
    let config = load_config(&state)?;
    Ok(Json(mcp_views(&config)))
}

async fn save_mcp(
    State(state): State<ShellState>,
    Json(payload): Json<McpUpsertRequest>,
) -> Result<Json<Vec<McpServerView>>, ApiError> {
    if payload.name.trim().is_empty() {
        return Err(ApiError::bad_request("MCP server name must not be empty"));
    }
    let transport = parse_transport(&payload.transport)?;
    validate_mcp_payload(transport, &payload)?;

    let mut settings = read_settings(&state.cwd)?;
    let root = object_mut(&mut settings);
    let mcp_servers = root
        .entry("mcpServers".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let mcp_object = object_mut(mcp_servers);
    if let Some(original_name) = payload
        .original_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if original_name != payload.name {
            mcp_object.remove(original_name);
        }
    }

    let mut server = Map::new();
    server.insert(
        "type".to_string(),
        Value::String(
            match transport {
                McpTransport::Stdio => "stdio",
                McpTransport::Http => "http",
                McpTransport::Sse => "sse",
                McpTransport::Ws => "ws",
            }
            .to_string(),
        ),
    );
    if let Some(command) = payload
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        server.insert("command".to_string(), Value::String(command.to_string()));
    }
    if !payload.args.is_empty() {
        server.insert(
            "args".to_string(),
            Value::Array(payload.args.iter().cloned().map(Value::String).collect()),
        );
    }
    if let Some(endpoint) = payload
        .endpoint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        server.insert("url".to_string(), Value::String(endpoint.to_string()));
    }
    if let Some(timeout_ms) = payload.timeout_ms {
        server.insert("timeoutMs".to_string(), Value::Number(timeout_ms.into()));
    }
    if let Some(auth_value) = build_auth_value(&payload)? {
        server.insert("auth".to_string(), auth_value);
    }

    mcp_object.insert(payload.name, Value::Object(server));
    write_settings(&state.cwd, &settings)?;
    let config = load_config(&state)?;
    Ok(Json(mcp_views(&config)))
}

async fn delete_mcp(
    State(state): State<ShellState>,
    AxumPath(name): AxumPath<String>,
) -> Result<Json<Vec<McpServerView>>, ApiError> {
    let mut settings = read_settings(&state.cwd)?;
    let root = object_mut(&mut settings);
    let Some(mcp_servers) = root.get_mut("mcpServers") else {
        return Err(ApiError::not_found("no MCP servers are configured"));
    };
    let removed = object_mut(mcp_servers).remove(&name);
    if removed.is_none() {
        return Err(ApiError::not_found(format!(
            "MCP server `{name}` was not found"
        )));
    }
    write_settings(&state.cwd, &settings)?;
    let config = load_config(&state)?;
    Ok(Json(mcp_views(&config)))
}

fn provider_view(config: &opencowork_runtime::RuntimeConfig, model: &str) -> ProviderSettingsView {
    if let Some(provider) = config.provider() {
        return ProviderSettingsView {
            model: model.to_string(),
            name: provider.name().to_string(),
            api_key_env: provider.api_key_env().to_string(),
            base_url: provider.base_url().to_string(),
            base_url_env: provider.base_url_env().map(ToOwned::to_owned),
            timeout_ms: provider.timeout_ms(),
            persisted: true,
        };
    }
    let profile = default_openai_profile_for_model(model);
    let base_url = profile.resolved_base_url();
    ProviderSettingsView {
        model: model.to_string(),
        name: profile.provider_name,
        api_key_env: profile.api_key_env,
        base_url,
        base_url_env: profile.base_url_env,
        timeout_ms: profile.timeout_ms,
        persisted: false,
    }
}

fn session_items(state: &ShellState) -> Result<Vec<SessionListItem>, ApiError> {
    let store = SessionStore::new(state.config_home.join("sessions"));
    Ok(store
        .list()
        .map_err(internal_error)?
        .into_iter()
        .map(|descriptor| SessionListItem {
            id: descriptor.id,
            message_count: descriptor.message_count,
            updated_at_unix_ms: descriptor.updated_at_unix_ms,
        })
        .collect())
}

fn skill_views(state: &ShellState) -> Vec<SkillView> {
    SkillCatalog::discover(&skill_roots(&state.cwd, &state.config_home))
        .skills()
        .iter()
        .map(skill_view)
        .collect()
}

fn skill_detail(state: &ShellState, slug: &str) -> Result<SkillDetailView, ApiError> {
    validate_slug(slug)?;
    let catalog = SkillCatalog::discover(&skill_roots(&state.cwd, &state.config_home));
    let summary = catalog
        .skills()
        .iter()
        .find(|summary| skill_slug(summary).as_deref() == Some(slug))
        .ok_or_else(|| ApiError::not_found(format!("skill `{slug}` was not found")))?;
    let raw = fs::read_to_string(&summary.path).map_err(internal_error)?;
    Ok(SkillDetailView {
        skill: skill_view(summary),
        content: strip_skill_frontmatter(&raw),
    })
}

fn skill_view(summary: &SkillSummary) -> SkillView {
    SkillView {
        name: summary.name.clone(),
        description: summary.description.clone(),
        path: summary.path.display().to_string(),
        origin: match summary.origin {
            SkillOrigin::SkillsDir => "skills",
            SkillOrigin::LegacyCommandsDir => "commands",
        }
        .to_string(),
        when_to_use: summary.when_to_use.clone(),
        argument_hint: summary.argument_hint.clone(),
        allowed_tools: summary.allowed_tools.clone(),
        paths: summary.paths.clone(),
        execution_context: summary.execution_context.map(|value| match value {
            SkillExecutionContext::Current => "current".to_string(),
            SkillExecutionContext::Fork => "fork".to_string(),
        }),
        version: summary.version.clone(),
        agent: summary.agent.clone(),
        model: summary.model.clone(),
        effort: summary.effort.clone(),
    }
}

fn mcp_views(config: &opencowork_runtime::RuntimeConfig) -> Vec<McpServerView> {
    config
        .mcp_servers()
        .values()
        .map(|server| McpServerView {
            name: server.name.clone(),
            transport: match server.transport {
                McpTransport::Stdio => "stdio",
                McpTransport::Http => "http",
                McpTransport::Sse => "sse",
                McpTransport::Ws => "ws",
            }
            .to_string(),
            command: server.command.clone(),
            args: server.args.clone(),
            endpoint: server.endpoint.clone(),
            timeout_ms: server.timeout_ms,
            auth_type: server.auth.label().to_string(),
            token_env: match &server.auth {
                McpAuthConfig::BearerEnv { token_env } => Some(token_env.clone()),
                _ => None,
            },
            token_path: match &server.auth {
                McpAuthConfig::BearerFile { token_path } => Some(token_path.clone()),
                _ => None,
            },
        })
        .collect()
}

fn validate_slug(slug: &str) -> Result<(), ApiError> {
    let trimmed = slug.trim();
    if trimmed.is_empty()
        || trimmed.contains("..")
        || trimmed.contains('/')
        || trimmed.contains('\\')
    {
        return Err(ApiError::bad_request("invalid skill slug"));
    }
    Ok(())
}

fn skill_slug(summary: &SkillSummary) -> Option<String> {
    summary
        .path
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
}

fn strip_skill_frontmatter(raw: &str) -> String {
    let normalized = raw.replace("\r\n", "\n");
    if let Some(rest) = normalized.strip_prefix("---\n") {
        if let Some(index) = rest.find("\n---\n") {
            return rest[index + 5..].trim().to_string();
        }
    }
    normalized.trim().to_string()
}

fn load_config(state: &ShellState) -> Result<opencowork_runtime::RuntimeConfig, ApiError> {
    ConfigLoader::default_for(&state.cwd)
        .load()
        .map_err(internal_error)
}

fn shell_settings_path(cwd: &Path) -> PathBuf {
    cwd.join(".opencowork").join("settings.json")
}

fn read_settings(cwd: &Path) -> Result<Value, ApiError> {
    let path = shell_settings_path(cwd);
    match fs::read_to_string(path) {
        Ok(contents) => {
            let parsed = serde_json::from_str::<Value>(&contents).map_err(internal_error)?;
            Ok(if parsed.is_object() {
                parsed
            } else {
                Value::Object(Map::new())
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Map::new())),
        Err(error) => Err(internal_error(error)),
    }
}

fn write_settings(cwd: &Path, value: &Value) -> Result<(), ApiError> {
    let path = shell_settings_path(cwd);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(internal_error)?;
    }
    let content = serde_json::to_string_pretty(value).map_err(internal_error)?;
    fs::write(path, content).map_err(internal_error)
}

fn object_mut(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().expect("object")
}

fn parse_transport(value: &str) -> Result<McpTransport, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "stdio" => Ok(McpTransport::Stdio),
        "http" => Ok(McpTransport::Http),
        "sse" => Ok(McpTransport::Sse),
        "ws" => Ok(McpTransport::Ws),
        _ => Err(ApiError::bad_request("unsupported MCP transport")),
    }
}

fn validate_mcp_payload(
    transport: McpTransport,
    payload: &McpUpsertRequest,
) -> Result<(), ApiError> {
    match transport {
        McpTransport::Stdio => {
            if payload
                .command
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                return Err(ApiError::bad_request("stdio MCP servers require a command"));
            }
        }
        McpTransport::Http | McpTransport::Sse | McpTransport::Ws => {
            if payload
                .endpoint
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                return Err(ApiError::bad_request(
                    "network MCP servers require an endpoint URL",
                ));
            }
        }
    }
    Ok(())
}

fn build_auth_value(payload: &McpUpsertRequest) -> Result<Option<Value>, ApiError> {
    match payload
        .auth_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        None | Some("none") => Ok(None),
        Some("bearer-env") => {
            let token_env = payload
                .token_env
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ApiError::bad_request("bearer-env auth requires tokenEnv"))?;
            Ok(Some(serde_json::json!({
                "type": "bearer-env",
                "tokenEnv": token_env,
            })))
        }
        Some("bearer-file") => {
            let token_path = payload
                .token_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ApiError::bad_request("bearer-file auth requires tokenPath"))?;
            Ok(Some(serde_json::json!({
                "type": "bearer-file",
                "tokenPath": token_path,
            })))
        }
        Some(other) => Err(ApiError::bad_request(format!(
            "unsupported MCP auth type `{other}`"
        ))),
    }
}

fn render_skill_markdown(payload: &SkillUpsertRequest) -> String {
    let mut lines = vec!["---".to_string()];
    lines.push(format!("name: {}", quote_yaml(&payload.name)));
    if let Some(description) = payload
        .description
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("description: {}", quote_yaml(description)));
    }
    if let Some(when_to_use) = payload
        .when_to_use
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("when_to_use: {}", quote_yaml(when_to_use)));
    }
    if let Some(argument_hint) = payload
        .argument_hint
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("argument_hint: {}", quote_yaml(argument_hint)));
    }
    if !payload.allowed_tools.is_empty() {
        lines.push("allowed_tools:".to_string());
        for tool in &payload.allowed_tools {
            lines.push(format!("  - {}", quote_yaml(tool)));
        }
    }
    if !payload.paths.is_empty() {
        lines.push("paths:".to_string());
        for path in &payload.paths {
            lines.push(format!("  - {}", quote_yaml(path)));
        }
    }
    if let Some(execution_context) = payload
        .execution_context
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("context: {}", quote_yaml(execution_context)));
    }
    if let Some(version) = payload
        .version
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("version: {}", quote_yaml(version)));
    }
    if let Some(agent) = payload
        .agent
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("agent: {}", quote_yaml(agent)));
    }
    if let Some(model) = payload
        .model
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("model: {}", quote_yaml(model)));
    }
    if let Some(effort) = payload
        .effort
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("effort: {}", quote_yaml(effort)));
    }
    lines.push("---".to_string());
    lines.push(String::new());
    lines.push(payload.content.trim().to_string());
    lines.join("\n")
}

fn quote_yaml(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn slugify(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "custom-skill".to_string()
    } else {
        slug
    }
}

fn skill_roots(cwd: &Path, config_home: &Path) -> Vec<PathBuf> {
    vec![
        cwd.join(".claude").join("skills"),
        cwd.join(".claude").join("commands"),
        cwd.join(".opencowork").join("skills"),
        cwd.join(".opencowork").join("commands"),
        cwd.join(".codex").join("skills"),
        cwd.join(".codex").join("commands"),
        config_home.join("skills"),
        config_home.join("commands"),
    ]
}

fn internal_error(error: impl std::fmt::Display) -> ApiError {
    ApiError::internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        quote_yaml, render_skill_markdown, shell_settings_path, slugify, SkillUpsertRequest,
    };
    use std::path::Path;

    #[test]
    fn skill_markdown_contains_frontmatter() {
        let markdown = render_skill_markdown(&SkillUpsertRequest {
            slug: None,
            name: "frontend-review".to_string(),
            description: Some("Review frontend shell polish".to_string()),
            when_to_use: Some("When working on the shell".to_string()),
            argument_hint: None,
            allowed_tools: vec!["read_file".to_string(), "edit_file".to_string()],
            paths: vec!["src/**".to_string()],
            execution_context: Some("current".to_string()),
            version: None,
            agent: None,
            model: None,
            effort: None,
            content: "Keep the shell bold.".to_string(),
        });
        assert!(markdown.contains("name: \"frontend-review\""));
        assert!(markdown.contains("allowed_tools:"));
        assert!(markdown.contains("Keep the shell bold."));
    }

    #[test]
    fn slugify_normalizes_text() {
        assert_eq!(slugify("Open Claw Shell"), "open-claw-shell");
        assert_eq!(slugify(""), "custom-skill");
    }

    #[test]
    fn settings_path_points_to_project_config() {
        let path = shell_settings_path(Path::new("d:/repo"));
        assert!(
            path.ends_with(".opencowork\\settings.json")
                || path.ends_with(".opencowork/settings.json")
        );
    }

    #[test]
    fn yaml_quotes_escape_double_quotes() {
        assert_eq!(quote_yaml("a\"b"), "\"a\\\"b\"");
    }
}

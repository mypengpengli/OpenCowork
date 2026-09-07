mod acp;
mod chat;

use crate::acp::{
    AcpCoordinator, AcpOverviewView, AcpSettingsUpdateRequest, AcpThreadCreateRequest,
};
use anyhow::Context;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use opencowork_api::default_openai_profile_for_model;
use opencowork_app::{AppEvent, AppRuntime};
use opencowork_commands::{handle_command, specs as slash_specs, SlashCommand};
use opencowork_mcp::{McpAuthConfig, McpTransport};
use opencowork_runtime::{
    default_config_home, project_memory_entrypoint, project_memory_root,
    project_session_memory_path, project_team_memory_entrypoint, project_team_memory_root,
    ConfigLoader, ConversationMessage, MessageRole, Session, SessionMemoryState, SessionStore,
};
use opencowork_skills::{SkillCatalog, SkillExecutionContext, SkillOrigin, SkillSummary};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const INDEX_HTML: &str = include_str!("../static/index.html");
const APP_CSS: &str = include_str!("../static/app.css");
const APP_JS: &str = include_str!("../static/app.js");
const CHAT_STREAM_JS: &str = include_str!("../static/chat-stream.mjs");

#[derive(Clone)]
struct ShellState {
    cwd: PathBuf,
    config_home: PathBuf,
    acp: AcpCoordinator,
    turns: chat::TurnRegistry,
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
    provider_profiles: Vec<ProviderProfileView>,
    active_provider_profile_id: Option<String>,
    team_memory_sync: serde_json::Value,
    sessions: Vec<SessionListItem>,
    skills: Vec<SkillView>,
    mcp_servers: Vec<McpServerView>,
    acp: AcpOverviewView,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemoryOverviewQuery {
    session_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryOverviewResponse {
    project_memory_root: String,
    team_memory_root: String,
    project_entrypoint_path: String,
    team_entrypoint_path: String,
    active_session_id: Option<String>,
    active_session_title: Option<String>,
    active_session_memory_path: Option<String>,
    active_session_memory_exists: bool,
    active_session_memory_state: Option<SessionMemoryStateView>,
    project_note_count: usize,
    team_note_count: usize,
    relevant_candidate_count: usize,
    documents: Vec<MemoryDocumentView>,
    checklist: Vec<MemoryParityItemView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionMemoryStateView {
    initialized: bool,
    last_triggered_message_count: usize,
    last_summarized_message_count: usize,
    tokens_at_last_extraction: usize,
    extraction_in_flight: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryDocumentView {
    scope: String,
    kind: String,
    title: String,
    description: String,
    relative_path: String,
    path: String,
    exists: bool,
    editable: bool,
    bytes: Option<u64>,
    updated_at_unix_ms: Option<u128>,
    active_session: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryParityItemView {
    status: String,
    title: String,
    detail: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemoryDocumentRequest {
    scope: String,
    relative_path: String,
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemoryDocumentSaveRequest {
    scope: String,
    relative_path: String,
    session_id: Option<String>,
    content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryDocumentContentView {
    document: MemoryDocumentView,
    content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SlashCommandSpecView {
    name: String,
    summary: String,
    argument_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolManifestEntryView {
    name: String,
    description: String,
    permission: String,
    source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderSettingsView {
    model: String,
    name: String,
    api_key: String,
    api_key_env: String,
    base_url: String,
    base_url_env: Option<String>,
    timeout_ms: u64,
    persisted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderProfileView {
    id: String,
    label: String,
    model: String,
    name: String,
    api_key: String,
    has_api_key: bool,
    api_key_env: String,
    base_url: String,
    base_url_env: Option<String>,
    timeout_ms: u64,
    active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderProfileRecord {
    id: String,
    label: String,
    model: String,
    name: String,
    #[serde(default)]
    api_key: String,
    api_key_env: String,
    base_url: String,
    base_url_env: Option<String>,
    timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionListItem {
    id: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    preview: Option<String>,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatRequest {
    #[serde(default)]
    turn_id: Option<String>,
    session_id: Option<String>,
    input: String,
    model: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatResponse {
    session_id: String,
    session: Session,
    events: Vec<AppEvent>,
    iterations: usize,
    estimated_prompt_tokens: usize,
    compacted: bool,
    status: String,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SlashExecuteRequest {
    input: String,
    session_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SlashExecuteResponse {
    title: String,
    output: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderUpdateRequest {
    model: Option<String>,
    name: String,
    api_key: Option<String>,
    api_key_env: Option<String>,
    base_url: String,
    base_url_env: Option<String>,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderProfileUpsertRequest {
    id: Option<String>,
    label: String,
    model: String,
    name: String,
    api_key: Option<String>,
    api_key_env: Option<String>,
    clear_api_key: Option<bool>,
    base_url: String,
    base_url_env: Option<String>,
    timeout_ms: Option<u64>,
    activate: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PermissionModeUpdateRequest {
    permission_mode: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PermissionModeView {
    value: String,
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
    if env::args().nth(1).as_deref() == Some("--chat-worker") {
        return tokio::task::spawn_blocking(chat::worker_main)
            .await?
            .map_err(anyhow::Error::msg);
    }
    let cwd = env::current_dir().context("failed to resolve current directory")?;
    let config_home = default_config_home();
    let acp = AcpCoordinator::new(cwd.clone(), config_home.clone());
    acp.spawn_background_worker();
    let port = env::var("OPENCOWORK_SHELL_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(33211);
    ensure_shell_defaults(&cwd).map_err(|error| anyhow::anyhow!(error.message))?;
    let state = ShellState {
        cwd,
        config_home,
        acp,
        turns: Default::default(),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/app.css", get(app_css))
        .route("/app.js", get(app_js))
        .route("/chat-stream.mjs", get(|| async {
            ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], CHAT_STREAM_JS)
        }))
        .route("/api/health", get(|| async { StatusCode::NO_CONTENT }))
        .route("/api/bootstrap", get(get_bootstrap))
        .route("/api/team-memory-sync", get(get_team_memory_sync))
        .route("/api/slash-specs", get(list_slash_specs))
        .route("/api/tool-manifest", get(get_tool_manifest))
        .route("/api/slash", post(run_slash_command))
        .route(
            "/api/permission",
            get(get_permission_mode).post(save_permission_mode),
        )
        .route(
            "/api/provider",
            get(get_provider).post(save_provider).delete(reset_provider),
        )
        .route(
            "/api/provider-profiles",
            get(list_provider_profiles).post(save_provider_profile),
        )
        .route(
            "/api/provider-profiles/:id/activate",
            post(activate_provider_profile),
        )
        .route(
            "/api/provider-profiles/:id",
            axum::routing::delete(delete_provider_profile),
        )
        .route("/api/sessions", get(list_sessions))
        .route("/api/sessions/:id", get(get_session).delete(delete_session))
        .route("/api/chat", post(chat::stream_turn))
        .route("/api/chat/:id/cancel", post(chat::cancel_turn))
        .route("/api/memory", get(get_memory_overview))
        .route("/api/memory/read", post(read_memory_document))
        .route("/api/memory/save", post(save_memory_document))
        .route("/api/memory/delete", post(delete_memory_document))
        .route("/api/skills", get(list_skills).post(save_skill))
        .route("/api/skills/:slug", get(get_skill).delete(delete_skill))
        .route("/api/mcp", get(list_mcp).post(save_mcp))
        .route("/api/mcp/:name", axum::routing::delete(delete_mcp))
        .route("/api/acp", get(get_acp).post(save_acp_settings))
        .route("/api/acp/threads", post(create_acp_thread))
        .route("/api/acp/threads/:id/reset", post(reset_acp_thread))
        .route(
            "/api/acp/threads/:id",
            axum::routing::delete(delete_acp_thread),
        )
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("OpenCowork shell listening at http://{addr}");
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
    // Disk scans must not block the async executor serving the window and health checks.
    tokio::task::spawn_blocking(move || build_bootstrap(&state))
        .await
        .map_err(internal_error)?
        .map(Json)
}

fn build_bootstrap(state: &ShellState) -> Result<BootstrapResponse, ApiError> {
    let config = load_config(state)?;
    let model = config.model().unwrap_or("gpt-5.4-mini").to_string();
    let settings = read_settings(&state.cwd)?;
    let acp = state.acp.overview(&settings).map_err(ApiError::internal)?;
    let (provider_profiles, active_provider_profile_id) =
        provider_profile_views(&settings, &provider_view(&config, &model));
    let sync_status = AppRuntime::current_team_memory_sync_status();
    Ok(BootstrapResponse {
        app_name: "OpenCowork",
        cwd: state.cwd.display().to_string(),
        settings_file: shell_settings_path(&state.cwd).display().to_string(),
        model: model.clone(),
        permission_mode: permission_mode_label(
            config
                .permission_mode()
                .unwrap_or(opencowork_runtime::PermissionMode::DangerFullAccess),
        )
        .to_string(),
        provider: provider_view(&config, &model),
        provider_profiles,
        active_provider_profile_id,
        team_memory_sync: serde_json::to_value(sync_status).map_err(internal_error)?,
        sessions: session_items(state)?,
        skills: skill_views(state),
        mcp_servers: mcp_views(&config),
        acp,
    })
}

async fn get_team_memory_sync(
    State(state): State<ShellState>,
) -> Result<Json<Value>, ApiError> {
    let status = tokio::task::spawn_blocking(move || {
        AppRuntime::initialize_team_memory_sync(&state.cwd)
    })
    .await
    .map_err(internal_error)?
    .map_err(ApiError::internal)?;
    Ok(Json(serde_json::to_value(status).map_err(internal_error)?))
}

async fn get_acp(State(state): State<ShellState>) -> Result<Json<AcpOverviewView>, ApiError> {
    let settings = read_settings(&state.cwd)?;
    let overview = state.acp.overview(&settings).map_err(ApiError::internal)?;
    Ok(Json(overview))
}

async fn save_acp_settings(
    State(state): State<ShellState>,
    Json(payload): Json<AcpSettingsUpdateRequest>,
) -> Result<Json<AcpOverviewView>, ApiError> {
    let mut settings = read_settings(&state.cwd)?;
    state
        .acp
        .apply_settings_update(&mut settings, &payload)
        .map_err(ApiError::bad_request)?;
    write_settings(&state.cwd, &settings)?;
    let overview = state.acp.overview(&settings).map_err(ApiError::internal)?;
    Ok(Json(overview))
}

async fn create_acp_thread(
    State(state): State<ShellState>,
    Json(payload): Json<AcpThreadCreateRequest>,
) -> Result<Json<AcpOverviewView>, ApiError> {
    let settings = read_settings(&state.cwd)?;
    state
        .acp
        .create_thread(&settings, &payload)
        .map_err(acp_action_error)?;
    let overview = state.acp.overview(&settings).map_err(ApiError::internal)?;
    Ok(Json(overview))
}

async fn reset_acp_thread(
    State(state): State<ShellState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<AcpOverviewView>, ApiError> {
    state.acp.reset_thread(&id).map_err(acp_action_error)?;
    let settings = read_settings(&state.cwd)?;
    let overview = state.acp.overview(&settings).map_err(ApiError::internal)?;
    Ok(Json(overview))
}

async fn delete_acp_thread(
    State(state): State<ShellState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<AcpOverviewView>, ApiError> {
    state.acp.delete_thread(&id).map_err(acp_action_error)?;
    let settings = read_settings(&state.cwd)?;
    let overview = state.acp.overview(&settings).map_err(ApiError::internal)?;
    Ok(Json(overview))
}

async fn get_provider(
    State(state): State<ShellState>,
) -> Result<Json<ProviderSettingsView>, ApiError> {
    let config = load_config(&state)?;
    let model = config.model().unwrap_or("gpt-5.4-mini").to_string();
    Ok(Json(provider_view(&config, &model)))
}

async fn list_slash_specs() -> Json<Vec<SlashCommandSpecView>> {
    Json(
        slash_specs()
            .iter()
            .map(|spec| SlashCommandSpecView {
                name: spec.name.to_string(),
                summary: spec.summary.to_string(),
                argument_hint: spec.argument_hint.map(ToOwned::to_owned),
            })
            .collect(),
    )
}

async fn get_tool_manifest(
    State(state): State<ShellState>,
) -> Result<Json<Vec<ToolManifestEntryView>>, ApiError> {
    let cwd = state.cwd.clone();
    let mut tools = tokio::task::spawn_blocking(move || {
        let app = AppRuntime::load(&cwd).map_err(|error| error.to_string())?;
        let mut items = app
            .tool_manifest()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(
                |(name, description, permission, _schema)| ToolManifestEntryView {
                    source: infer_tool_source(&name).to_string(),
                    permission: permission_mode_label(permission).to_string(),
                    name,
                    description,
                },
            )
            .collect::<Vec<_>>();
        items.sort_by(|left, right| left.name.cmp(&right.name));
        Ok::<_, String>(items)
    })
    .await
    .map_err(internal_error)?
    .map_err(ApiError::internal)?;
    tools.shrink_to_fit();
    Ok(Json(tools))
}

async fn run_slash_command(
    State(state): State<ShellState>,
    Json(payload): Json<SlashExecuteRequest>,
) -> Result<Json<SlashExecuteResponse>, ApiError> {
    let input = payload.input.trim().to_string();
    let Some(command) = SlashCommand::parse(&input) else {
        return Err(ApiError::bad_request("slash input must start with `/`"));
    };

    let cwd = state.cwd.clone();
    let config_home = state.config_home.clone();
    let session_id = payload.session_id.clone();
    let input_for_worker = input.clone();
    let response = tokio::task::spawn_blocking(move || {
        let app = AppRuntime::load(&cwd).map_err(|error| error.to_string())?;
        let store = SessionStore::new(config_home.join("sessions"));
        let session = session_id
            .as_deref()
            .and_then(|id| store.load(id).ok())
            .unwrap_or_else(Session::new);
        let title = format!(
            "/{}",
            input_for_worker
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("help")
        );
        let output =
            handle_command(&command, &session, app.permission_mode()).unwrap_or_else(|| {
                format!(
                    "Unknown slash command: {}\n\n{}",
                    input_for_worker,
                    opencowork_commands::render_help()
                )
            });
        Ok::<_, String>(SlashExecuteResponse { title, output })
    })
    .await
    .map_err(internal_error)?
    .map_err(ApiError::internal)?;

    Ok(Json(response))
}

async fn get_permission_mode(
    State(state): State<ShellState>,
) -> Result<Json<PermissionModeView>, ApiError> {
    let config = load_config(&state)?;
    Ok(Json(PermissionModeView {
        value: permission_mode_label(
            config
                .permission_mode()
                .unwrap_or(opencowork_runtime::PermissionMode::DangerFullAccess),
        )
        .to_string(),
    }))
}

async fn save_permission_mode(
    State(state): State<ShellState>,
    Json(payload): Json<PermissionModeUpdateRequest>,
) -> Result<Json<PermissionModeView>, ApiError> {
    let label = normalize_permission_mode_label(&payload.permission_mode)?;
    let mut settings = read_settings(&state.cwd)?;
    let root = object_mut(&mut settings);
    root.insert(
        "permissionMode".to_string(),
        Value::String(label.to_string()),
    );
    write_settings(&state.cwd, &settings)?;
    Ok(Json(PermissionModeView {
        value: label.to_string(),
    }))
}

async fn save_provider(
    State(state): State<ShellState>,
    Json(payload): Json<ProviderUpdateRequest>,
) -> Result<Json<ProviderSettingsView>, ApiError> {
    if payload.name.trim().is_empty() {
        return Err(ApiError::bad_request("provider name must not be empty"));
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

    let api_key_env = payload
        .api_key_env
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            let model = payload
                .model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("gpt-5.4-mini");
            default_openai_profile_for_model(model).api_key_env
        });

    let mut provider = Map::new();
    provider.insert(
        "kind".to_string(),
        Value::String("openai-compatible".to_string()),
    );
    provider.insert("name".to_string(), Value::String(payload.name));
    provider.insert("apiKeyEnv".to_string(), Value::String(api_key_env.clone()));
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
    {
        let shell = shell_mut(root);
        if let Some(api_key) = payload
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            shell.insert(
                "inlineProviderApiKey".to_string(),
                Value::String(api_key.to_string()),
            );
            shell.insert(
                "inlineProviderApiKeyEnv".to_string(),
                Value::String(api_key_env),
            );
        } else {
            shell.remove("inlineProviderApiKey");
            shell.remove("inlineProviderApiKeyEnv");
        }
    }
    write_settings(&state.cwd, &settings)?;

    let config = load_config(&state)?;
    let model = config.model().unwrap_or("gpt-5.4-mini").to_string();
    Ok(Json(provider_view(&config, &model)))
}

async fn list_provider_profiles(
    State(state): State<ShellState>,
) -> Result<Json<Vec<ProviderProfileView>>, ApiError> {
    let config = load_config(&state)?;
    let model = config.model().unwrap_or("gpt-5.4-mini").to_string();
    let settings = read_settings(&state.cwd)?;
    let (profiles, _) = provider_profile_views(&settings, &provider_view(&config, &model));
    Ok(Json(profiles))
}

async fn save_provider_profile(
    State(state): State<ShellState>,
    Json(payload): Json<ProviderProfileUpsertRequest>,
) -> Result<Json<Vec<ProviderProfileView>>, ApiError> {
    if payload.label.trim().is_empty() {
        return Err(ApiError::bad_request(
            "provider profile label must not be empty",
        ));
    }
    if payload.model.trim().is_empty() {
        return Err(ApiError::bad_request(
            "provider profile model must not be empty",
        ));
    }
    if payload.name.trim().is_empty() {
        return Err(ApiError::bad_request(
            "provider profile name must not be empty",
        ));
    }
    if payload.base_url.trim().is_empty() {
        return Err(ApiError::bad_request(
            "provider profile baseUrl must not be empty",
        ));
    }

    let api_key_env = payload
        .api_key_env
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_openai_profile_for_model(payload.model.trim()).api_key_env);

    let mut settings = read_settings(&state.cwd)?;
    let (mut profiles, active_id) = read_provider_profiles(&settings);
    let requested_id = payload
        .id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let next_id =
        requested_id.unwrap_or_else(|| unique_provider_profile_id(&profiles, &payload.label));
    let mut record = ProviderProfileRecord {
        id: next_id.clone(),
        label: payload.label.trim().to_string(),
        model: payload.model.trim().to_string(),
        name: payload.name.trim().to_string(),
        api_key: payload
            .api_key
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_string(),
        api_key_env,
        base_url: payload.base_url.trim().to_string(),
        base_url_env: payload
            .base_url_env
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        timeout_ms: payload.timeout_ms.unwrap_or(90_000),
    };

    if let Some(existing) = profiles.iter_mut().find(|profile| profile.id == next_id) {
        if payload.clear_api_key.unwrap_or(false) {
            record.api_key.clear();
        } else if record.api_key.trim().is_empty() {
            record.api_key = existing.api_key.clone();
        }
        *existing = record.clone();
    } else {
        if payload.clear_api_key.unwrap_or(false) {
            record.api_key.clear();
        }
        profiles.push(record.clone());
    }

    let next_active = if payload.activate || profiles.len() == 1 {
        Some(next_id.clone())
    } else {
        active_id
    };
    write_provider_profiles(&state.cwd, &mut settings, &profiles, next_active.as_deref())?;

    let config = load_config(&state)?;
    let model = config.model().unwrap_or("gpt-5.4-mini").to_string();
    let (views, _) = provider_profile_views(&settings, &provider_view(&config, &model));
    Ok(Json(views))
}

async fn activate_provider_profile(
    State(state): State<ShellState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Vec<ProviderProfileView>>, ApiError> {
    let mut settings = read_settings(&state.cwd)?;
    let (profiles, _) = read_provider_profiles(&settings);
    if !profiles.iter().any(|profile| profile.id == id) {
        return Err(ApiError::not_found(format!(
            "provider profile `{id}` was not found"
        )));
    }
    write_provider_profiles(&state.cwd, &mut settings, &profiles, Some(&id))?;
    let config = load_config(&state)?;
    let model = config.model().unwrap_or("gpt-5.4-mini").to_string();
    let (views, _) = provider_profile_views(&settings, &provider_view(&config, &model));
    Ok(Json(views))
}

async fn delete_provider_profile(
    State(state): State<ShellState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Vec<ProviderProfileView>>, ApiError> {
    let mut settings = read_settings(&state.cwd)?;
    let (mut profiles, active_id) = read_provider_profiles(&settings);
    let previous_len = profiles.len();
    profiles.retain(|profile| profile.id != id);
    if profiles.len() == previous_len {
        return Err(ApiError::not_found(format!(
            "provider profile `{id}` was not found"
        )));
    }
    let next_active = if active_id.as_deref() == Some(id.as_str()) {
        profiles.first().map(|profile| profile.id.as_str())
    } else {
        active_id.as_deref()
    };
    write_provider_profiles(&state.cwd, &mut settings, &profiles, next_active)?;
    let config = load_config(&state)?;
    let model = config.model().unwrap_or("gpt-5.4-mini").to_string();
    let (views, _) = provider_profile_views(&settings, &provider_view(&config, &model));
    Ok(Json(views))
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
    if state.turns.lock().map_err(internal_error)?.values().any(|turn| turn.session_id == session_id) {
        return Err(ApiError { status: StatusCode::CONFLICT, message: "Stop the running turn before deleting this session.".into() });
    }
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

async fn get_memory_overview(
    State(state): State<ShellState>,
    Query(query): Query<MemoryOverviewQuery>,
) -> Result<Json<MemoryOverviewResponse>, ApiError> {
    Ok(Json(build_memory_overview(
        &state,
        query.session_id.as_deref(),
    )?))
}

async fn read_memory_document(
    State(state): State<ShellState>,
    Json(payload): Json<MemoryDocumentRequest>,
) -> Result<Json<MemoryDocumentContentView>, ApiError> {
    let document = resolve_memory_document_view(
        &state,
        payload.scope.as_str(),
        payload.relative_path.as_str(),
        payload.session_id.as_deref(),
    )?;
    let path = PathBuf::from(&document.path);
    let content = if path.exists() {
        fs::read_to_string(&path).map_err(internal_error)?
    } else {
        default_memory_document_content(&document)
    };
    Ok(Json(MemoryDocumentContentView { document, content }))
}

async fn save_memory_document(
    State(state): State<ShellState>,
    Json(payload): Json<MemoryDocumentSaveRequest>,
) -> Result<Json<MemoryDocumentContentView>, ApiError> {
    let document = resolve_memory_document_view(
        &state,
        payload.scope.as_str(),
        payload.relative_path.as_str(),
        payload.session_id.as_deref(),
    )?;
    let path = PathBuf::from(&document.path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(internal_error)?;
    }
    let content = payload.content.replace("\r\n", "\n");
    fs::write(&path, &content).map_err(internal_error)?;
    sync_session_memory_if_needed(&state, &document, Some(content.clone()))?;
    let refreshed = resolve_memory_document_view(
        &state,
        payload.scope.as_str(),
        payload.relative_path.as_str(),
        payload.session_id.as_deref(),
    )?;
    Ok(Json(MemoryDocumentContentView {
        document: refreshed,
        content,
    }))
}

async fn delete_memory_document(
    State(state): State<ShellState>,
    Json(payload): Json<MemoryDocumentRequest>,
) -> Result<Json<MemoryOverviewResponse>, ApiError> {
    let document = resolve_memory_document_view(
        &state,
        payload.scope.as_str(),
        payload.relative_path.as_str(),
        payload.session_id.as_deref(),
    )?;
    let path = PathBuf::from(&document.path);
    if !path.exists() {
        return Err(ApiError::not_found(format!(
            "memory document `{}` was not found",
            document.relative_path
        )));
    }
    fs::remove_file(&path).map_err(internal_error)?;
    sync_session_memory_if_needed(&state, &document, None)?;
    Ok(Json(build_memory_overview(
        &state,
        payload.session_id.as_deref(),
    )?))
}

fn run_chat_blocking(
    cwd: &Path,
    payload: ChatRequest,
    session: Session,
    on_event: &mut dyn FnMut(AppEvent),
) -> Result<ChatResponse, String> {
    apply_shell_api_key_override(cwd)?;
    let app = AppRuntime::load(cwd).map_err(|error| error.to_string())?;
    let model = payload.model.unwrap_or_else(|| app.model().to_string());

    let mut events = Vec::new();
    let mut sink = |event: AppEvent| {
        on_event(event.clone());
        events.push(event);
    };
    let execution = app
        .run_turn(&model, session, &payload.input, Some(&mut sink))
        .map_err(|error| error.to_string())?;
    Ok(ChatResponse {
        session_id: payload.session_id.ok_or("worker session ID is missing")?,
        session: execution.session,
        events,
        iterations: execution.summary.iterations,
        estimated_prompt_tokens: execution.prompt.estimated_tokens,
        compacted: execution.prompt.compacted,
        status: "completed".to_string(),
        error: None,
    })
}

fn apply_shell_api_key_override(cwd: &Path) -> Result<(), String> {
    let settings = read_settings(cwd).map_err(|error| error.message)?;
    let (profiles, active_id) = read_provider_profiles(&settings);
    let active_profile = active_id
        .as_deref()
        .and_then(|id| profiles.iter().find(|profile| profile.id == id))
        .or_else(|| profiles.first());

    if let Some(profile) = active_profile {
        // SAFETY: this local shell config bridges a stored API key into the env-var based runtime.
        unsafe {
            if !profile.api_key.trim().is_empty() {
                env::set_var(&profile.api_key_env, &profile.api_key);
            }
        }
        return Ok(());
    }

    let shell = settings.get("shell").and_then(Value::as_object);
    let inline_api_key = shell
        .and_then(|object| object.get("inlineProviderApiKey"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let inline_api_key_env = shell
        .and_then(|object| object.get("inlineProviderApiKeyEnv"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let (Some(api_key), Some(api_key_env)) = (inline_api_key, inline_api_key_env) {
        unsafe {
            env::set_var(api_key_env, api_key);
        }
    }
    Ok(())
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
            api_key: String::new(),
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
        api_key: String::new(),
        api_key_env: profile.api_key_env,
        base_url,
        base_url_env: profile.base_url_env,
        timeout_ms: profile.timeout_ms,
        persisted: false,
    }
}

fn permission_mode_label(mode: opencowork_runtime::PermissionMode) -> &'static str {
    match mode {
        opencowork_runtime::PermissionMode::ReadOnly => "read-only",
        opencowork_runtime::PermissionMode::WorkspaceWrite => "workspace-write",
        opencowork_runtime::PermissionMode::DangerFullAccess => "danger-full-access",
    }
}

fn infer_tool_source(name: &str) -> &'static str {
    if name.eq_ignore_ascii_case("skill") {
        "skill"
    } else if name.eq_ignore_ascii_case("toolsearch") {
        "discovery"
    } else if name.starts_with("mcp__") {
        "mcp"
    } else if name.starts_with("plugin__") {
        "plugin"
    } else {
        "builtin"
    }
}

fn normalize_permission_mode_label(value: &str) -> Result<&'static str, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "read-only" | "readonly" | "default" | "plan" => Ok("read-only"),
        "workspace-write" | "workspace" | "workspacewrite" | "ask" => Ok("workspace-write"),
        "danger-full-access" | "danger" | "dangerfullaccess" | "allow-all" | "allowall" => {
            Ok("danger-full-access")
        }
        other => Err(ApiError::bad_request(format!(
            "unsupported permission mode `{other}`"
        ))),
    }
}

fn default_provider_profile_record(
    config: &opencowork_runtime::RuntimeConfig,
) -> ProviderProfileRecord {
    let model = config.model().unwrap_or("gpt-5.4-mini").to_string();
    let provider = provider_view(config, &model);
    ProviderProfileRecord {
        id: "default-provider".to_string(),
        label: "默认 API".to_string(),
        model: provider.model,
        name: provider.name,
        api_key: provider.api_key,
        api_key_env: provider.api_key_env,
        base_url: provider.base_url,
        base_url_env: provider.base_url_env,
        timeout_ms: provider.timeout_ms,
    }
}

fn read_provider_profiles(settings: &Value) -> (Vec<ProviderProfileRecord>, Option<String>) {
    let shell = settings.get("shell").and_then(Value::as_object);
    let profiles = shell
        .and_then(|object| object.get("providerProfiles"))
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    serde_json::from_value::<ProviderProfileRecord>(entry.clone()).ok()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let active_id = shell
        .and_then(|object| object.get("activeProviderProfileId"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    (profiles, active_id)
}

fn provider_profile_views(
    settings: &Value,
    fallback: &ProviderSettingsView,
) -> (Vec<ProviderProfileView>, Option<String>) {
    let (mut profiles, mut active_id) = read_provider_profiles(settings);
    if profiles.is_empty() {
        profiles.push(ProviderProfileRecord {
            id: "default-provider".to_string(),
            label: "默认 API".to_string(),
            model: fallback.model.clone(),
            name: fallback.name.clone(),
            api_key: fallback.api_key.clone(),
            api_key_env: fallback.api_key_env.clone(),
            base_url: fallback.base_url.clone(),
            base_url_env: fallback.base_url_env.clone(),
            timeout_ms: fallback.timeout_ms,
        });
        active_id = Some("default-provider".to_string());
    }
    if active_id.is_none() {
        active_id = profiles.first().map(|profile| profile.id.clone());
    }
    let active_ref = active_id.as_deref();
    (
        profiles
            .into_iter()
            .map(|profile| ProviderProfileView {
                active: active_ref == Some(profile.id.as_str()),
                id: profile.id,
                label: profile.label,
                model: profile.model,
                name: profile.name,
                api_key: String::new(),
                has_api_key: !profile.api_key.trim().is_empty(),
                api_key_env: profile.api_key_env,
                base_url: profile.base_url,
                base_url_env: profile.base_url_env,
                timeout_ms: profile.timeout_ms,
            })
            .collect(),
        active_id,
    )
}

fn unique_provider_profile_id(profiles: &[ProviderProfileRecord], label: &str) -> String {
    let base = slugify(label);
    let mut candidate = base.clone();
    let mut index = 2usize;
    while profiles.iter().any(|profile| profile.id == candidate) {
        candidate = format!("{base}-{index}");
        index += 1;
    }
    candidate
}

fn apply_provider_profile(root: &mut Map<String, Value>, profile: &ProviderProfileRecord) {
    root.insert("model".to_string(), Value::String(profile.model.clone()));
    let mut provider = Map::new();
    provider.insert(
        "kind".to_string(),
        Value::String("openai-compatible".to_string()),
    );
    provider.insert("name".to_string(), Value::String(profile.name.clone()));
    provider.insert(
        "apiKeyEnv".to_string(),
        Value::String(profile.api_key_env.clone()),
    );
    provider.insert(
        "baseUrl".to_string(),
        Value::String(profile.base_url.clone()),
    );
    if let Some(base_url_env) = &profile.base_url_env {
        provider.insert(
            "baseUrlEnv".to_string(),
            Value::String(base_url_env.clone()),
        );
    }
    provider.insert(
        "timeoutMs".to_string(),
        Value::Number(profile.timeout_ms.into()),
    );
    root.insert("provider".to_string(), Value::Object(provider));
}

fn write_provider_profiles(
    cwd: &Path,
    settings: &mut Value,
    profiles: &[ProviderProfileRecord],
    active_id: Option<&str>,
) -> Result<(), ApiError> {
    let root = object_mut(settings);
    {
        let shell = shell_mut(root);
        shell.insert(
            "providerProfiles".to_string(),
            serde_json::to_value(profiles).map_err(internal_error)?,
        );
        match active_id {
            Some(value) => {
                shell.insert(
                    "activeProviderProfileId".to_string(),
                    Value::String(value.to_string()),
                );
            }
            None => {
                shell.remove("activeProviderProfileId");
            }
        }
    }

    if let Some(active_id) = active_id {
        if let Some(profile) = profiles.iter().find(|profile| profile.id == active_id) {
            apply_provider_profile(root, profile);
        }
    } else {
        root.remove("provider");
        root.remove("model");
    }

    write_settings(cwd, settings)
}

fn session_items(state: &ShellState) -> Result<Vec<SessionListItem>, ApiError> {
    let store = SessionStore::new(state.config_home.join("sessions"));
    store
        .map_sessions(|descriptor, session| {
            let title = derive_session_title(&session, &descriptor.id);
            let preview = derive_session_preview(&session)
                .filter(|value| !value.eq_ignore_ascii_case(&title));
            SessionListItem {
                id: descriptor.id,
                title,
                preview,
                message_count: descriptor.message_count,
                updated_at_unix_ms: descriptor.updated_at_unix_ms,
            }
        })
        .map_err(internal_error)
}

fn build_memory_overview(
    state: &ShellState,
    session_id: Option<&str>,
) -> Result<MemoryOverviewResponse, ApiError> {
    let project_root = project_memory_root(&state.config_home, &state.cwd);
    let team_root = project_team_memory_root(&state.config_home, &state.cwd);
    let project_entrypoint = project_memory_entrypoint(&state.config_home, &state.cwd);
    let team_entrypoint = project_team_memory_entrypoint(&state.config_home, &state.cwd);
    let session = load_memory_session(state, session_id)?;
    let active_session_id = session
        .as_ref()
        .and_then(|value| value.id().map(ToOwned::to_owned));
    let active_session_title = session
        .as_ref()
        .and_then(|value| value.id().map(|id| derive_session_title(value, id)));
    let active_session_memory_path = session
        .as_ref()
        .map(|value| project_session_memory_path(&state.config_home, &state.cwd, value))
        .map(|path| path.display().to_string());
    let active_session_memory_exists = active_session_memory_path
        .as_deref()
        .map(Path::new)
        .map(Path::exists)
        .unwrap_or(false);

    let mut documents = vec![
        memory_document_view(
            "project",
            "entrypoint",
            "Project MEMORY.md",
            "Persistent workspace memory entrypoint loaded on every turn.",
            "MEMORY.md",
            project_entrypoint.clone(),
            false,
        ),
        memory_document_view(
            "team",
            "entrypoint",
            "Team MEMORY.md",
            "Shared team memory entrypoint loaded beside project memory.",
            "MEMORY.md",
            team_entrypoint.clone(),
            false,
        ),
    ];

    let mut project_note_count = 0usize;
    documents.extend(scan_memory_documents(
        "project",
        "note",
        &project_root,
        Some(Path::new("team")),
        "Project memory note",
        &mut project_note_count,
    )?);

    let mut team_note_count = 0usize;
    documents.extend(scan_memory_documents(
        "team",
        "note",
        &team_root,
        None,
        "Team memory note",
        &mut team_note_count,
    )?);

    if let Some(value) = session.as_ref() {
        let session_path = project_session_memory_path(&state.config_home, &state.cwd, value);
        documents.push(memory_document_view(
            "session",
            "session",
            "Current session memory",
            "Background-maintained working notes for the active session.",
            "summary.md",
            session_path,
            true,
        ));
    }

    documents.sort_by(|left, right| {
        left.scope
            .cmp(&right.scope)
            .then(left.kind.cmp(&right.kind))
            .then(left.relative_path.cmp(&right.relative_path))
    });

    Ok(MemoryOverviewResponse {
        project_memory_root: project_root.display().to_string(),
        team_memory_root: team_root.display().to_string(),
        project_entrypoint_path: project_entrypoint.display().to_string(),
        team_entrypoint_path: team_entrypoint.display().to_string(),
        active_session_id,
        active_session_title,
        active_session_memory_path,
        active_session_memory_exists,
        active_session_memory_state: session
            .as_ref()
            .map(|value| SessionMemoryStateView::from(&value.session_memory_state)),
        project_note_count,
        team_note_count,
        relevant_candidate_count: project_note_count,
        documents,
        checklist: memory_parity_checklist(),
    })
}

fn load_memory_session(
    state: &ShellState,
    session_id: Option<&str>,
) -> Result<Option<Session>, ApiError> {
    let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let store = SessionStore::new(state.config_home.join("sessions"));
    let session = store.load(session_id).map_err(internal_error)?;
    Ok(Some(session))
}

fn scan_memory_documents(
    scope: &str,
    kind: &str,
    root: &Path,
    excluded_prefix: Option<&Path>,
    description: &str,
    note_count: &mut usize,
) -> Result<Vec<MemoryDocumentView>, ApiError> {
    let mut documents = Vec::new();
    if !root.exists() {
        return Ok(documents);
    }
    collect_memory_documents(
        scope,
        kind,
        root,
        root,
        excluded_prefix,
        description,
        note_count,
        &mut documents,
    )?;
    Ok(documents)
}

fn collect_memory_documents(
    scope: &str,
    kind: &str,
    root: &Path,
    current: &Path,
    excluded_prefix: Option<&Path>,
    description: &str,
    note_count: &mut usize,
    documents: &mut Vec<MemoryDocumentView>,
) -> Result<(), ApiError> {
    for entry in fs::read_dir(current).map_err(internal_error)? {
        let entry = entry.map_err(internal_error)?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(internal_error)?
            .to_path_buf();
        if let Some(prefix) = excluded_prefix {
            if relative.starts_with(prefix) {
                continue;
            }
        }
        if path.is_dir() {
            collect_memory_documents(
                scope,
                kind,
                root,
                &path,
                excluded_prefix,
                description,
                note_count,
                documents,
            )?;
            continue;
        }
        if !is_memory_document_path(&path) {
            continue;
        }
        let relative_path = relative.to_string_lossy().replace('\\', "/");
        if relative_path.eq_ignore_ascii_case("MEMORY.md") {
            continue;
        }
        *note_count += 1;
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("memory.md")
            .to_string();
        documents.push(memory_document_view(
            scope,
            kind,
            &title,
            description,
            &relative_path,
            path,
            false,
        ));
    }
    Ok(())
}

fn is_memory_document_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("md") | Some("markdown") | Some("txt")
    )
}

fn memory_document_view(
    scope: &str,
    kind: &str,
    title: &str,
    description: &str,
    relative_path: &str,
    path: PathBuf,
    active_session: bool,
) -> MemoryDocumentView {
    let (bytes, updated_at_unix_ms) = file_metadata_summary(&path);
    MemoryDocumentView {
        scope: scope.to_string(),
        kind: kind.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        relative_path: relative_path.to_string(),
        path: path.display().to_string(),
        exists: path.exists(),
        editable: true,
        bytes,
        updated_at_unix_ms,
        active_session,
    }
}

fn resolve_memory_document_view(
    state: &ShellState,
    scope: &str,
    relative_path: &str,
    session_id: Option<&str>,
) -> Result<MemoryDocumentView, ApiError> {
    let normalized_scope = scope.trim().to_ascii_lowercase();
    let normalized_relative = normalize_memory_relative_path(relative_path)?;
    match normalized_scope.as_str() {
        "project" => {
            let path =
                project_memory_root(&state.config_home, &state.cwd).join(&normalized_relative);
            let kind = if normalized_relative.eq_ignore_ascii_case("MEMORY.md") {
                "entrypoint"
            } else {
                "note"
            };
            Ok(memory_document_view(
                "project",
                kind,
                Path::new(&normalized_relative)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("MEMORY.md"),
                if kind == "entrypoint" {
                    "Persistent workspace memory entrypoint loaded on every turn."
                } else {
                    "Project memory note"
                },
                &normalized_relative,
                path,
                false,
            ))
        }
        "team" => {
            let path =
                project_team_memory_root(&state.config_home, &state.cwd).join(&normalized_relative);
            let kind = if normalized_relative.eq_ignore_ascii_case("MEMORY.md") {
                "entrypoint"
            } else {
                "note"
            };
            Ok(memory_document_view(
                "team",
                kind,
                Path::new(&normalized_relative)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("MEMORY.md"),
                if kind == "entrypoint" {
                    "Shared team memory entrypoint loaded beside project memory."
                } else {
                    "Team memory note"
                },
                &normalized_relative,
                path,
                false,
            ))
        }
        "session" => {
            let session = load_memory_session(state, session_id)?
                .ok_or_else(|| ApiError::bad_request("session memory requires a sessionId"))?;
            let path = project_session_memory_path(&state.config_home, &state.cwd, &session);
            Ok(memory_document_view(
                "session",
                "session",
                "Current session memory",
                "Background-maintained working notes for the active session.",
                "summary.md",
                path,
                true,
            ))
        }
        _ => Err(ApiError::bad_request("unsupported memory scope")),
    }
}

fn normalize_memory_relative_path(value: &str) -> Result<String, ApiError> {
    let candidate = value.trim().replace('\\', "/");
    if candidate.is_empty() {
        return Err(ApiError::bad_request("memory path must not be empty"));
    }
    let path = Path::new(&candidate);
    if path.is_absolute() {
        return Err(ApiError::bad_request("memory path must be relative"));
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(value) => parts.push(value.to_string_lossy().to_string()),
            std::path::Component::CurDir => {}
            _ => {
                return Err(ApiError::bad_request(
                    "memory path must stay inside its root",
                ))
            }
        }
    }
    let normalized = parts.join("/");
    if normalized.is_empty() {
        return Err(ApiError::bad_request("memory path must not be empty"));
    }
    Ok(normalized)
}

fn file_metadata_summary(path: &Path) -> (Option<u64>, Option<u128>) {
    let Ok(metadata) = fs::metadata(path) else {
        return (None, None);
    };
    let updated_at_unix_ms = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis());
    (Some(metadata.len()), updated_at_unix_ms)
}

fn default_memory_document_content(document: &MemoryDocumentView) -> String {
    match (document.scope.as_str(), document.kind.as_str()) {
        ("project", "entrypoint") => {
            "# Project memory\n\n- Capture durable workspace constraints, decisions, and warnings here.\n".to_string()
        }
        ("team", "entrypoint") => {
            "# Team memory\n\n- Capture shared team conventions and coordination notes here.\n".to_string()
        }
        ("session", _) => "# Session Title\n_\n\n# Current State\n_\n".to_string(),
        _ => format!("# {}\n\n", document.title),
    }
}

fn sync_session_memory_if_needed(
    state: &ShellState,
    document: &MemoryDocumentView,
    content: Option<String>,
) -> Result<(), ApiError> {
    if document.scope != "session" || !document.active_session {
        return Ok(());
    }
    let Some(session_id) = Path::new(&document.path)
        .parent()
        .and_then(|value| value.parent())
        .and_then(|value| value.file_name())
        .and_then(|value| value.to_str())
    else {
        return Ok(());
    };
    let store = SessionStore::new(state.config_home.join("sessions"));
    let mut session = store.load(session_id).map_err(internal_error)?;
    session.current_session_memory = content;
    store
        .save_named(session_id, &session)
        .map_err(internal_error)?;
    Ok(())
}

fn memory_parity_checklist() -> Vec<MemoryParityItemView> {
    vec![
        MemoryParityItemView {
            status: "done".to_string(),
            title: "Three-layer memory is wired".to_string(),
            detail: "Instruction memory, current session memory, and relevant memory recall are already active in prompt assembly.".to_string(),
        },
        MemoryParityItemView {
            status: "done".to_string(),
            title: "Memory management shell is added".to_string(),
            detail: "The shell can now inspect and edit project, team, and current session memory files.".to_string(),
        },
        MemoryParityItemView {
            status: "next".to_string(),
            title: "Split relevant recall into a dedicated side-query route".to_string(),
            detail: "Relevant-memory selection still rides the main provider path instead of a stricter side-query flow.".to_string(),
        },
        MemoryParityItemView {
            status: "next".to_string(),
            title: "Add surfaced-memory throttling".to_string(),
            detail: "Reference repos throttle how many recalled memories can surface across turns; OpenCoWork still needs that guard.".to_string(),
        },
        MemoryParityItemView {
            status: "next".to_string(),
            title: "Finish memory distillation and richer team sync handling".to_string(),
            detail: "Auto-memory distillation plus richer team-memory auth and conflict handling remain behind the reference behavior.".to_string(),
        },
    ]
}

impl From<&SessionMemoryState> for SessionMemoryStateView {
    fn from(value: &SessionMemoryState) -> Self {
        Self {
            initialized: value.initialized,
            last_triggered_message_count: value.last_triggered_message_count,
            last_summarized_message_count: value.last_summarized_message_count,
            tokens_at_last_extraction: value.tokens_at_last_extraction,
            extraction_in_flight: value.extraction_started_at_unix_ms.is_some(),
        }
    }
}

fn derive_session_title(session: &Session, fallback_id: &str) -> String {
    extract_session_memory_section_line(
        session.current_session_memory.as_deref(),
        "Session Title",
        72,
    )
    .or_else(|| first_message_text(session, MessageRole::User, 72))
    .or_else(|| first_message_text(session, MessageRole::Assistant, 72))
    .unwrap_or_else(|| fallback_id.to_string())
}

fn derive_session_preview(session: &Session) -> Option<String> {
    extract_session_memory_section_line(
        session.current_session_memory.as_deref(),
        "Current State",
        160,
    )
    .or_else(|| latest_message_text(session, MessageRole::Assistant, 160))
    .or_else(|| latest_message_text(session, MessageRole::User, 160))
}

fn extract_session_memory_section_line(
    session_memory: Option<&str>,
    heading: &str,
    max_chars: usize,
) -> Option<String> {
    let content = session_memory?;
    let target = format!("# {heading}");
    let mut in_target = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            if in_target {
                break;
            }
            in_target = trimmed.eq_ignore_ascii_case(&target);
            continue;
        }
        if !in_target || trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('_') && trimmed.ends_with('_') {
            continue;
        }
        return sanitize_session_line(trimmed, max_chars);
    }
    None
}

fn first_message_text(session: &Session, role: MessageRole, max_chars: usize) -> Option<String> {
    session
        .messages
        .iter()
        .find(|message| message.role == role)
        .and_then(ConversationMessage::first_text)
        .and_then(|text| sanitize_session_line(text, max_chars))
}

fn latest_message_text(session: &Session, role: MessageRole, max_chars: usize) -> Option<String> {
    session
        .messages
        .iter()
        .rev()
        .find(|message| message.role == role)
        .and_then(ConversationMessage::first_text)
        .and_then(|text| sanitize_session_line(text, max_chars))
}

fn sanitize_session_line(value: &str, max_chars: usize) -> Option<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    let mut truncated = String::new();
    for (index, ch) in normalized.chars().enumerate() {
        if index >= max_chars {
            truncated.push_str("...");
            return Some(truncated);
        }
        truncated.push(ch);
    }
    Some(truncated)
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

fn shell_mut(root: &mut Map<String, Value>) -> &mut Map<String, Value> {
    let entry = root
        .entry("shell".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    object_mut(entry)
}

fn ensure_shell_defaults(cwd: &Path) -> Result<(), ApiError> {
    let config = ConfigLoader::default_for(cwd)
        .load()
        .map_err(internal_error)?;
    let default_profile = default_provider_profile_record(&config);
    let mut settings = read_settings(cwd)?;
    let (mut profiles, mut active_id) = read_provider_profiles(&settings);
    let mut changed = false;

    {
        let root = object_mut(&mut settings);
        if !root.contains_key("permissionMode") {
            root.insert(
                "permissionMode".to_string(),
                Value::String("danger-full-access".to_string()),
            );
            changed = true;
        }
    }

    if profiles.is_empty() {
        profiles.push(default_profile.clone());
        active_id = Some(default_profile.id.clone());
        changed = true;
    } else if active_id.is_none() {
        active_id = profiles.first().map(|profile| profile.id.clone());
        changed = true;
    }

    for profile in &mut profiles {
        if profile.api_key.trim().is_empty() && looks_like_inline_api_key(&profile.api_key_env) {
            profile.api_key = profile.api_key_env.trim().to_string();
            profile.api_key_env = default_openai_profile_for_model(&profile.model).api_key_env;
            changed = true;
        }
    }

    if changed {
        write_provider_profiles(cwd, &mut settings, &profiles, active_id.as_deref())?;
    }

    Ok(())
}

fn looks_like_inline_api_key(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with("sk-")
        || trimmed.starts_with("xai-")
        || trimmed.starts_with("rk-")
        || trimmed.starts_with("sess-")
    {
        return true;
    }
    let env_like = trimmed
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_');
    !env_like && trimmed.len() >= 24
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

fn acp_action_error(message: String) -> ApiError {
    if message.contains("was not found") {
        ApiError::not_found(message)
    } else {
        ApiError::bad_request(message)
    }
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

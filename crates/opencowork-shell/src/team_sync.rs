use super::*;

pub(super) async fn settings(State(state): State<ShellState>) -> Result<Json<Value>, ApiError> {
    let config = load_config(&state)?;
    let value = config
        .merged()
        .get("teamMemorySync")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    Ok(Json(
        serde_json::json!({"enabled":value["enabled"].as_bool().unwrap_or(false),"endpoint":value["endpoint"],"repo":value["repo"],"tokenEnv":value["tokenEnv"],"tokenFile":value["tokenFile"]}),
    ))
}
pub(super) async fn save(
    State(state): State<ShellState>,
    Json(value): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let enabled = value["enabled"]
        .as_bool()
        .ok_or_else(|| ApiError::bad_request("enabled boolean required"))?;
    let mut sync = serde_json::Map::new();
    sync.insert("enabled".into(), Value::Bool(enabled));
    for key in ["endpoint", "repo", "tokenEnv", "tokenFile"] {
        let text = value[key].as_str().unwrap_or("").trim();
        if text.len() > 2048 || text.contains(['\r', '\n']) {
            return Err(ApiError::bad_request("Invalid sync setting"));
        }
        sync.insert(
            key.into(),
            if text.is_empty() {
                Value::Null
            } else {
                Value::String(text.into())
            },
        );
    }
    if enabled {
        let endpoint = sync["endpoint"].as_str().unwrap_or("");
        let url = reqwest::Url::parse(endpoint)
            .map_err(|_| ApiError::bad_request("请输入有效的 http(s) 同步地址。"))?;
        if !["http", "https"].contains(&url.scheme())
            || url.host_str().is_none()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || sync["repo"].is_null()
        {
            return Err(ApiError::bad_request(
                "请输入 http(s) 同步地址和仓库标识；认证请使用令牌字段。",
            ));
        }
        if let Some(file) = sync["tokenFile"].as_str() {
            if !Path::new(file).is_absolute() {
                return Err(ApiError::bad_request("令牌文件需要完整的绝对路径。"));
            }
        }
        if !sync["tokenEnv"].is_null() && !sync["tokenFile"].is_null() {
            return Err(ApiError::bad_request("令牌环境变量和令牌文件请选择一种。"));
        }
    }
    let mut settings = read_settings(&state.cwd)?;
    if !settings["teamMemorySync"].is_object() {
        settings["teamMemorySync"] = serde_json::json!({});
    }
    for (key, value) in sync {
        settings["teamMemorySync"][key] = value;
    }
    write_settings(&state.cwd, &settings)?;
    Ok(Json(serde_json::json!({"saved":true})))
}
pub(super) async fn conflicts(State(state): State<ShellState>) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let root = project_team_memory_root(&state.config_home, &state.cwd);
        opencowork_tools::team_memory_conflicts(&root)
            .map(|items| Json(serde_json::json!({"conflicts":items})))
            .map_err(ApiError::internal)
    })
    .await
    .map_err(internal_error)?
}
pub(super) async fn resolve(
    State(state): State<ShellState>,
    Json(value): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let root = project_team_memory_root(&state.config_home, &state.cwd);
        opencowork_tools::resolve_team_memory_conflict(
            &root,
            value["id"].as_str().unwrap_or(""),
            value["choice"].as_str().unwrap_or(""),
            value["remoteHash"].as_str().unwrap_or(""),
            value["localHash"].as_str().unwrap_or(""),
        )
        .map_err(ApiError::bad_request)?;
        Ok(Json(serde_json::json!({"resolved":true})))
    })
    .await
    .map_err(internal_error)?
}

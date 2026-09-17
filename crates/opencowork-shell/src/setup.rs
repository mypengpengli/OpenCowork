use super::*;

// Local inspection only: never connect a model or MCP server on page load.
pub(super) async fn status(State(state): State<ShellState>) -> Result<Json<Value>, ApiError> {
    let config = load_config(&state)?;
    let model = config.model().unwrap_or("gpt-5.4-mini");
    let provider = provider_view(&config, model);
    let settings = read_settings(&state.cwd)?;
    let (profiles, active) = read_provider_profiles(&settings);
    let inline = active
        .as_deref()
        .and_then(|id| profiles.iter().find(|p| p.id == id))
        .or_else(|| profiles.first())
        .is_some_and(|p| !p.api_key.trim().is_empty());
    let legacy = profiles.is_empty()
        && settings
            .pointer("/shell/inlineProviderApiKey")
            .and_then(Value::as_str)
            .is_some_and(|v| !v.trim().is_empty());
    let environment = env::var(&provider.api_key_env).is_ok_and(|v| !v.trim().is_empty());
    let servers = mcp_views(&config);
    Ok(Json(
        serde_json::json!({"model":model,"provider":provider.name,"credentialPresent":inline || legacy || environment,"workspace":state.cwd.display().to_string(),"mcpCount":servers.len()}),
    ))
}

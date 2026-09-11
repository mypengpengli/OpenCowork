use super::*;
pub(super) async fn list(State(state): State<ShellState>) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let bytes = workspace::git(&state.cwd, &["worktree", "list", "--porcelain"])?;
        Ok(Json(
            serde_json::json!({"content":String::from_utf8_lossy(&bytes)}),
        ))
    })
    .await
    .map_err(internal_error)?
}
pub(super) async fn create(
    State(state): State<ShellState>,
    Json(v): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move||{
    let name=v["name"].as_str().unwrap_or("");if !workflows::valid(name)||name.starts_with('-'){return Err(ApiError::bad_request("Use a short alphanumeric task name"));}
    let git_root=workspace::git(&state.cwd,&["rev-parse","--show-toplevel"])?;
    let key=opencowork_tools::file_history::version(&git_root);let managed=state.config_home.join("worktrees").join(&key[..16]);
    fs::create_dir_all(&managed).map_err(internal_error)?;let managed=managed.canonicalize().map_err(internal_error)?;let target=managed.join(name);
    if target.exists(){return Err(ApiError::bad_request("Task worktree already exists"));}
    let target_text=target.to_string_lossy();
    let git_target=if let Some(unc)=target_text.strip_prefix(r"\\?\UNC\"){format!(r"\\{unc}")}else{target_text.strip_prefix(r"\\?\").unwrap_or(&target_text).to_owned()};
    let branch=format!("cowork/{name}");workspace::git(&state.cwd,&["worktree","add","-b",&branch,&git_target,"HEAD"])?;
    Ok(Json(serde_json::json!({"path":target,"branch":branch,"base":"HEAD","note":"Committed files form the isolated checkout; uncommitted changes remain in the original workspace."})))
}).await.map_err(internal_error)?
}

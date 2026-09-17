use super::{internal_error, workspace_manager, ApiError, ShellState};
use axum::{extract::Path, http::StatusCode, Extension as State, Json};
use opencowork_runtime::{ExclusiveLease, Session, SessionStore};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RenameRequest {
    title: String,
}

fn load(state: &ShellState, id: &str) -> Result<Session, ApiError> {
    super::workspace::validate_id(id)?;
    let session = SessionStore::new(state.config_home.join("sessions"))
        .load(id)
        .map_err(|e| ApiError::not_found(e.to_string()))?;
    if session.workspace.as_ref().is_some_and(|path| {
        !workspace_manager::same_directory(std::path::Path::new(path), &state.cwd)
    }) {
        return Err(ApiError::bad_request(
            "请先打开该会话所属工作区。 / Open the session's workspace first.",
        ));
    }
    Ok(session)
}

pub(super) async fn rename(
    State(state): State<ShellState>,
    Path(id): Path<String>,
    Json(input): Json<RenameRequest>,
) -> Result<Json<Session>, ApiError> {
    let title = input.title.trim();
    if title.is_empty() || title.chars().count() > 72 || title.chars().any(char::is_control) {
        return Err(ApiError::bad_request("会话名称需为 1–72 个字符，不能包含换行。 / Use 1–72 characters without control characters."));
    }
    super::workspace::validate_id(&id)?;
    let _lease = ExclusiveLease::acquire(
        &state
            .config_home
            .join("turn-locks")
            .join(format!("{id}.lock")),
    )
    .map_err(|_| ApiError {
        status: StatusCode::CONFLICT,
        message: "请先停止当前任务再重命名。 / Stop the running task before renaming.".into(),
    })?;
    let mut session = load(&state, &id)?;
    session.title = Some(title.to_owned());
    SessionStore::new(state.config_home.join("sessions"))
        .save_named(&id, &session)
        .map_err(internal_error)?;
    Ok(Json(session))
}

pub(super) async fn reveal(
    State(state): State<ShellState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let session = load(&state, &id)?;
    let path = session
        .workspace
        .map(std::path::PathBuf::from)
        .unwrap_or(state.cwd);
    if !path.is_dir() {
        return Err(ApiError::bad_request(
            "工作区文件夹不存在。 / Workspace folder no longer exists.",
        ));
    }
    #[cfg(target_os = "windows")]
    let program = "explorer.exe";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";
    let mut child = std::process::Command::new(program)
        .arg(path)
        .spawn()
        .map_err(internal_error)?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(StatusCode::NO_CONTENT)
}

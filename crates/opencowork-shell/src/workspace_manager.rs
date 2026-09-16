//! Resolve each request to its selected workspace. Never change process cwd:
//! concurrent tabs and already-running workers retain their own project context.
use super::{internal_error, AcpCoordinator, ApiError, ShellState};
use axum::extract::{Query, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use axum::{Extension, Json};
use opencowork_runtime::{write_json_atomic, ExclusiveLease};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct WorkspaceEntry {
    id: String,
    path: PathBuf,
}

#[derive(Clone)]
pub(super) struct WorkspaceManager {
    default: ShellState,
    opened: Arc<Mutex<HashMap<String, ShellState>>>,
}

pub(super) fn same_directory(a: &Path, b: &Path) -> bool {
    let a = a.canonicalize().unwrap_or_else(|_| a.to_owned());
    let b = b.canonicalize().unwrap_or_else(|_| b.to_owned());
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

fn directory(path: &str) -> Result<PathBuf, ApiError> {
    let path = Path::new(path.trim());
    if !path.is_absolute() || !path.is_dir() {
        return Err(ApiError::bad_request(
            "请选择存在的文件夹，或输入完整的绝对路径。",
        ));
    }
    let canonical = path.canonicalize().map_err(internal_error)?;
    let value = canonical.to_string_lossy();
    // Keep normal Windows paths in prompts, settings and the UI.
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        Ok(PathBuf::from(format!(r"\\{rest}")))
    } else if let Some(rest) = value.strip_prefix(r"\\?\") {
        Ok(PathBuf::from(rest))
    } else {
        Ok(canonical)
    }
}

impl WorkspaceManager {
    pub(super) fn new(default: ShellState) -> Result<Self, ApiError> {
        let manager = Self {
            default,
            opened: Default::default(),
        };
        let entry = manager.remember(&manager.default.cwd)?;
        manager
            .opened
            .lock()
            .map_err(internal_error)?
            .insert(entry.id, manager.default.clone());
        Ok(manager)
    }

    fn catalog_path(&self) -> PathBuf {
        self.default.config_home.join("workspaces.json")
    }

    fn entries(&self) -> Result<Vec<WorkspaceEntry>, ApiError> {
        let path = self.catalog_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        serde_json::from_slice(&std::fs::read(path).map_err(internal_error)?)
            .map_err(internal_error)
    }

    fn remember(&self, path: &Path) -> Result<WorkspaceEntry, ApiError> {
        let catalog = self.catalog_path();
        let _lease =
            ExclusiveLease::acquire(&catalog.with_extension("lock")).map_err(internal_error)?;
        let mut entries = self.entries()?;
        let entry = entries
            .iter()
            .find(|e| same_directory(&e.path, path))
            .cloned()
            .unwrap_or_else(|| WorkspaceEntry {
                id: uuid::Uuid::new_v4().to_string(),
                path: path.to_owned(),
            });
        entries.retain(|e| e.id != entry.id);
        entries.insert(0, entry.clone());
        write_json_atomic(&catalog, &entries).map_err(internal_error)?;
        Ok(entry)
    }

    fn resolve(&self, id: &str) -> Result<ShellState, ApiError> {
        if id.is_empty() {
            return Ok(self.default.clone());
        }
        let mut opened = self.opened.lock().map_err(internal_error)?;
        if let Some(state) = opened.get(id) {
            return Ok(state.clone());
        }
        let entry = self
            .entries()?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| ApiError::bad_request("工作区不存在，请重新打开文件夹。"))?;
        let cwd = directory(&entry.path.to_string_lossy())?;
        super::ensure_shell_defaults(&cwd)?;
        let acp = AcpCoordinator::new(cwd.clone(), self.default.config_home.clone());
        let state = ShellState {
            cwd,
            acp,
            ..self.default.clone()
        };
        state.acp.spawn_background_worker();
        super::schedules::start(state.clone());
        opened.insert(id.to_owned(), state.clone());
        Ok(state)
    }
}

pub(super) async fn resolve_request(
    State(manager): State<WorkspaceManager>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let id = request
        .headers()
        .get("x-opencowork-workspace")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    // The folder chooser remains usable after a recent project is deleted or
    // a stale workspace URL is opened. Its paths are explicitly validated.
    let id = if request.uri().path().starts_with("/api/workspaces") {
        String::new()
    } else {
        id
    };
    let resolver = manager.clone();
    let state = tokio::task::spawn_blocking(move || resolver.resolve(&id))
        .await
        .map_err(internal_error)??;
    request.extensions_mut().insert(state);
    request.extensions_mut().insert(manager);
    Ok(next.run(request).await)
}

pub(super) async fn list(
    Extension(manager): Extension<WorkspaceManager>,
) -> Result<Json<Vec<WorkspaceEntry>>, ApiError> {
    Ok(Json(manager.entries()?))
}

#[derive(Deserialize)]
pub(super) struct OpenRequest {
    path: String,
}

pub(super) async fn open(
    Extension(manager): Extension<WorkspaceManager>,
    Json(input): Json<OpenRequest>,
) -> Result<Json<WorkspaceEntry>, ApiError> {
    let path = directory(&input.path)?;
    // Validate readability/config before publishing this selection.
    std::fs::read_dir(&path).map_err(internal_error)?;
    super::ensure_shell_defaults(&path)?;
    Ok(Json(manager.remember(&path)?))
}

#[derive(Deserialize)]
pub(super) struct DirectoryQuery {
    path: Option<String>,
}

pub(super) async fn directories(
    Extension(state): Extension<ShellState>,
    Query(query): Query<DirectoryQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let path = directory(
        query
            .path
            .as_deref()
            .unwrap_or(&state.cwd.to_string_lossy()),
    )?;
    tokio::task::spawn_blocking(move || {
        let mut folders: Vec<_> = std::fs::read_dir(&path).map_err(internal_error)?
            .filter_map(Result::ok).filter(|e| e.path().is_dir())
            .take(1001).map(|e| e.path()).collect();
        folders.sort();
        let truncated = folders.len() > 1000; folders.truncate(1000);
        let drives: Vec<PathBuf> = if cfg!(windows) {
            ('A'..='Z').map(|c| PathBuf::from(format!("{c}:\\"))).filter(|p| p.is_dir()).collect()
        } else { vec![PathBuf::from("/")] };
        Ok(Json(serde_json::json!({"path":path,"parent":path.parent(),"folders":folders,"drives":drives,"truncated":truncated})))
    }).await.map_err(internal_error)?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_requires_an_existing_absolute_directory() {
        assert!(directory("relative/folder").is_err());
        assert!(directory("\0").is_err());
        let root = std::env::temp_dir().join(format!("workspace-select-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("child")).unwrap();
        std::fs::write(root.join("file.txt"), "fixture").unwrap();
        assert!(directory(&root.join("file.txt").to_string_lossy()).is_err());
        assert!(same_directory(
            &directory(&root.join("child/..").to_string_lossy()).unwrap(),
            &root
        ));
        std::fs::remove_dir_all(root).unwrap();
    }
}

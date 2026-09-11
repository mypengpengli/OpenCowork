use super::*;
use std::io::Read;
use std::process::{Command, Stdio};

const FILE_LIMIT: u64 = 512 * 1024;
const REFERENCE_CHARS: usize = 24_000;

pub(super) async fn computer_screenshot(
    State(state): State<ShellState>,
    AxumPath(name): AxumPath<String>,
) -> Result<Response, ApiError> {
    if !name.ends_with(".jpg")
        || name.len() > 100
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
    {
        return Err(ApiError::bad_request("Invalid screenshot name"));
    }
    tokio::task::spawn_blocking(move || {
        let root = state
            .config_home
            .join("screenshots")
            .canonicalize()
            .map_err(internal_error)?;
        let path = root.join(name).canonicalize().map_err(internal_error)?;
        if !path.starts_with(&root)
            || !path.is_file()
            || fs::metadata(&path).map_err(internal_error)?.len() > 8 * 1024 * 1024
        {
            return Err(ApiError::bad_request("Screenshot unavailable"));
        }
        let bytes = fs::read(path).map_err(internal_error)?;
        Ok((
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "no-store"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            bytes,
        )
            .into_response())
    })
    .await
    .map_err(internal_error)?
}

pub(super) fn context_reason(name: &str) -> &'static str {
    if name.contains("relevant-memory:") {
        "按当前问题和工作区相关性召回；每轮最多 3 条，同条记忆间隔至少 6 条消息"
    } else if name.contains("conditional-skill:") {
        "技能的路径条件匹配当前操作文件"
    } else if name.contains("skill:") {
        "已通过技能工具加载的操作说明"
    } else if name.contains("skill") {
        "可用技能目录，供模型按需选择"
    } else if name.contains("session-memory") {
        "当前会话的工作摘要"
    } else if name.contains("memory") {
        "项目或团队的持久记忆"
    } else if name.starts_with("instruction:") {
        "工作区配置或自动发现的指令文件"
    } else if name == "task-progress" {
        "任务步骤、完成检查及恢复时的已保存进度"
    } else {
        "运行环境、基础行为或对话摘要"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Reference {
    pub kind: String,
    pub path: String,
}

#[derive(Deserialize)]
pub(super) struct FileQuery {
    path: String,
}

pub(super) fn safe_file(cwd: &Path, relative: &str) -> Result<PathBuf, ApiError> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path.is_absolute()
        || relative.contains(':')
        || path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(ApiError::bad_request(
            "Expected a relative workspace file path",
        ));
    }
    let root = cwd.canonicalize().map_err(internal_error)?;
    let resolved = root.join(path).canonicalize().map_err(internal_error)?;
    if !resolved.starts_with(&root) || !resolved.is_file() {
        return Err(ApiError::bad_request("File must be inside the workspace"));
    }
    Ok(resolved)
}

fn read_text(path: &Path) -> Result<(String, bool), ApiError> {
    let file = fs::File::open(path).map_err(internal_error)?;
    let mut bytes = Vec::new();
    file.take(FILE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(internal_error)?;
    let truncated = bytes.len() > FILE_LIMIT as usize;
    bytes.truncate(FILE_LIMIT as usize);
    if bytes.contains(&0) {
        return Err(ApiError::bad_request(
            "Binary files cannot be previewed as text",
        ));
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok((text, truncated))
}

pub(super) fn git(cwd: &Path, args: &[&str]) -> Result<Vec<u8>, ApiError> {
    let mut command = Command::new("git");
    command
        .arg("-c")
        .arg(format!("safe.directory={}", cwd.display()))
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.output().map_err(internal_error)?;
    if !output.status.success() {
        return Err(ApiError::bad_request(String::from_utf8_lossy(
            &output.stderr,
        )));
    }
    if output.stdout.len() > 8 * 1024 * 1024 {
        return Err(ApiError::bad_request(
            "Git output exceeds 8 MB; narrow the workspace",
        ));
    }
    Ok(output.stdout)
}

pub(super) async fn list(State(state): State<ShellState>) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let files = match git(&state.cwd, &["ls-files", "-co", "--exclude-standard", "-z"]) {
            Ok(files) => files,
            Err(_) => {
                let mut files = walkdir::WalkDir::new(&state.cwd).max_depth(12).into_iter()
                    .filter_entry(|entry| ![".git","target","node_modules",".background"].contains(&entry.file_name().to_string_lossy().as_ref()))
                    .filter_map(Result::ok).filter(|entry|entry.file_type().is_file())
                    .filter_map(|entry|entry.path().strip_prefix(&state.cwd).ok().map(|p|p.to_string_lossy().replace('\\',"/")))
                    .take(3001).collect::<Vec<_>>();
                files.sort(); let truncated=files.len()>3000;files.truncate(3000);
                return Ok(Json(serde_json::json!({"files":files,"changes":[],"truncated":truncated,"gitAvailable":false})));
            }
        };
        let mut files = files
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| String::from_utf8_lossy(p).into_owned())
            .collect::<Vec<_>>();
        files.sort();
        files.dedup();
        let truncated = files.len() > 3000;
        files.truncate(3000);
        let bytes = git(
            &state.cwd,
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        )?;
        let mut changes = Vec::new();
        let mut entries = bytes.split(|b| *b == 0).filter(|p| !p.is_empty());
        while let Some(entry) = entries.next() {
            if entry.len() < 4 {
                continue;
            }
            let status = String::from_utf8_lossy(&entry[..2]).into_owned();
            let path = String::from_utf8_lossy(&entry[3..]).into_owned();
            let original = if status.contains('R') || status.contains('C') {
                entries
                    .next()
                    .map(|v| String::from_utf8_lossy(v).into_owned())
            } else {
                None
            };
            changes.push(serde_json::json!({"status":status,"path":path,"original":original}));
        }
        Ok(Json(
            serde_json::json!({"files":files,"changes":changes,"truncated":truncated}),
        ))
    })
    .await
    .map_err(internal_error)?
}

pub(super) async fn file(
    State(state): State<ShellState>,
    Query(query): Query<FileQuery>,
) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let path = safe_file(&state.cwd, &query.path)?;
        let (content, truncated) = read_text(&path)?;
        Ok(Json(
            serde_json::json!({"path":query.path,"content":content,"truncated":truncated}),
        ))
    })
    .await
    .map_err(internal_error)?
}

pub(super) async fn diff(
    State(state): State<ShellState>,
    Query(query): Query<FileQuery>,
) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        // Deleted files need not exist, but pathspecs must be literal and relative.
        if query.path.contains(':') || query.path.contains('\0') || Path::new(&query.path).components().any(|c| !matches!(c,std::path::Component::Normal(_))) { return Err(ApiError::bad_request("Invalid diff path")); }
        let spec=format!(":(literal){}",query.path);
        let staged=git(&state.cwd,&["diff","--no-ext-diff","--no-textconv","--cached","--",&spec])?;
        let unstaged=git(&state.cwd,&["diff","--no-ext-diff","--no-textconv","--",&spec])?;
        let combined=format!("--- Staged changes ---\n{}\n--- Working tree changes ---\n{}",String::from_utf8_lossy(&staged),String::from_utf8_lossy(&unstaged));
        Ok(Json(serde_json::json!({"content":combined.chars().take(FILE_LIMIT as usize).collect::<String>(),"truncated":combined.chars().count()>FILE_LIMIT as usize})))
    }).await.map_err(internal_error)?
}

pub(super) fn references(
    state: &ShellState,
    refs: &[Reference],
) -> Result<(String, Value), ApiError> {
    if refs.len() > 8 {
        return Err(ApiError::bad_request("Attach at most 8 files or sessions"));
    }
    let mut remaining = REFERENCE_CHARS;
    let mut content = String::new();
    let mut diagnostics = Vec::new();
    for reference in refs {
        let (text, source_truncated) = match reference.kind.as_str() {
            "file" => read_text(&safe_file(&state.cwd, &reference.path)?)?,
            "session" => {
                validate_id(&reference.path)?;
                let session = SessionStore::new(state.config_home.join("sessions"))
                    .load(&reference.path)
                    .map_err(internal_error)?;
                (
                    session
                        .messages
                        .iter()
                        .filter(|m| matches!(m.role, MessageRole::User | MessageRole::Assistant))
                        .filter_map(|m| m.first_text())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    false,
                )
            }
            _ => return Err(ApiError::bad_request("Unknown reference kind")),
        };
        let cap = remaining.min(12_000);
        let kept = text.chars().take(cap).collect::<String>();
        let chars = kept.chars().count();
        remaining -= chars;
        diagnostics.push(serde_json::json!({"kind":reference.kind,"path":reference.path,"characters":chars,"estimatedTokens":(chars+3)/4,"truncated":source_truncated||text.chars().count()>chars}));
        content.push_str(&format!(
            "\n<reference kind={:?} path={:?}>\n{}\n</reference>\n",
            reference.kind, reference.path, kept
        ));
    }
    Ok((
        content,
        serde_json::json!({"references":diagnostics,"characterBudget":REFERENCE_CHARS,"estimatedTokenBudget":REFERENCE_CHARS/4,"usedCharacters":REFERENCE_CHARS-remaining}),
    ))
}

pub(super) fn validate_id(id: &str) -> Result<(), ApiError> {
    if id.is_empty()
        || id.len() > 100
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(ApiError::bad_request("Invalid session ID"));
    }
    Ok(())
}

pub(super) async fn preview(
    State(state): State<ShellState>,
    Json(refs): Json<Vec<Reference>>,
) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || references(&state, &refs).map(|(_, d)| Json(d)))
        .await
        .map_err(internal_error)?
}

pub(super) async fn plan(
    State(state): State<ShellState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    validate_id(&id)?;
    let path = state
        .config_home
        .join("task-plans")
        .join(format!("{id}.json"));
    let mut plan = match fs::read_to_string(path) {
        Ok(s) => serde_json::from_str::<Value>(&s).map_err(internal_error)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Value::Null,
        Err(e) => return Err(internal_error(e)),
    };
    let active = state
        .turns
        .lock()
        .map_err(internal_error)?
        .values()
        .any(|t| t.session_id == id);
    if !active {
        if let Some(steps) = plan.get_mut("steps").and_then(Value::as_array_mut) {
            for step in steps {
                if step["status"] == "in_progress" {
                    step["status"] = serde_json::json!("interrupted");
                }
            }
        }
    }
    Ok(Json(plan))
}

pub(super) async fn diagnostics(
    State(state): State<ShellState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Value>, ApiError> {
    validate_id(&id)?;
    let path = state
        .config_home
        .join("context-diagnostics")
        .join(format!("{id}.json"));
    Ok(Json(
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null),
    ))
}

pub(super) async fn features(State(state): State<ShellState>) -> Result<Json<Value>, ApiError> {
    let config = load_config(&state)?;
    Ok(Json(
        serde_json::json!({"computerEnabled":config.merged().pointer("/computer/enabled").and_then(Value::as_bool).unwrap_or(true),"computerSupported":cfg!(windows),"backgroundMemoryEnabled":config.merged().pointer("/memory/backgroundEnabled").and_then(Value::as_bool).unwrap_or(false)}),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FeatureUpdate {
    computer_enabled: bool,
    background_memory_enabled: bool,
}
pub(super) async fn save_features(
    State(state): State<ShellState>,
    Json(update): Json<FeatureUpdate>,
) -> Result<Json<Value>, ApiError> {
    let mut settings = read_settings(&state.cwd)?;
    if !settings["computer"].is_object() {
        settings["computer"] = serde_json::json!({});
    }
    if !settings["memory"].is_object() {
        settings["memory"] = serde_json::json!({});
    }
    settings["computer"]["enabled"] = serde_json::json!(update.computer_enabled);
    settings["memory"]["backgroundEnabled"] = serde_json::json!(update.background_memory_enabled);
    write_settings(&state.cwd, &settings)?;
    if !update.computer_enabled {
        // A running desktop action must not survive revoking the capability.
        for turn in state.turns.lock().map_err(internal_error)?.values() {
            turn.cancel.send_replace(true);
        }
    }
    features(State(state)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_path_escape_and_invalid_ids() {
        let cwd = std::env::current_dir().unwrap();
        for path in ["../secret", "C:/secret", "/etc/passwd", "file:stream"] {
            assert!(safe_file(&cwd, path).is_err());
        }
        for id in ["../x", "", "a/b", "a:b"] {
            assert!(validate_id(id).is_err());
        }
        assert!(validate_id("session-123_abc").is_ok());
    }
}

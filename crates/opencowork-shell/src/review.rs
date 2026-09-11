use super::*;
use opencowork_tools::file_history;
use std::io::Write;

fn relative(path: &str) -> Result<(), ApiError> {
    if path.is_empty()
        || path.contains([':', '\0', '\n', '\r'])
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(ApiError::bad_request("Expected a relative workspace path"));
    }
    Ok(())
}
fn parts(diff: &str) -> (String, Vec<String>) {
    let mut header = String::new();
    let mut hunks: Vec<String> = Vec::new();
    for line in diff.split_inclusive('\n') {
        if line.starts_with("@@ ") {
            hunks.push(String::new());
        }
        if let Some(hunk) = hunks.last_mut() {
            hunk.push_str(line);
        } else {
            header.push_str(line);
        }
    }
    (header, hunks)
}
fn snapshot(cwd: &Path, path: &str) -> Result<Value, ApiError> {
    relative(path)?;
    opencowork_runtime::check_write_root(cwd, Path::new(path)).map_err(ApiError::bad_request)?;
    let spec = format!(":(literal){path}");
    let unstaged = workspace::git(
        cwd,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--",
            &spec,
        ],
    )?;
    let staged = workspace::git(
        cwd,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--cached",
            "--",
            &spec,
        ],
    )?;
    let content = fs::read(cwd.join(path)).ok();
    let version = content
        .as_deref()
        .map(file_history::version)
        .unwrap_or_else(|| "missing".into());
    let mut all = unstaged.clone();
    all.extend(&staged);
    all.extend(version.as_bytes());
    let u = String::from_utf8_lossy(&unstaged);
    let s = String::from_utf8_lossy(&staged);
    Ok(
        serde_json::json!({"path":path,"version":file_history::version(&all),"fileVersion":version,"unstaged":u,"staged":s,"unstagedHunks":parts(&u).1,"stagedHunks":parts(&s).1}),
    )
}
#[derive(Deserialize)]
pub(super) struct ReviewQuery {
    path: String,
}
pub(super) async fn get(
    State(state): State<ShellState>,
    Query(q): Query<ReviewQuery>,
) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || snapshot(&state.cwd, &q.path).map(Json))
        .await
        .map_err(internal_error)?
}
// Apply a reverse hunk in memory, checking its exact source lines. The file
// history writer then checks the observed version and records recovery data.
fn reverse_hunks(content: &str, hunks: &[String]) -> Result<String, String> {
    let eol = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut lines = content
        .split_inclusive('\n')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    for h in hunks.iter().rev() {
        let mut iter = h.lines();
        let header = iter.next().ok_or("Missing hunk header")?;
        let range = header.split_whitespace().nth(2).ok_or("Invalid hunk")?;
        let start = range
            .trim_start_matches('+')
            .split(',')
            .next()
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or("Invalid hunk position")?;
        let mut old: Vec<String> = Vec::new();
        let mut new: Vec<String> = Vec::new();
        let mut previous = ' ';
        for line in iter {
            let prefix = line.chars().next().unwrap_or(' ');
            if prefix == '\\' {
                if previous != '-' {
                    if let Some(s) = new.last_mut() {
                        if s.ends_with("\r\n") {
                            s.truncate(s.len() - 2);
                        } else {
                            s.pop();
                        }
                    }
                }
                if previous != '+' {
                    if let Some(s) = old.last_mut() {
                        if s.ends_with("\r\n") {
                            s.truncate(s.len() - 2);
                        } else {
                            s.pop();
                        }
                    }
                }
                continue;
            }
            let text = format!("{}{eol}", line.get(1..).ok_or("Invalid hunk line")?);
            if prefix != '+' {
                old.push(text.clone());
            }
            if prefix != '-' {
                new.push(text);
            }
            previous = prefix;
        }
        let at = if new.is_empty() {
            start
        } else {
            start.saturating_sub(1)
        };
        if lines.get(at..at + new.len()) != Some(new.as_slice()) {
            return Err("stale_file: hunk no longer matches".into());
        }
        lines.splice(at..at + new.len(), old);
    }
    Ok(lines.concat())
}
pub(super) async fn change(
    State(state): State<ShellState>,
    Json(v): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        if v["action"] == "restore" {
            return file_history::restore(&state.cwd, v["snapshotId"].as_str().unwrap_or(""))
                .map(Json)
                .map_err(ApiError::bad_request);
        }
        let path = v["path"]
            .as_str()
            .ok_or_else(|| ApiError::bad_request("Path required"))?;
        let _lease = opencowork_runtime::ExclusiveLease::acquire(
            &state.config_home.join("review-locks").join(format!(
                "{}.lock",
                file_history::version(state.cwd.to_string_lossy().as_bytes())
            )),
        )
        .map_err(internal_error)?;
        let current = snapshot(&state.cwd, path)?;
        if current["version"] != v["version"] {
            return Err(ApiError::bad_request(
                "stale_file: refresh the review; newer changes preserved",
            ));
        }
        let action = v["action"].as_str().unwrap_or("");
        let spec = format!(":(literal){path}");
        if v["hunk"].is_null() && action == "stage" {
            workspace::git(&state.cwd, &["add", "--", &spec])?;
        } else if v["hunk"].is_null() && action == "unstage" {
            workspace::git(&state.cwd, &["reset", "--quiet", "HEAD", "--", &spec])?;
        } else {
            let diff = current[if action == "unstage" {
                "staged"
            } else {
                "unstaged"
            }]
            .as_str()
            .unwrap_or("");
            let (header, hunks) = parts(diff);
            if hunks.is_empty()
                || header.contains("new file mode")
                || header.contains("deleted file mode")
                || header.contains("rename from")
                || header.contains("GIT binary patch")
            {
                return Err(ApiError::bad_request(
                    "Hunk actions require an existing ordinary text file",
                ));
            }
            let chosen = if let Some(i) = v["hunk"].as_u64() {
                vec![hunks
                    .get(i as usize)
                    .ok_or_else(|| ApiError::bad_request("Invalid hunk"))?
                    .clone()]
            } else {
                hunks
            };
            if action == "revert" {
                let original = fs::read_to_string(state.cwd.join(path)).map_err(internal_error)?;
                let next = reverse_hunks(&original, &chosen).map_err(ApiError::bad_request)?;
                return file_history::write(
                    &state.cwd.join(path),
                    &next,
                    current["fileVersion"].as_str(),
                )
                .map(Json)
                .map_err(ApiError::bad_request);
            }
            if !["stage", "unstage"].contains(&action) {
                return Err(ApiError::bad_request("Unknown review action"));
            }
            let mut command = std::process::Command::new("git");
            command
                .arg("-c")
                .arg(format!("safe.directory={}", state.cwd.display()))
                .args(["apply", "--cached", "--whitespace=nowarn"]);
            if action == "unstage" {
                command.arg("--reverse");
            }
            command
                .current_dir(&state.cwd)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            let mut child = command.spawn().map_err(internal_error)?;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(format!("{header}{}", chosen.concat()).as_bytes())
                .map_err(internal_error)?;
            let output = child.wait_with_output().map_err(internal_error)?;
            if !output.status.success() {
                return Err(ApiError::bad_request(String::from_utf8_lossy(
                    &output.stderr,
                )));
            }
        }
        Ok(Json(serde_json::json!({"updated":true})))
    })
    .await
    .map_err(internal_error)?
}
pub(super) async fn history(State(state): State<ShellState>) -> Result<Json<Value>, ApiError> {
    let mut items = Vec::new();
    for entry in fs::read_dir(state.config_home.join("file-history"))
        .into_iter()
        .flatten()
        .flatten()
    {
        if entry.path().extension().is_none_or(|e| e != "json") {
            continue;
        }
        if let Ok(v) = fs::read(&entry.path())
            .and_then(|b| serde_json::from_slice::<Value>(&b).map_err(std::io::Error::other))
        {
            if v["path"].as_str().is_some_and(|p| {
                opencowork_runtime::check_write_root(&state.cwd, Path::new(p)).is_ok()
            }) {
                items.push(
                    serde_json::json!({"id":v["id"],"path":v["path"],"createdAt":v["createdAt"]}),
                );
            }
        }
    }
    items.sort_by_key(|v| std::cmp::Reverse(v["createdAt"].as_u64().unwrap_or(0)));
    items.truncate(100);
    Ok(Json(serde_json::json!(items)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reverse_preserves_other_edits() {
        let s = "one\nchanged\nthree\nuser\n";
        let h = "@@ -1,3 +1,3 @@\n one\n-old\n+changed\n three\n";
        assert_eq!(
            reverse_hunks(s, &[h.into()]).unwrap(),
            "one\nold\nthree\nuser\n"
        );
        assert!(reverse_hunks("different\n", &[h.into()]).is_err());
    }
    #[test]
    fn reverse_handles_missing_newline() {
        assert_eq!(reverse_hunks("new",&["@@ -1 +1 @@\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n".into()]).unwrap(),"old");
    }

    #[test]
    fn reverse_preserves_windows_line_endings() {
        assert_eq!(
            reverse_hunks(
                "new\r\nuntouched\r\n",
                &["@@ -1,2 +1,2 @@\n-old\n+new\n untouched\n".into()]
            )
            .unwrap(),
            "old\r\nuntouched\r\n"
        );
    }
}

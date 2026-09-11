#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PermissionMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub input: String,
    pub required: PermissionMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    Deny { reason: String },
}

pub trait PermissionPrompter {
    fn decide(&mut self, request: &PermissionRequest) -> PermissionDecision;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionPolicy {
    active_mode: PermissionMode,
}

impl PermissionPolicy {
    #[must_use]
    pub fn new(active_mode: PermissionMode) -> Self {
        Self { active_mode }
    }

    #[must_use]
    pub fn active_mode(&self) -> PermissionMode {
        self.active_mode
    }

    pub fn authorize(
        &self,
        tool_name: &str,
        input: &str,
        required: PermissionMode,
        prompter: Option<&mut dyn PermissionPrompter>,
    ) -> PermissionDecision {
        if self.active_mode == PermissionMode::WorkspaceWrite
            && matches!(tool_name, "write_file" | "edit_file")
        {
            let checked = (|| {
                let value: serde_json::Value =
                    serde_json::from_str(input).map_err(|e| e.to_string())?;
                let path = value["path"].as_str().ok_or("Missing file path")?;
                check_write_root(
                    &std::env::current_dir().map_err(|e| e.to_string())?,
                    std::path::Path::new(path),
                )
            })();
            if let Err(reason) = checked {
                return PermissionDecision::Deny { reason };
            }
        }
        if self.active_mode >= required {
            return PermissionDecision::Allow;
        }
        match prompter {
            Some(prompter) => prompter.decide(&PermissionRequest {
                tool_name: tool_name.to_string(),
                input: input.to_string(),
                required,
            }),
            None => PermissionDecision::Deny {
                reason: format!(
                    "tool `{tool_name}` requires {:?}, current mode is {:?}",
                    required, self.active_mode
                ),
            },
        }
    }
}

/// Resolve existing parents too, so new files cannot escape through a junction.
/// This is a file-tool guard, not an OS sandbox for arbitrary subprocesses.
pub fn check_write_root(root: &std::path::Path, path: &std::path::Path) -> Result<(), String> {
    use std::path::Component;
    if path.as_os_str().is_empty() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("workspace_write_denied: traversal or empty path".into());
    }
    // Alternate data streams and device paths are not normal workspace files.
    if path
        .components()
        .any(|c| matches!(c, Component::Normal(n) if n.to_string_lossy().contains(':')))
    {
        return Err("workspace_write_denied: alternate data stream".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let target = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let mut parent = target.as_path();
    loop {
        match parent.symlink_metadata() {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                parent = parent
                    .parent()
                    .ok_or("workspace_write_denied: no existing parent")?;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    let resolved = parent.canonicalize().map_err(|e| e.to_string())?;
    if !resolved.starts_with(&root) {
        return Err("workspace_write_denied: path is outside the workspace".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confines_existing_and_new_paths() {
        let base = std::env::temp_dir().join(format!("cowork-roots-{}", std::process::id()));
        let root = base.join("project");
        std::fs::create_dir_all(&root).unwrap();
        assert!(check_write_root(&root, std::path::Path::new("new/inside.txt")).is_ok());
        assert!(check_write_root(&root, &base.join("outside.txt")).is_err());
        assert!(check_write_root(&root, std::path::Path::new("../outside.txt")).is_err());
        assert!(check_write_root(&root, std::path::Path::new("file:stream")).is_err());
        #[cfg(unix)]
        {
            let link = root.join("escape");
            let _ = std::os::unix::fs::symlink(&base, &link);
            assert!(check_write_root(&root, &link.join("new.txt")).is_err());
        }
    }
}

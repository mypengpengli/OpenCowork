use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
pub fn version(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn write(path: &Path, content: &str, expected: Option<&str>) -> Result<Value, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    let home = opencowork_runtime::default_config_home().join("file-history");
    let key = version(absolute.to_string_lossy().as_bytes());
    let _lease = opencowork_runtime::ExclusiveLease::acquire(&home.join(format!("{key}.lock")))
        .map_err(|_| "File is being edited by another tool".to_string())?;
    let before = match std::fs::read_to_string(&absolute) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.to_string()),
    };
    let old_version = before
        .as_deref()
        .map(|s| version(s.as_bytes()))
        .unwrap_or_else(|| "missing".into());
    if expected.is_some_and(|e| e != old_version) {
        return Err("stale_file: read the file again before editing".into());
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let id = format!("{}-{stamp}", std::process::id());
    let after_version = version(content.as_bytes());
    let entry = json!({"id":id,"path":absolute,"before":before,"beforeVersion":old_version,"afterVersion":after_version,"createdAt":stamp/1_000_000});
    opencowork_runtime::write_json_atomic(&home.join(format!("{id}.json")), &entry)
        .map_err(|e| e.to_string())?;
    if let Some(parent) = absolute.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // The optimistic check also protects against changes made since read_file.
    let current = std::fs::read_to_string(&absolute).ok();
    if current != before {
        return Err("stale_file: concurrent modification detected".into());
    }
    let temp = absolute.with_extension(format!("cowork-{id}.tmp"));
    let result = std::fs::write(&temp, content).and_then(|_| std::fs::rename(&temp, &absolute));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(|e| e.to_string())?;
    Ok(json!({"path":path,"written":true,"version":after_version,"snapshotId":id}))
}

pub fn restore(cwd: &Path, id: &str) -> Result<Value, String> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit() || b == b'-') {
        return Err("Invalid snapshot ID".into());
    }
    let home = opencowork_runtime::default_config_home().join("file-history");
    let entry: Value = serde_json::from_slice(
        &std::fs::read(home.join(format!("{id}.json"))).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let path = Path::new(entry["path"].as_str().ok_or("Snapshot path missing")?);
    opencowork_runtime::check_write_root(cwd, path)?;
    if let Some(before) = entry["before"].as_str() {
        write(path, before, entry["afterVersion"].as_str())
    } else {
        let _lease = opencowork_runtime::ExclusiveLease::acquire(&home.join(format!(
            "{}.lock",
            version(path.to_string_lossy().as_bytes())
        )))
        .map_err(|e| e.to_string())?;
        if version(&std::fs::read(path).map_err(|e| e.to_string())?)
            != entry["afterVersion"].as_str().unwrap_or("")
        {
            return Err("stale_file: newer changes preserved".into());
        }
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
        Ok(json!({"restored":id}))
    }
}

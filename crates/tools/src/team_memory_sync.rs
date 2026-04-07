use opencowork_runtime::{project_team_memory_root, RuntimeConfig};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const MAX_FILE_SIZE_BYTES: u64 = 250_000;
const MAX_CONFLICT_RETRIES: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TeamMemorySyncStatus {
    pub running: bool,
    pub repo_slug: Option<String>,
    pub endpoint: Option<String>,
    pub last_error: Option<String>,
    pub last_known_checksum: Option<String>,
    pub pending_changes: bool,
    pub files_pulled: usize,
    pub files_pushed: usize,
    pub last_pull_unix_ms: Option<u128>,
    pub last_push_unix_ms: Option<u128>,
}

#[derive(Debug, Clone, Default)]
struct SyncState {
    last_known_checksum: Option<String>,
    server_checksums: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct TeamMemoryContentPayload {
    entries: BTreeMap<String, String>,
    #[serde(default, rename = "entryChecksums")]
    entry_checksums: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct TeamMemoryPullPayload {
    #[serde(default)]
    checksum: Option<String>,
    content: TeamMemoryContentPayload,
}

#[derive(Debug, Deserialize)]
struct TeamMemoryUploadPayload {
    #[serde(default)]
    checksum: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct LocalSnapshot {
    entries: BTreeMap<String, String>,
    hashes: BTreeMap<String, String>,
}

enum TeamMemoryCommand {
    NotifyWrite,
    ForcePull,
    ForcePush,
    Stop,
}

struct TeamMemoryService {
    sender: Sender<TeamMemoryCommand>,
    status: Arc<Mutex<TeamMemorySyncStatus>>,
    handle: Option<JoinHandle<()>>,
}

static TEAM_MEMORY_SERVICE: OnceLock<Mutex<Option<TeamMemoryService>>> = OnceLock::new();

fn service_slot() -> &'static Mutex<Option<TeamMemoryService>> {
    TEAM_MEMORY_SERVICE.get_or_init(|| Mutex::new(None))
}

pub fn start_team_memory_sync(
    cwd: &Path,
    config_home: &Path,
    config: &RuntimeConfig,
) -> Result<bool, String> {
    let team_config = config.team_memory_sync();
    if !team_config.is_configured() {
        return Ok(false);
    }

    let endpoint = team_config
        .endpoint()
        .map(ToOwned::to_owned)
        .ok_or_else(|| "team memory sync endpoint is not configured".to_string())?;
    let repo_slug = resolve_repo_slug(cwd, team_config.repo()).ok_or_else(|| {
        "team memory sync requires a configured repo or a GitHub remote".to_string()
    })?;
    {
        let slot = service_slot();
        let guard = slot
            .lock()
            .map_err(|_| "team memory service lock poisoned".to_string())?;
        if let Some(existing) = guard.as_ref() {
            if let Ok(current) = existing.status.lock() {
                if current.running
                    && current.repo_slug.as_deref() == Some(repo_slug.as_str())
                    && current.endpoint.as_deref() == Some(endpoint.as_str())
                {
                    return Ok(false);
                }
            }
        }
    }
    let team_dir = project_team_memory_root(config_home, cwd);
    fs::create_dir_all(&team_dir).map_err(|error| error.to_string())?;

    let client = build_client(team_config.timeout_ms())?;
    let token = team_config
        .token_env()
        .and_then(|name| std::env::var(name).ok())
        .filter(|value| !value.trim().is_empty());
    let mut sync_state = SyncState::default();
    let status = Arc::new(Mutex::new(TeamMemorySyncStatus {
        running: true,
        repo_slug: Some(repo_slug.clone()),
        endpoint: Some(endpoint.clone()),
        ..TeamMemorySyncStatus::default()
    }));

    if let Err(error) = pull_team_memory(
        &client,
        &endpoint,
        &repo_slug,
        token.as_deref(),
        &team_dir,
        &mut sync_state,
        &status,
    ) {
        update_status(&status, |value| value.last_error = Some(error));
    }

    let initial_snapshot = scan_local_snapshot(&team_dir).map_err(|error| error.to_string())?;
    let (sender, receiver) = mpsc::channel();
    let poll_ms = team_config.poll_ms();
    let debounce_ms = team_config.debounce_ms();
    let thread_status = Arc::clone(&status);
    let thread_handle = thread::Builder::new()
        .name("opencowork-team-memory-sync".to_string())
        .spawn(move || {
            run_service_loop(
                client,
                endpoint,
                repo_slug,
                token,
                team_dir,
                sync_state,
                initial_snapshot,
                receiver,
                thread_status,
                poll_ms,
                debounce_ms,
            )
        })
        .map_err(|error| error.to_string())?;

    let slot = service_slot();
    let mut guard = slot
        .lock()
        .map_err(|_| "team memory service lock poisoned".to_string())?;
    if guard.is_some() {
        drop(guard);
        stop_team_memory_sync()?;
        guard = slot
            .lock()
            .map_err(|_| "team memory service lock poisoned".to_string())?;
    }
    *guard = Some(TeamMemoryService {
        sender,
        status,
        handle: Some(thread_handle),
    });
    Ok(true)
}

pub fn stop_team_memory_sync() -> Result<(), String> {
    let service = {
        let slot = service_slot();
        let mut guard = slot
            .lock()
            .map_err(|_| "team memory service lock poisoned".to_string())?;
        guard.take()
    };
    let Some(mut service) = service else {
        return Ok(());
    };

    let _ = service.sender.send(TeamMemoryCommand::Stop);
    if let Some(handle) = service.handle.take() {
        handle
            .join()
            .map_err(|_| "team memory service thread panicked".to_string())?;
    }
    Ok(())
}

pub fn notify_team_memory_write() -> Result<(), String> {
    with_service_sender(|sender| sender.send(TeamMemoryCommand::NotifyWrite))?;
    Ok(())
}

pub fn notify_team_memory_write_if_needed(file_path: &str) -> Result<(), String> {
    if crate::team_memory::is_team_memory_path(file_path)? {
        notify_team_memory_write()?;
    }
    Ok(())
}

pub fn force_pull_team_memory() -> Result<(), String> {
    with_service_sender(|sender| sender.send(TeamMemoryCommand::ForcePull))?;
    Ok(())
}

pub fn force_push_team_memory() -> Result<(), String> {
    with_service_sender(|sender| sender.send(TeamMemoryCommand::ForcePush))?;
    Ok(())
}

#[must_use]
pub fn team_memory_sync_status() -> TeamMemorySyncStatus {
    let Ok(guard) = service_slot().lock() else {
        return TeamMemorySyncStatus::default();
    };
    guard
        .as_ref()
        .and_then(|service| service.status.lock().ok().map(|value| value.clone()))
        .unwrap_or_default()
}

fn with_service_sender(
    f: impl FnOnce(&Sender<TeamMemoryCommand>) -> Result<(), mpsc::SendError<TeamMemoryCommand>>,
) -> Result<(), String> {
    let slot = service_slot();
    let guard = slot
        .lock()
        .map_err(|_| "team memory service lock poisoned".to_string())?;
    let Some(service) = guard.as_ref() else {
        return Ok(());
    };
    f(&service.sender).map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
fn run_service_loop(
    client: Client,
    endpoint: String,
    repo_slug: String,
    token: Option<String>,
    team_dir: PathBuf,
    mut sync_state: SyncState,
    mut snapshot: LocalSnapshot,
    receiver: Receiver<TeamMemoryCommand>,
    status: Arc<Mutex<TeamMemorySyncStatus>>,
    poll_ms: u64,
    debounce_ms: u64,
) {
    let poll_interval = Duration::from_millis(poll_ms.max(250));
    let debounce_interval = u128::from(debounce_ms.max(250));
    let mut pending_since: Option<u128> = None;

    loop {
        match receiver.recv_timeout(poll_interval) {
            Ok(TeamMemoryCommand::NotifyWrite) => {
                pending_since.get_or_insert_with(now_ms);
                update_status(&status, |value| value.pending_changes = true);
            }
            Ok(TeamMemoryCommand::ForcePull) => {
                if let Err(error) = pull_team_memory(
                    &client,
                    &endpoint,
                    &repo_slug,
                    token.as_deref(),
                    &team_dir,
                    &mut sync_state,
                    &status,
                ) {
                    update_status(&status, |value| value.last_error = Some(error));
                } else if let Ok(new_snapshot) = scan_local_snapshot(&team_dir) {
                    snapshot = new_snapshot;
                }
            }
            Ok(TeamMemoryCommand::ForcePush) => {
                match push_team_memory(
                    &client,
                    &endpoint,
                    &repo_slug,
                    token.as_deref(),
                    &team_dir,
                    &mut sync_state,
                    &snapshot,
                    &status,
                ) {
                    Ok(_) => {
                        pending_since = None;
                        update_status(&status, |value| value.pending_changes = false);
                        if let Ok(new_snapshot) = scan_local_snapshot(&team_dir) {
                            snapshot = new_snapshot;
                        }
                    }
                    Err(error) => update_status(&status, |value| value.last_error = Some(error)),
                }
            }
            Ok(TeamMemoryCommand::Stop) => {
                if pending_since.is_some() {
                    let _ = push_team_memory(
                        &client,
                        &endpoint,
                        &repo_slug,
                        token.as_deref(),
                        &team_dir,
                        &mut sync_state,
                        &snapshot,
                        &status,
                    );
                }
                update_status(&status, |value| {
                    value.pending_changes = false;
                    value.running = false;
                });
                break;
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Ok(new_snapshot) = scan_local_snapshot(&team_dir) {
                    if new_snapshot.hashes != snapshot.hashes {
                        snapshot = new_snapshot;
                        pending_since.get_or_insert_with(now_ms);
                        update_status(&status, |value| value.pending_changes = true);
                    }
                }

                if pending_since
                    .is_some_and(|started| now_ms().saturating_sub(started) >= debounce_interval)
                {
                    match push_team_memory(
                        &client,
                        &endpoint,
                        &repo_slug,
                        token.as_deref(),
                        &team_dir,
                        &mut sync_state,
                        &snapshot,
                        &status,
                    ) {
                        Ok(_) => {
                            pending_since = None;
                            update_status(&status, |value| value.pending_changes = false);
                            if let Ok(new_snapshot) = scan_local_snapshot(&team_dir) {
                                snapshot = new_snapshot;
                            }
                        }
                        Err(error) => {
                            update_status(&status, |value| value.last_error = Some(error));
                            pending_since = Some(now_ms());
                        }
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                update_status(&status, |value| value.running = false);
                break;
            }
        }
    }
}

fn build_client(timeout_ms: u64) -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_millis(timeout_ms.max(1_000)))
        .build()
        .map_err(|error| error.to_string())
}

fn pull_team_memory(
    client: &Client,
    endpoint: &str,
    repo_slug: &str,
    token: Option<&str>,
    team_dir: &Path,
    sync_state: &mut SyncState,
    status: &Arc<Mutex<TeamMemorySyncStatus>>,
) -> Result<usize, String> {
    let mut request = client.get(sync_endpoint(endpoint, repo_slug));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    if let Some(checksum) = &sync_state.last_known_checksum {
        request = request.header("If-None-Match", format!("\"{checksum}\""));
    }
    let response = request.send().map_err(|error| error.to_string())?;
    let response_status = response.status();
    if response_status == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(0);
    }
    if response_status == reqwest::StatusCode::NOT_FOUND {
        sync_state.last_known_checksum = None;
        sync_state.server_checksums.clear();
        update_status(status, |value| {
            value.last_pull_unix_ms = Some(now_ms());
            value.last_known_checksum = None;
            value.last_error = None;
        });
        return Ok(0);
    }
    if !response_status.is_success() {
        return Err(format!(
            "team memory pull failed with status {}",
            response_status.as_u16()
        ));
    }

    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_matches('"').to_string());
    let payload = response
        .json::<TeamMemoryPullPayload>()
        .map_err(|error| error.to_string())?;
    write_remote_entries(team_dir, &payload.content.entries)?;
    sync_state.server_checksums = payload.content.entry_checksums;
    sync_state.last_known_checksum = payload.checksum.or(etag);
    update_status(status, |value| {
        value.files_pulled = payload.content.entries.len();
        value.last_pull_unix_ms = Some(now_ms());
        value.last_known_checksum = sync_state.last_known_checksum.clone();
        value.last_error = None;
    });
    Ok(payload.content.entries.len())
}

#[allow(clippy::too_many_arguments)]
fn push_team_memory(
    client: &Client,
    endpoint: &str,
    repo_slug: &str,
    token: Option<&str>,
    team_dir: &Path,
    sync_state: &mut SyncState,
    snapshot: &LocalSnapshot,
    status: &Arc<Mutex<TeamMemorySyncStatus>>,
) -> Result<usize, String> {
    push_team_memory_with_retry(
        client,
        endpoint,
        repo_slug,
        token,
        team_dir,
        sync_state,
        snapshot,
        status,
        MAX_CONFLICT_RETRIES,
    )
}

#[allow(clippy::too_many_arguments)]
fn push_team_memory_with_retry(
    client: &Client,
    endpoint: &str,
    repo_slug: &str,
    token: Option<&str>,
    team_dir: &Path,
    sync_state: &mut SyncState,
    snapshot: &LocalSnapshot,
    status: &Arc<Mutex<TeamMemorySyncStatus>>,
    retries_remaining: usize,
) -> Result<usize, String> {
    let changed_entries = snapshot
        .entries
        .iter()
        .filter(|(path, _)| {
            let local_hash = snapshot.hashes.get(*path);
            let remote_hash = sync_state.server_checksums.get(*path);
            local_hash != remote_hash
        })
        .map(|(path, content)| (path.clone(), content.clone()))
        .collect::<BTreeMap<_, _>>();
    if changed_entries.is_empty() {
        update_status(status, |value| {
            value.last_error = None;
            value.last_push_unix_ms = Some(now_ms());
        });
        return Ok(0);
    }

    let mut request = client
        .put(sync_endpoint(endpoint, repo_slug))
        .json(&json!({ "entries": changed_entries }));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    if let Some(checksum) = &sync_state.last_known_checksum {
        request = request.header("If-Match", format!("\"{checksum}\""));
    }
    let response = request.send().map_err(|error| error.to_string())?;
    let response_status = response.status();

    if response_status == reqwest::StatusCode::PRECONDITION_FAILED {
        if retries_remaining == 0 {
            return Err("team memory push conflicted after retries".to_string());
        }
        let _ = pull_team_memory(
            client, endpoint, repo_slug, token, team_dir, sync_state, status,
        )?;
        let refreshed_snapshot =
            scan_local_snapshot(team_dir).map_err(|error| error.to_string())?;
        return push_team_memory_with_retry(
            client,
            endpoint,
            repo_slug,
            token,
            team_dir,
            sync_state,
            &refreshed_snapshot,
            status,
            retries_remaining.saturating_sub(1),
        );
    }

    if !response_status.is_success() {
        return Err(format!(
            "team memory push failed with status {}",
            response_status.as_u16()
        ));
    }

    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim_matches('"').to_string());
    let payload = response
        .json::<TeamMemoryUploadPayload>()
        .ok()
        .and_then(|value| value.checksum)
        .or(etag);
    for (path, hash) in &snapshot.hashes {
        if changed_entries.contains_key(path) {
            sync_state
                .server_checksums
                .insert(path.clone(), hash.clone());
        }
    }
    sync_state.last_known_checksum = payload;
    update_status(status, |value| {
        value.files_pushed = changed_entries.len();
        value.last_push_unix_ms = Some(now_ms());
        value.last_known_checksum = sync_state.last_known_checksum.clone();
        value.last_error = None;
    });
    Ok(changed_entries.len())
}

fn scan_local_snapshot(team_dir: &Path) -> Result<LocalSnapshot, std::io::Error> {
    let mut snapshot = LocalSnapshot::default();
    for entry in WalkDir::new(team_dir).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.len() > MAX_FILE_SIZE_BYTES {
            continue;
        }
        let relative = path
            .strip_prefix(team_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if !is_valid_entry_key(&relative) {
            continue;
        }
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let hash = hash_content(&content);
        snapshot.entries.insert(relative.clone(), content);
        snapshot.hashes.insert(relative, hash);
    }
    Ok(snapshot)
}

fn write_remote_entries(team_dir: &Path, entries: &BTreeMap<String, String>) -> Result<(), String> {
    for (key, content) in entries {
        if !is_valid_entry_key(key) {
            continue;
        }
        let path = team_dir.join(PathBuf::from(
            key.replace('/', std::path::MAIN_SEPARATOR_STR),
        ));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(path, content).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn sync_endpoint(base: &str, repo_slug: &str) -> String {
    let separator = if base.contains('?') { '&' } else { '?' };
    format!("{base}{separator}repo={repo_slug}")
}

fn hash_content(content: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(content.as_bytes());
    format!("sha256:{:x}", digest.finalize())
}

fn update_status(
    status: &Arc<Mutex<TeamMemorySyncStatus>>,
    update: impl FnOnce(&mut TeamMemorySyncStatus),
) {
    if let Ok(mut guard) = status.lock() {
        update(&mut guard);
    }
}

fn resolve_repo_slug(cwd: &Path, configured: Option<&str>) -> Option<String> {
    configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| detect_repo_slug_from_git(cwd))
}

fn detect_repo_slug_from_git(cwd: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("config")
        .arg("--get")
        .arg("remote.origin.url")
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let remote = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_github_repo_slug(&remote)
}

fn parse_github_repo_slug(remote: &str) -> Option<String> {
    let trimmed = remote.trim().trim_end_matches('/');
    let without_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    if let Some((_, tail)) = without_git.split_once("github.com/") {
        let mut parts = tail.split('/').filter(|value| !value.is_empty());
        let owner = parts.next()?;
        let repo = parts.next()?;
        return Some(format!("{owner}/{repo}"));
    }
    if let Some(tail) = without_git.strip_prefix("git@github.com:") {
        let mut parts = tail.split('/').filter(|value| !value.is_empty());
        let owner = parts.next()?;
        let repo = parts.next()?;
        return Some(format!("{owner}/{repo}"));
    }
    None
}

fn is_valid_entry_key(key: &str) -> bool {
    let candidate = Path::new(key);
    !candidate.is_absolute()
        && candidate.components().all(|component| {
            matches!(component, Component::Normal(_)) || component == Component::CurDir
        })
        && !key.contains('\\')
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::{hash_content, is_valid_entry_key, parse_github_repo_slug};

    #[test]
    fn parses_github_https_remote() {
        assert_eq!(
            parse_github_repo_slug("https://github.com/acme/project.git"),
            Some("acme/project".to_string())
        );
    }

    #[test]
    fn parses_github_ssh_remote() {
        assert_eq!(
            parse_github_repo_slug("git@github.com:acme/project.git"),
            Some("acme/project".to_string())
        );
    }

    #[test]
    fn rejects_invalid_team_memory_key() {
        assert!(!is_valid_entry_key("../MEMORY.md"));
        assert!(!is_valid_entry_key("..\\MEMORY.md"));
        assert!(is_valid_entry_key("patterns/MEMORY.md"));
    }

    #[test]
    fn hashes_content_like_sha256() {
        assert_eq!(
            hash_content("abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}

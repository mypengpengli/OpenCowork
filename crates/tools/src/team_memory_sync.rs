use opencowork_runtime::{project_team_memory_root, RuntimeConfig};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const MAX_FILE_SIZE_BYTES: u64 = 250_000;
const MAX_CONFLICT_RETRIES: usize = 2;
const MAX_PAYLOAD_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ENTRIES: usize = 512;

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SyncState {
    #[serde(default)]
    remote_identity: Option<String>,
    last_known_checksum: Option<String>,
    server_checksums: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct TeamMemoryContentPayload {
    entries: BTreeMap<String, String>,
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

static TEAM_MEMORY_START: Mutex<()> = Mutex::new(());

static TEAM_MEMORY_SERVICE: OnceLock<Mutex<BTreeMap<PathBuf, TeamMemoryService>>> = OnceLock::new();

fn service_slot() -> &'static Mutex<BTreeMap<PathBuf, TeamMemoryService>> {
    TEAM_MEMORY_SERVICE.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub fn start_team_memory_sync(
    cwd: &Path,
    config_home: &Path,
    config: &RuntimeConfig,
) -> Result<bool, String> {
    let _start = TEAM_MEMORY_START
        .lock()
        .map_err(|_| "Team memory initialization lock poisoned".to_string())?;
    let team_dir = project_team_memory_root(config_home, cwd);
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
        if let Some(existing) = guard.get(&team_dir) {
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
    fs::create_dir_all(&team_dir).map_err(|error| error.to_string())?;

    let client = build_client(team_config.timeout_ms())?;
    let token = if let Some(file) = config
        .merged()
        .pointer("/teamMemorySync/tokenFile")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.trim().is_empty())
    {
        let value = read_limited_file(Path::new(file), 16_384)
            .map_err(|_| "Cannot read team memory token file (maximum 16 KB)".to_string())?;
        Some(value.trim().to_owned())
    } else if let Some(name) = team_config
        .token_env()
        .filter(|name| !name.trim().is_empty())
    {
        let explicit = config
            .merged()
            .pointer("/teamMemorySync/tokenEnv")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|v| !v.trim().is_empty())
            || std::env::var("OPENCOWORK_TEAM_MEMORY_SYNC_TOKEN_ENV").is_ok();
        match std::env::var(name) {
            Ok(value) => Some(value),
            Err(_) if explicit => {
                return Err(format!(
                    "Team memory token environment variable {name} is missing"
                ))
            }
            Err(_) => None,
        }
    } else {
        None
    };
    if token
        .as_ref()
        .is_some_and(|t| t.trim().is_empty() || t.contains(['\r', '\n']))
    {
        return Err("Team memory token is empty or contains a newline".into());
    }
    // Stop the old destination before reading or changing this workspace's state.
    let previous = service_slot()
        .lock()
        .map_err(|_| "Team memory service lock poisoned")?
        .remove(&team_dir);
    if let Some(mut previous) = previous {
        let _ = previous.sender.send(TeamMemoryCommand::Stop);
        if let Some(handle) = previous.handle.take() {
            let _ = handle.join();
        }
    }
    let mut sync_state: SyncState = fs::read(team_dir.join(".sync-state.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    bind_remote_identity(&mut sync_state, &endpoint, &repo_slug);
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
    let service_key = team_dir.clone();
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
    guard.insert(
        service_key,
        TeamMemoryService {
            sender,
            status,
            handle: Some(thread_handle),
        },
    );
    Ok(true)
}

pub fn stop_team_memory_sync() -> Result<(), String> {
    let service = {
        let slot = service_slot();
        let mut guard = slot
            .lock()
            .map_err(|_| "team memory service lock poisoned".to_string())?;
        std::mem::take(&mut *guard)
    };
    for (_, mut service) in service {
        let _ = service.sender.send(TeamMemoryCommand::Stop);
        if let Some(handle) = service.handle.take() {
            handle
                .join()
                .map_err(|_| "team memory service thread panicked".to_string())?;
        }
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
    let cwd = std::env::current_dir().unwrap_or_default();
    team_memory_sync_status_for(&cwd, &opencowork_runtime::default_config_home())
}

pub fn team_memory_sync_status_for(cwd: &Path, config_home: &Path) -> TeamMemorySyncStatus {
    let key = project_team_memory_root(config_home, cwd);
    let Ok(guard) = service_slot().lock() else {
        return TeamMemorySyncStatus::default();
    };
    guard
        .get(&key)
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
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let key = project_team_memory_root(&opencowork_runtime::default_config_home(), &cwd);
    let Some(service) = guard.get(&key) else {
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
    let mut pending_since = if snapshot.hashes != sync_state.server_checksums {
        Some(now_ms())
    } else {
        None
    };
    update_status(&status, |value| {
        value.pending_changes = pending_since.is_some()
    });
    let mut last_remote_poll = now_ms();

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
                if now_ms().saturating_sub(last_remote_poll) >= u128::from(poll_ms.max(5_000)) {
                    last_remote_poll = now_ms();
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
                }
                match scan_local_snapshot(&team_dir) {
                    Ok(new_snapshot) => {
                        if new_snapshot.hashes != snapshot.hashes {
                            snapshot = new_snapshot;
                            pending_since.get_or_insert_with(now_ms);
                            update_status(&status, |value| value.pending_changes = true);
                        }
                    }
                    Err(error) => {
                        update_status(&status, |value| value.last_error = Some(error.to_string()));
                        continue;
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
        .redirect(reqwest::redirect::Policy::none())
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
        if !team_memory_conflicts(team_dir)?.is_empty() {
            return Err("Team memory conflicts require review".into());
        }
        update_status(status, |value| {
            value.last_error = None;
            value.last_pull_unix_ms = Some(now_ms());
        });
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
    let payload: TeamMemoryPullPayload = read_bounded_json(response)?;
    validate_entries(&payload.content.entries)?;
    let conflicts = merge_remote_entries(
        team_dir,
        &payload.content.entries,
        &sync_state.server_checksums,
    )?;
    // Compute hashes ourselves instead of trusting absent or inconsistent server metadata.
    sync_state.server_checksums = payload
        .content
        .entries
        .iter()
        .map(|(k, v)| (k.clone(), hash_content(v)))
        .collect();
    sync_state.last_known_checksum = payload.checksum.or(etag);
    opencowork_runtime::write_json_atomic(&team_dir.join(".sync-state.json"), &sync_state)
        .map_err(|e| e.to_string())?;
    if conflicts > 0 {
        return Err(format!(
            "{conflicts} team memory conflicts require review; local files preserved"
        ));
    }
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
    let latest = scan_local_snapshot(team_dir).map_err(|e| e.to_string())?;
    let _ = snapshot;
    push_team_memory_with_retry(
        client,
        endpoint,
        repo_slug,
        token,
        team_dir,
        sync_state,
        &latest,
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
    if !team_memory_conflicts(team_dir)?.is_empty() {
        return Err("Team memory conflicts require review before upload".into());
    }
    validate_entries(&snapshot.entries)?;
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

    let body =
        serde_json::to_vec(&json!({"entries":changed_entries})).map_err(|e| e.to_string())?;
    if body.len() as u64 > MAX_PAYLOAD_BYTES {
        return Err("Team memory upload exceeds 4 MB".into());
    }
    let mut request = client
        .put(sync_endpoint(endpoint, repo_slug))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    if let Some(checksum) = &sync_state.last_known_checksum {
        request = request.header("If-Match", format!("\"{checksum}\""));
    } else {
        request = request.header("If-None-Match", "*");
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
    let payload = if response_status == reqwest::StatusCode::NO_CONTENT {
        etag
    } else {
        read_bounded_json::<TeamMemoryUploadPayload>(response)?
            .checksum
            .or(etag)
    };
    if payload.is_none() {
        return Err("Sync service must return a checksum or ETag after upload".into());
    }
    for (path, hash) in &snapshot.hashes {
        if changed_entries.contains_key(path) {
            sync_state
                .server_checksums
                .insert(path.clone(), hash.clone());
        }
    }
    sync_state.last_known_checksum = payload;
    opencowork_runtime::write_json_atomic(&team_dir.join(".sync-state.json"), &sync_state)
        .map_err(|e| e.to_string())?;
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
    for entry in WalkDir::new(team_dir)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.len() > MAX_FILE_SIZE_BYTES {
            return Err(std::io::Error::other("Team memory file exceeds 250 KB"));
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
        if snapshot.entries.len() > MAX_ENTRIES
            || snapshot
                .entries
                .values()
                .map(|v| v.len() as u64)
                .sum::<u64>()
                > MAX_PAYLOAD_BYTES
        {
            return Err(std::io::Error::other("Team memory snapshot exceeds limits"));
        }
    }
    Ok(snapshot)
}

fn read_bounded_json<T: serde::de::DeserializeOwned>(
    response: reqwest::blocking::Response,
) -> Result<T, String> {
    if response
        .content_length()
        .is_some_and(|size| size > MAX_PAYLOAD_BYTES)
    {
        return Err("Team memory response exceeds 4 MB".into());
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_PAYLOAD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_PAYLOAD_BYTES {
        return Err("Team memory response exceeds 4 MB".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
fn validate_entries(entries: &BTreeMap<String, String>) -> Result<(), String> {
    if entries.len() > MAX_ENTRIES
        || entries.values().map(|v| v.len() as u64).sum::<u64>() > MAX_PAYLOAD_BYTES
    {
        return Err("Team memory payload exceeds limits".into());
    }
    for (key, content) in entries {
        if !is_valid_entry_key(key) || content.len() as u64 > MAX_FILE_SIZE_BYTES {
            return Err(format!("Invalid or oversized team memory entry: {key}"));
        }
    }
    Ok(())
}
fn safe_team_path(root: &Path, key: &str) -> Result<PathBuf, String> {
    if !is_valid_entry_key(key) {
        return Err("Invalid team memory path".into());
    }
    let path = root.join(key);
    let base = root.canonicalize().map_err(|e| e.to_string())?;
    let mut ancestor = path.as_path();
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or("Invalid team memory parent")?;
    }
    if !ancestor
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(&base)
    {
        return Err("Team memory path escapes root".into());
    }
    Ok(path)
}
fn merge_remote_entries(
    root: &Path,
    entries: &BTreeMap<String, String>,
    base: &BTreeMap<String, String>,
) -> Result<usize, String> {
    validate_entries(entries)?;
    let _lease = opencowork_runtime::ExclusiveLease::acquire(&root.join(".sync.lock"))
        .map_err(|e| e.to_string())?;
    let mut conflicts = 0;
    for (key, remote) in entries {
        let path = safe_team_path(root, key)?;
        let local = match fs::metadata(&path) {
            Ok(_) => Some(read_limited_file(&path, MAX_FILE_SIZE_BYTES)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        let remote_hash = hash_content(remote);
        if local.as_deref() == Some(remote) {
            continue;
        }
        if let Some(local) = local {
            let local_hash = hash_content(&local);
            if base.get(key) == Some(&remote_hash) {
                continue;
            } // Remote unchanged; preserve unpushed local edits.
            if base.get(key) != Some(&local_hash) {
                let id = hash_content(key).replace("sha256:", "");
                opencowork_runtime::write_json_atomic(&root.join(".conflicts").join(format!("{id}.json")), &json!({"id":id,"path":key,"localHash":local_hash,"remoteHash":remote_hash,"remote":remote})).map_err(|e|e.to_string())?;
                conflicts += 1;
                continue;
            }
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        write_text_atomic(&path, remote)?;
    }
    Ok(conflicts)
}
pub fn team_memory_conflicts(root: &Path) -> Result<Vec<serde_json::Value>, String> {
    let dir = root.join(".conflicts");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .take(MAX_ENTRIES)
    {
        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(entry.path()).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let path = safe_team_path(root, value["path"].as_str().ok_or("Invalid conflict")?)?;
        let local = read_limited_file(&path, MAX_FILE_SIZE_BYTES).or_else(|e| {
            if !path.exists() {
                Ok(String::new())
            } else {
                Err(e)
            }
        })?;
        value["localHash"] = json!(hash_content(&local));
        value["local"] = json!(local);
        result.push(value);
    }
    Ok(result)
}
pub fn resolve_team_memory_conflict(
    root: &Path,
    id: &str,
    choice: &str,
    expected: &str,
    expected_local: &str,
) -> Result<(), String> {
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid conflict ID".into());
    }
    let _lease = opencowork_runtime::ExclusiveLease::acquire(&root.join(".sync.lock"))
        .map_err(|e| e.to_string())?;
    let record = root.join(".conflicts").join(format!("{id}.json"));
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&record).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if value["remoteHash"].as_str() != Some(expected) {
        return Err("Remote conflict changed; refresh before resolving".into());
    }
    let path = safe_team_path(root, value["path"].as_str().ok_or("Invalid conflict path")?)?;
    let local = if path.exists() {
        read_limited_file(&path, MAX_FILE_SIZE_BYTES)?
    } else {
        String::new()
    };
    if hash_content(&local) != expected_local {
        return Err("Local file changed; refresh the conflict before resolving".into());
    }
    match choice {
        "local" => {}
        "remote" => {
            write_text_atomic(
                &path,
                value["remote"].as_str().ok_or("Invalid remote content")?,
            )?;
        }
        _ => return Err("Choose local or remote".into()),
    }
    fs::remove_file(record).map_err(|e| e.to_string())
}

fn bind_remote_identity(state: &mut SyncState, endpoint: &str, repo: &str) {
    let identity = hash_content(&format!("{endpoint}\n{repo}"));
    if state.remote_identity.as_deref() != Some(identity.as_str()) {
        *state = SyncState {
            remote_identity: Some(identity),
            ..Default::default()
        };
    }
}

fn read_limited_file(path: &Path, limit: u64) -> Result<String, String> {
    let mut content = String::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_string(&mut content)
        .map_err(|e| e.to_string())?;
    if content.len() as u64 > limit {
        return Err("File exceeds size limit".into());
    }
    Ok(content)
}
fn write_text_atomic(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = path.with_extension(format!("pending-{}-{stamp}", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result.map_err(|e| e.to_string())
}

fn sync_endpoint(base: &str, repo_slug: &str) -> String {
    let separator = if base.contains('?') { '&' } else { '?' };
    let encoded: String = repo_slug
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    format!("{base}{separator}repo={encoded}")
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
        && !key.contains(['\\', ':'])
        && !key
            .split('/')
            .any(|part| part.starts_with('.') || part.is_empty())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "cowork-sync-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn entries(text: &str) -> BTreeMap<String, String> {
        BTreeMap::from([("notes.md".into(), text.into())])
    }
    fn server(response: String) -> (String, JoinHandle<String>) {
        server_many(vec![response])
    }
    fn server_many(responses: Vec<String>) -> (String, JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let mut requests = String::new();
            for response in responses {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut one = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    if socket.read(&mut one).unwrap() == 0 {
                        break;
                    }
                    bytes.push(one[0]);
                }
                let headers = String::from_utf8_lossy(&bytes).into_owned();
                let size: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|v| v.trim().parse().ok())
                    })
                    .unwrap_or(0);
                let mut body = vec![0; size];
                socket.read_exact(&mut body).unwrap();
                let _ = socket.write_all(response.as_bytes());
                requests.push_str(&String::from_utf8_lossy(&bytes));
            }
            requests
        });
        (url, handle)
    }
    fn client() -> Client {
        Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
    }

    #[test]
    fn remote_merge_preserves_local_edits_and_requires_explicit_resolution() {
        let f = Fixture::new();
        fs::write(f.0.join("notes.md"), "local").unwrap();
        let base = entries(&hash_content("original"));
        assert_eq!(
            merge_remote_entries(&f.0, &entries("remote"), &base).unwrap(),
            1
        );
        assert_eq!(fs::read_to_string(f.0.join("notes.md")).unwrap(), "local");
        let c = team_memory_conflicts(&f.0).unwrap().remove(0);
        assert!(resolve_team_memory_conflict(
            &f.0,
            c["id"].as_str().unwrap(),
            "remote",
            "stale",
            c["localHash"].as_str().unwrap()
        )
        .is_err());
        fs::write(f.0.join("notes.md"), "manual merge").unwrap();
        assert!(resolve_team_memory_conflict(
            &f.0,
            c["id"].as_str().unwrap(),
            "remote",
            c["remoteHash"].as_str().unwrap(),
            c["localHash"].as_str().unwrap()
        )
        .is_err());
        let c = team_memory_conflicts(&f.0).unwrap().remove(0);
        resolve_team_memory_conflict(
            &f.0,
            c["id"].as_str().unwrap(),
            "local",
            c["remoteHash"].as_str().unwrap(),
            c["localHash"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(f.0.join("notes.md")).unwrap(),
            "manual merge"
        );
        assert!(team_memory_conflicts(&f.0).unwrap().is_empty());
    }
    #[test]
    fn remote_resolution_and_unchanged_remote_are_safe() {
        let f = Fixture::new();
        fs::write(f.0.join("notes.md"), "local").unwrap();
        assert_eq!(
            merge_remote_entries(&f.0, &entries("old"), &entries(&hash_content("old"))).unwrap(),
            0
        );
        assert_eq!(fs::read_to_string(f.0.join("notes.md")).unwrap(), "local");
        merge_remote_entries(&f.0, &entries("new"), &entries(&hash_content("old"))).unwrap();
        let c = team_memory_conflicts(&f.0).unwrap().remove(0);
        resolve_team_memory_conflict(
            &f.0,
            c["id"].as_str().unwrap(),
            "remote",
            c["remoteHash"].as_str().unwrap(),
            c["localHash"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(f.0.join("notes.md")).unwrap(), "new");
    }
    #[test]
    fn rejects_oversized_and_unsafe_entries_before_writing() {
        let f = Fixture::new();
        for key in [
            "../outside.md",
            "sub/../../outside.md",
            "x:stream",
            ".conflicts/x",
            "x\\y",
            "/absolute",
        ] {
            assert!(!is_valid_entry_key(key));
        }
        let mut values = entries("valid");
        values.insert("z.md".into(), "x".repeat(MAX_FILE_SIZE_BYTES as usize + 1));
        assert!(merge_remote_entries(&f.0, &values, &BTreeMap::new()).is_err());
        assert!(!f.0.join("notes.md").exists());
        let many = (0..=MAX_ENTRIES)
            .map(|n| (format!("{n}.md"), String::new()))
            .collect();
        assert!(validate_entries(&many).is_err());
    }
    #[test]
    fn snapshot_excludes_private_sync_state_and_reports_large_files() {
        let f = Fixture::new();
        fs::write(f.0.join("notes.md"), "note").unwrap();
        fs::create_dir(f.0.join(".conflicts")).unwrap();
        fs::write(f.0.join(".conflicts/x.json"), "secret").unwrap();
        fs::write(f.0.join(".sync-state.json"), "private").unwrap();
        assert_eq!(scan_local_snapshot(&f.0).unwrap().entries, entries("note"));
        fs::write(
            f.0.join("big.md"),
            vec![b'x'; MAX_FILE_SIZE_BYTES as usize + 1],
        )
        .unwrap();
        assert!(scan_local_snapshot(&f.0).is_err());
    }
    #[test]
    fn pull_auth_failure_does_not_change_local_state() {
        for status in [401, 403] {
            let f = Fixture::new();
            fs::write(f.0.join("notes.md"), "local").unwrap();
            let (url, h) = server(format!(
                "HTTP/1.1 {status} Denied\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            ));
            let mut state = SyncState::default();
            let status = Arc::new(Mutex::new(TeamMemorySyncStatus::default()));
            assert!(pull_team_memory(
                &client(),
                &url,
                "owner/repo&other=bad",
                Some("test-token"),
                &f.0,
                &mut state,
                &status
            )
            .is_err());
            let request = h.join().unwrap();
            assert!(request.contains("repo=owner%2Frepo%26other%3Dbad"));
            assert!(request
                .to_lowercase()
                .contains("authorization: bearer test-token"));
            assert_eq!(fs::read_to_string(f.0.join("notes.md")).unwrap(), "local");
            assert!(state.server_checksums.is_empty());
        }
    }
    #[test]
    fn rejects_large_http_response_with_or_without_length() {
        for headers in [
            format!("Content-Length: {}\r\n", MAX_PAYLOAD_BYTES + 1),
            String::new(),
        ] {
            let (url, h) = server(format!(
                "HTTP/1.1 200 OK\r\n{headers}Connection: close\r\n\r\n{}",
                "x".repeat(MAX_PAYLOAD_BYTES as usize + 1)
            ));
            let response = client().get(url).send().unwrap();
            assert!(read_bounded_json::<serde_json::Value>(response)
                .unwrap_err()
                .contains("4 MB"));
            h.join().unwrap();
        }
    }
    #[test]
    fn precondition_conflict_pulls_without_overwriting_local_changes() {
        let f = Fixture::new();
        fs::write(f.0.join("notes.md"), "mine").unwrap();
        let body = serde_json::to_string(
            &json!({"checksum":"v2","content":{"entries":{"notes.md":"theirs"}}}),
        )
        .unwrap();
        let (url, h) = server_many(vec![
            "HTTP/1.1 412 Precondition Failed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .into(),
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        ]);
        let mut state = SyncState {
            remote_identity: None,
            last_known_checksum: Some("v1".into()),
            server_checksums: entries(&hash_content("original")),
        };
        let status = Arc::new(Mutex::new(TeamMemorySyncStatus::default()));
        let error = push_team_memory(
            &client(),
            &url,
            "repo",
            None,
            &f.0,
            &mut state,
            &LocalSnapshot::default(),
            &status,
        )
        .unwrap_err();
        assert!(error.contains("conflicts require review"));
        assert_eq!(fs::read_to_string(f.0.join("notes.md")).unwrap(), "mine");
        assert_eq!(team_memory_conflicts(&f.0).unwrap()[0]["remote"], "theirs");
        let requests = h.join().unwrap().to_lowercase();
        assert!(requests.contains("if-match: \"v1\""));
        assert!(requests.contains("if-none-match: \"v1\""));
    }

    #[test]
    fn switching_remote_does_not_reuse_another_repositories_base() {
        let mut state = SyncState::default();
        bind_remote_identity(&mut state, "https://sync.test", "first");
        state.last_known_checksum = Some("first-version".into());
        state.server_checksums = entries(&hash_content("first content"));
        bind_remote_identity(&mut state, "https://sync.test", "first");
        assert!(state.last_known_checksum.is_some());
        bind_remote_identity(&mut state, "https://sync.test", "second");
        assert!(state.last_known_checksum.is_none());
        assert!(state.server_checksums.is_empty());
    }

    #[test]
    fn sync_status_is_scoped_to_workspace() {
        let f = Fixture::new();
        let a = f.0.join("a");
        let b = f.0.join("b");
        let key = project_team_memory_root(&f.0, &a);
        let (sender, _) = mpsc::channel();
        service_slot().lock().unwrap().insert(
            key.clone(),
            TeamMemoryService {
                sender,
                status: Arc::new(Mutex::new(TeamMemorySyncStatus {
                    running: true,
                    repo_slug: Some("only-a".into()),
                    ..Default::default()
                })),
                handle: None,
            },
        );
        assert_eq!(
            team_memory_sync_status_for(&a, &f.0).repo_slug.as_deref(),
            Some("only-a")
        );
        assert!(!team_memory_sync_status_for(&b, &f.0).running);
        service_slot().lock().unwrap().remove(&key);
    }

    #[test]
    fn push_reads_latest_file_and_accepts_204_etag() {
        let f = Fixture::new();
        fs::write(f.0.join("notes.md"), "new").unwrap();
        let (url, h) = server(
            "HTTP/1.1 204 No Content\r\nETag: \"version2\"\r\nConnection: close\r\n\r\n".into(),
        );
        let mut state = SyncState::default();
        let status = Arc::new(Mutex::new(TeamMemorySyncStatus::default()));
        assert_eq!(
            push_team_memory(
                &client(),
                &url,
                "repo",
                None,
                &f.0,
                &mut state,
                &LocalSnapshot::default(),
                &status
            )
            .unwrap(),
            1
        );
        assert!(h
            .join()
            .unwrap()
            .to_lowercase()
            .contains("if-none-match: *"));
        assert_eq!(state.server_checksums["notes.md"], hash_content("new"));
        assert_eq!(state.last_known_checksum.as_deref(), Some("version2"));
    }

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

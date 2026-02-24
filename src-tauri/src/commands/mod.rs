use crate::capture::CaptureManager;
use crate::model::{is_transient_model_error, ChatWithToolsResult, ModelManager, ToolCall};
use crate::skills::{Skill, SkillFrontmatterOverrides, SkillManager, SkillMetadata, SkillsWatcher};
use crate::storage::{
    Config, SearchQuery, StorageConfig, StorageManager, SummaryRecord, TimeRange,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{Duration, Local, NaiveDateTime, TimeZone};
use glob::glob;
use quick_xml::events::Event;
use quick_xml::Reader;
use regex::{Regex, RegexBuilder};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::future::Future;
use std::io::{self, BufRead, BufReader};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_shell::ShellExt;
use tokio::process::Command as TokioCommand;
use tokio::sync::Mutex as TokioMutex;
use tokio::time::{sleep, timeout, Duration as TokioDuration};
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;
use zip::ZipArchive;

pub struct AppState {
    pub capture_manager: Arc<TokioMutex<CaptureManager>>,
    pub storage_manager: Arc<StorageManager>,
    pub request_cancellations: Arc<TokioMutex<HashMap<String, CancellationToken>>>,
    pub skills_watcher: Mutex<Option<SkillsWatcher>>,
    pub skills_version: Arc<AtomicU64>,
    pub skills_cache: Arc<TokioMutex<SkillsSnapshotCache>>,
}

#[derive(Default)]
pub struct SkillsSnapshotCache {
    pub initialized: bool,
    pub loaded_version: u64,
    pub skills: Vec<SkillMetadata>,
}

const MIN_RECENT_DETAIL_RECORDS: usize = 20;
const RELEASE_PAGE_URL: &str = "https://github.com/mypengpengli/OpenCowork/releases/latest";
const TOOL_MODE_UNSET_ERROR: &str = "TOOLS_MODE_UNSET";
const REQUEST_CANCELLED_ERROR: &str = "REQUEST_CANCELLED";
const TOOL_ERROR_PREFIX: &str = "TOOL_ERROR:";
const MAX_TOOL_LOOPS: usize = 999;
const MAX_REPEAT_TOOL_LOOPS: usize = 3;
const MODEL_MAX_RETRIES: usize = 2;
const MODEL_MAX_CONTINUES: usize = 1;
const MIN_HISTORY_MESSAGES_BEFORE_COMPRESSION: usize = 14;
const MAX_PERSISTED_TOOL_CONTEXT_CHARS: usize = 3000;
static BACKGROUND_TASK_COUNTER: AtomicU64 = AtomicU64::new(1);

const DEFAULT_MAX_READ_BYTES: usize = 200_000;
const DEFAULT_MAX_GLOB_RESULTS: usize = 500;
const DEFAULT_MAX_GREP_RESULTS: usize = 200;
const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_AGENT_BROWSER_TIMEOUT_MS: u64 = 20_000;
const MAX_COMMAND_TIMEOUT_MS: u64 = 900_000;
const MAX_COMMAND_OUTPUT_CHARS: usize = 20_000;
const MAX_GREP_FILE_BYTES: u64 = 2_000_000;

impl AppState {
    pub fn new() -> Self {
        Self {
            capture_manager: Arc::new(TokioMutex::new(CaptureManager::new())),
            storage_manager: Arc::new(StorageManager::new()),
            request_cancellations: Arc::new(TokioMutex::new(HashMap::new())),
            skills_watcher: Mutex::new(None),
            skills_version: Arc::new(AtomicU64::new(1)),
            skills_cache: Arc::new(TokioMutex::new(SkillsSnapshotCache::default())),
        }
    }

    pub fn bump_skills_version(&self) -> u64 {
        self.skills_version.fetch_add(1, Ordering::SeqCst) + 1
    }
}

#[tauri::command]
pub async fn get_config() -> Result<Config, String> {
    let storage = StorageManager::new();
    storage.load_config().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_system_locale(
    ui_locale: Option<String>,
    stored_locale: Option<String>,
    stored_version: Option<String>,
) -> Result<String, String> {
    println!(
        "[locale] ui_locale={} stored_locale={} stored_version={}",
        ui_locale.as_deref().unwrap_or(""),
        stored_locale.as_deref().unwrap_or(""),
        stored_version.as_deref().unwrap_or("")
    );
    #[cfg(target_os = "windows")]
    {
        match windows_ui_is_zh() {
            Some(is_zh) => {
                let resolved = if is_zh {
                    "zh".to_string()
                } else {
                    "en".to_string()
                };
                println!(
                    "[locale] get_system_locale windows_ui_is_zh={} -> {}",
                    is_zh, resolved
                );
                return Ok(resolved);
            }
            None => {
                println!("[locale] get_system_locale windows_ui_is_zh=None");
            }
        }
    }

    let fallback = sys_locale::get_locale().unwrap_or_default();
    println!("[locale] get_system_locale fallback -> {}", fallback);
    Ok(fallback)
}

#[tauri::command]
pub async fn log_ui_locale(
    ui_locale: String,
    stored_locale: Option<String>,
    stored_version: Option<String>,
) -> Result<(), String> {
    println!(
        "[locale] ui_locale={} stored_locale={} stored_version={}",
        ui_locale,
        stored_locale.unwrap_or_default(),
        stored_version.unwrap_or_default()
    );
    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_ui_is_zh() -> Option<bool> {
    let lang_id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    if lang_id == 0 {
        return None;
    }
    let primary_lang = lang_id & 0x3ff;
    Some(primary_lang == 0x04)
}

#[tauri::command]
pub async fn save_config(config: Config) -> Result<(), String> {
    let storage = StorageManager::new();
    storage.save_config(&config).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_profiles() -> Result<Vec<String>, String> {
    let storage = StorageManager::new();
    storage.list_profiles().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_profile(name: String, config: Config) -> Result<(), String> {
    let storage = StorageManager::new();
    storage
        .save_profile(&name, &config)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn load_profile(name: String) -> Result<Config, String> {
    let storage = StorageManager::new();
    storage.load_profile(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_profile(name: String) -> Result<(), String> {
    let storage = StorageManager::new();
    storage.delete_profile(&name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_model_connection(config: Config) -> Result<(), String> {
    let model_manager = ModelManager::new();
    model_manager.test_connection(&config.model).await
}

#[tauri::command]
pub async fn start_capture(
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let storage = StorageManager::new();
    let config = storage.load_config().map_err(|e| e.to_string())?;

    let mut manager = state.capture_manager.lock().await;
    manager.start(config, app_handle).await;
    Ok(())
}

#[tauri::command]
pub async fn stop_capture(state: State<'_, AppState>) -> Result<(), String> {
    let mut manager = state.capture_manager.lock().await;
    manager.stop().await;
    Ok(())
}

#[tauri::command]
pub async fn get_capture_status(state: State<'_, AppState>) -> Result<CaptureStatus, String> {
    let manager = state.capture_manager.lock().await;
    Ok(CaptureStatus {
        is_capturing: manager.is_running(),
        record_count: manager.get_count(),
        last_capture_time: None,
    })
}

#[tauri::command]
pub async fn cancel_request(state: State<'_, AppState>, request_id: String) -> Result<(), String> {
    let token = {
        let map = state.request_cancellations.lock().await;
        map.get(&request_id).cloned()
    };
    if let Some(token) = token {
        token.cancel();
    }
    Ok(())
}

#[derive(serde::Serialize)]
pub struct CaptureStatus {
    pub is_capturing: bool,
    pub record_count: u64,
    pub last_capture_time: Option<String>,
}

#[derive(serde::Deserialize, Clone)]
pub struct ChatHistoryMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCallInfo>>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct ToolCallInfo {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(serde::Serialize)]
pub struct ChatResponse {
    pub response: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_context: Vec<ToolContextMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_skill: Option<String>,
}

#[derive(serde::Serialize, Clone)]
pub struct ToolContextMessage {
    pub role: String,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallInfo>>,
}

#[derive(serde::Deserialize, Clone)]
pub struct AttachmentInput {
    pub path: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Default)]
struct AttachmentPayload {
    text: String,
    image_urls: Vec<String>,
    image_base64: Vec<String>,
}

#[tauri::command]
pub async fn save_clipboard_image(base64: String, name: Option<String>) -> Result<String, String> {
    let storage = StorageManager::new();
    let data_dir = storage.get_data_dir().to_path_buf();
    let attachments_dir = data_dir.join("attachments");
    fs::create_dir_all(&attachments_dir).map_err(|e| format!("创建附件目录失败: {}", e))?;

    let mut filename = sanitize_attachment_name(name.as_deref());
    if filename.is_empty() {
        filename = format!("clipboard-{}.png", Local::now().timestamp_millis());
    } else if Path::new(&filename).extension().is_none() {
        filename.push_str(".png");
    }

    let encoded = match base64.find("base64,") {
        Some(idx) => &base64[(idx + "base64,".len())..],
        None => base64.as_str(),
    };
    let encoded = encoded.trim();
    if encoded.is_empty() {
        return Err("剪贴板图片为空".to_string());
    }

    let bytes = BASE64
        .decode(encoded.as_bytes())
        .map_err(|e| format!("解码剪贴板图片失败: {}", e))?;
    if bytes.len() > MAX_ATTACHMENT_BYTES as usize {
        return Err("剪贴板图片过大，超过 5MB 限制".to_string());
    }

    let mut file_path = attachments_dir.join(&filename);
    if file_path.exists() {
        let stem = Path::new(&filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("clipboard");
        let ext = Path::new(&filename)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("png");
        let mut counter = 1u32;
        loop {
            let candidate = attachments_dir.join(format!("{}-{}.{}", stem, counter, ext));
            if !candidate.exists() {
                file_path = candidate;
                break;
            }
            counter += 1;
        }
    }

    fs::write(&file_path, bytes).map_err(|e| format!("保存剪贴板图片失败: {}", e))?;
    Ok(file_path.to_string_lossy().to_string())
}

#[derive(serde::Serialize, Clone)]
struct ProgressEvent {
    request_id: String,
    stage: String,
    message: String,
    detail: Option<String>,
    timestamp: String,
}

#[derive(Clone)]
struct ProgressEmitter {
    app_handle: AppHandle,
    request_id: String,
    enabled: bool,
}

impl ProgressEmitter {
    fn new(app_handle: &AppHandle, enabled: bool, request_id: Option<String>) -> Option<Self> {
        if !enabled {
            return None;
        }
        let request_id =
            request_id.unwrap_or_else(|| format!("req-{}", Local::now().timestamp_millis()));
        Some(Self {
            app_handle: app_handle.clone(),
            request_id,
            enabled: true,
        })
    }

    fn emit(&self, stage: &str, message: String, detail: Option<String>) {
        if !self.enabled {
            return;
        }
        let event = ProgressEvent {
            request_id: self.request_id.clone(),
            stage: stage.to_string(),
            message,
            detail,
            timestamp: Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
        };
        let _ = self.app_handle.emit("assistant-progress", event);
    }

    fn emit_start(&self, message: &str) {
        self.emit("start", message.to_string(), None);
    }

    fn emit_info(&self, message: String, detail: Option<String>) {
        self.emit("info", message, detail);
    }

    fn emit_step(&self, message: String, detail: Option<String>) {
        self.emit("step", message, detail);
    }

    fn emit_done(&self, message: &str) {
        self.emit("done", message.to_string(), None);
    }

    fn emit_error(&self, message: &str) {
        self.emit("error", message.to_string(), None);
    }
}

async fn register_cancel_token(state: &State<'_, AppState>, request_id: &str) -> CancellationToken {
    let mut map = state.request_cancellations.lock().await;
    map.entry(request_id.to_string())
        .or_insert_with(CancellationToken::new)
        .clone()
}

async fn clear_cancel_token(state: &State<'_, AppState>, request_id: &str) {
    let mut map = state.request_cancellations.lock().await;
    map.remove(request_id);
}

async fn get_available_skills_cached(
    state: &State<'_, AppState>,
    skill_manager: &SkillManager,
) -> Vec<SkillMetadata> {
    let version = state.skills_version.load(Ordering::SeqCst);
    {
        let cache = state.skills_cache.lock().await;
        if cache.initialized && cache.loaded_version == version {
            return cache.skills.clone();
        }
    }

    let discovered = skill_manager.discover_skills().unwrap_or_default();
    let mut cache = state.skills_cache.lock().await;
    cache.initialized = true;
    cache.loaded_version = version;
    cache.skills = discovered.clone();
    discovered
}

fn check_cancel(cancel_token: Option<&CancellationToken>) -> Result<(), String> {
    if let Some(token) = cancel_token {
        if token.is_cancelled() {
            return Err(REQUEST_CANCELLED_ERROR.to_string());
        }
    }
    Ok(())
}

async fn await_with_cancel<T, F>(token: &CancellationToken, fut: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    tokio::select! {
        _ = token.cancelled() => Err(REQUEST_CANCELLED_ERROR.to_string()),
        result = fut => result,
    }
}

fn should_retry_model_error(err: &str) -> bool {
    if err == REQUEST_CANCELLED_ERROR || err == TOOL_MODE_UNSET_ERROR {
        return false;
    }
    is_transient_model_error(err)
}

async fn retry_with_cancel<T, F, Fut>(
    token: &CancellationToken,
    progress: Option<&ProgressEmitter>,
    label: &str,
    mut make_fut: F,
) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let mut attempt = 0usize;
    loop {
        let result = await_with_cancel(token, make_fut()).await;
        match result {
            Ok(value) => return Ok(value),
            Err(err) => {
                if err == REQUEST_CANCELLED_ERROR {
                    return Err(err);
                }
                attempt += 1;
                if attempt > MODEL_MAX_RETRIES || !should_retry_model_error(&err) {
                    return Err(err);
                }
                if let Some(progress) = progress {
                    progress.emit_info(
                        format!("Retrying {} ({}/{})", label, attempt, MODEL_MAX_RETRIES),
                        Some(err.clone()),
                    );
                }
                sleep(TokioDuration::from_millis(400 * attempt as u64)).await;
            }
        }
    }
}

fn response_looks_incomplete(text: &str) -> bool {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return true;
    }
    let short = trimmed.len() < 400;
    let ends_with_colon = trimmed.ends_with(':') || trimmed.ends_with('?');
    let ends_with_ellipsis =
        trimmed.ends_with("...") || trimmed.ends_with('?') || trimmed.ends_with("??");
    let unbalanced_fence = trimmed.matches("```").count() % 2 == 1;
    let lower = trimmed.to_lowercase();
    let engine_hint = lower.contains("??????")
        || lower.contains("engine error")
        || lower.contains("internal error")
        || lower.contains("temporary error")
        || lower.contains("service error")
        || lower.contains("????")
        || lower.contains("try another way");

    (short && (ends_with_colon || ends_with_ellipsis)) || unbalanced_fence || engine_hint
}

fn estimate_text_tokens(text: &str) -> usize {
    let mut ascii_chars = 0usize;
    let mut non_ascii_chars = 0usize;
    for ch in text.chars() {
        if ch.is_ascii() {
            ascii_chars += 1;
        } else {
            non_ascii_chars += 1;
        }
    }
    (ascii_chars + 3) / 4 + non_ascii_chars + 1
}

fn estimate_history_tokens(
    system_prompt: &str,
    user_message: &str,
    history: &[ChatHistoryMessage],
) -> usize {
    let mut total = estimate_text_tokens(system_prompt) + estimate_text_tokens(user_message) + 24;
    for msg in history {
        total += estimate_text_tokens(&msg.content) + 8;
    }
    total
}

fn build_history_compression_summary(history: &[ChatHistoryMessage], max_chars: usize) -> String {
    let mut summary = String::from("Context compression summary of earlier conversation:\n");
    let mut used = summary.chars().count();

    for (idx, msg) in history.iter().enumerate() {
        if idx >= 80 {
            summary.push_str("- ...(more omitted)\n");
            break;
        }
        let role = if msg.role.eq_ignore_ascii_case("assistant") {
            "assistant"
        } else if msg.role.eq_ignore_ascii_case("system") {
            "system"
        } else {
            "user"
        };
        let compact = msg.content.split_whitespace().collect::<Vec<_>>().join(" ");
        let (snippet, truncated) = truncate_string(&compact, 220);
        let mut line = format!("- {}: {}", role, snippet);
        if truncated {
            line.push_str(" ...");
        }
        line.push('\n');

        let line_chars = line.chars().count();
        if used + line_chars > max_chars {
            summary.push_str("- ...(more omitted)\n");
            break;
        }
        summary.push_str(&line);
        used += line_chars;
    }

    summary
}

fn compress_history_if_needed(
    history: Option<Vec<ChatHistoryMessage>>,
    system_prompt: &str,
    user_message: &str,
    storage: &StorageConfig,
    progress: Option<&ProgressEmitter>,
) -> Option<Vec<ChatHistoryMessage>> {
    let history = history?;
    if history.len() <= 2 {
        return Some(history);
    }
    // Align with mainstream agent behavior: avoid eager compaction on short chats.
    // Keep full history for early turns and only compact once conversation is truly long.
    if history.len() < MIN_HISTORY_MESSAGES_BEFORE_COMPRESSION {
        return Some(history);
    }

    let max_context_tokens = storage.max_context_tokens.max(4096);
    let trigger_ratio = storage.context_compress_trigger_ratio.clamp(0.70, 0.99);
    let trigger_tokens = ((max_context_tokens as f32) * trigger_ratio).floor() as usize;

    let before_tokens = estimate_history_tokens(system_prompt, user_message, &history);
    if before_tokens <= trigger_tokens {
        return Some(history);
    }

    let keep_recent = history.len().min(12);
    let split_idx = history.len().saturating_sub(keep_recent);
    let older = &history[..split_idx];
    let recent = &history[split_idx..];
    let mut compressed = Vec::new();
    let has_summary = !older.is_empty();
    if has_summary {
        compressed.push(ChatHistoryMessage {
            role: "assistant".to_string(),
            content: build_history_compression_summary(older, 6000),
            tool_call_id: None,
            tool_calls: None,
        });
    }
    compressed.extend(recent.iter().cloned());

    let target_ratio = (trigger_ratio - 0.08).max(0.70);
    let target_tokens = ((max_context_tokens as f32) * target_ratio).floor() as usize;

    let mut loops = 0usize;
    while estimate_history_tokens(system_prompt, user_message, &compressed) > target_tokens
        && compressed.len() > 4
        && loops < 128
    {
        let remove_idx = if has_summary && compressed.len() > 1 {
            1
        } else {
            0
        };
        compressed.remove(remove_idx);
        loops += 1;
    }

    if has_summary && !compressed.is_empty() {
        let mut summary_limit = 3000usize;
        while estimate_history_tokens(system_prompt, user_message, &compressed) > target_tokens
            && summary_limit > 600
        {
            let (shortened, truncated) = truncate_string(&compressed[0].content, summary_limit);
            compressed[0].content = if truncated {
                format!("{}\n...(summary truncated)", shortened)
            } else {
                shortened
            };
            summary_limit = ((summary_limit as f32) * 0.75) as usize;
        }
    }

    while estimate_history_tokens(system_prompt, user_message, &compressed) > trigger_tokens
        && compressed.len() > 2
    {
        let remove_idx = if has_summary && compressed.len() > 1 {
            1
        } else {
            0
        };
        compressed.remove(remove_idx);
    }

    let after_tokens = estimate_history_tokens(system_prompt, user_message, &compressed);
    if let Some(progress) = progress {
        progress.emit_info(
            "Context compression activated".to_string(),
            Some(format!(
                "history {} -> {} messages, est tokens {} -> {} (limit {}, trigger {}%)",
                history.len(),
                compressed.len(),
                before_tokens,
                after_tokens,
                max_context_tokens,
                (trigger_ratio * 100.0).round() as u32
            )),
        );
    }

    Some(compressed)
}

fn is_context_overflow_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("context_length_exceeded")
        || lower.contains("context length")
        || lower.contains("context window")
        || lower.contains("maximum context")
        || lower.contains("too many tokens")
        || lower.contains("token limit")
        || lower.contains("prompt is too long")
        || lower.contains("input is too long")
        || lower.contains("improperly formed request")
        || lower.contains("bad request")
}

fn squeeze_history_keep_recent(
    history: &Option<Vec<ChatHistoryMessage>>,
    keep_recent: usize,
    summary_chars: Option<usize>,
    truncate_each_to: Option<usize>,
) -> Option<Vec<ChatHistoryMessage>> {
    let history = history.as_ref()?.clone();
    if history.len() <= keep_recent {
        return Some(history);
    }
    let split_idx = history.len().saturating_sub(keep_recent);
    let older = &history[..split_idx];
    let recent = &history[split_idx..];
    let mut squeezed = Vec::new();
    if let Some(max_chars) = summary_chars {
        if !older.is_empty() {
            squeezed.push(ChatHistoryMessage {
                role: "assistant".to_string(),
                content: build_history_compression_summary(older, max_chars),
                tool_call_id: None,
                tool_calls: None,
            });
        }
    }
    for msg in recent {
        let mut cloned = msg.clone();
        if let Some(max_chars) = truncate_each_to {
            let (shortened, truncated) = truncate_string(&cloned.content, max_chars);
            cloned.content = if truncated {
                format!("{}\n...(message truncated)", shortened)
            } else {
                shortened
            };
        }
        squeezed.push(cloned);
    }
    Some(squeezed)
}

fn build_overflow_recovery_histories(
    history: &Option<Vec<ChatHistoryMessage>>,
    system_prompt: &str,
    user_message: &str,
    storage: &StorageConfig,
) -> Vec<Option<Vec<ChatHistoryMessage>>> {
    let mut candidates = Vec::new();
    candidates.push(history.clone());
    if history.is_none() {
        return candidates;
    }

    let mut aggressive_storage = storage.clone();
    aggressive_storage.context_compress_trigger_ratio = storage
        .context_compress_trigger_ratio
        .clamp(0.70, 0.99)
        .min(0.82);
    aggressive_storage.max_context_tokens = storage.max_context_tokens.max(4096);
    let aggressive = compress_history_if_needed(
        history.clone(),
        system_prompt,
        user_message,
        &aggressive_storage,
        None,
    );
    candidates.push(squeeze_history_keep_recent(
        &aggressive,
        8,
        Some(1800),
        Some(2800),
    ));
    candidates.push(squeeze_history_keep_recent(history, 4, None, Some(1500)));
    candidates
}

#[derive(serde::Deserialize)]
struct ReadArgs {
    path: String,
    #[serde(default)]
    max_bytes: Option<usize>,
}

#[derive(serde::Deserialize)]
struct WriteArgs {
    path: String,
    content: String,
    #[serde(default)]
    append: Option<bool>,
}

#[derive(serde::Deserialize)]
struct EditArgs {
    path: String,
    old: String,
    new: String,
    #[serde(default)]
    replace_all: Option<bool>,
}

#[derive(serde::Deserialize)]
struct GlobArgs {
    pattern: String,
    #[serde(default)]
    max_results: Option<usize>,
}

#[derive(serde::Deserialize)]
struct GrepArgs {
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    glob: Option<String>,
    #[serde(default)]
    regex: Option<bool>,
    #[serde(default)]
    case_sensitive: Option<bool>,
    #[serde(default)]
    max_results: Option<usize>,
}

#[derive(serde::Deserialize)]
struct BashArgs {
    command: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Clone)]
struct ToolAccess {
    mode: String,
    allowed_commands: Vec<String>,
    allowed_dirs: Vec<PathBuf>,
    base_dir: PathBuf,
    tasks_dir: PathBuf,
}

#[tauri::command]
pub async fn chat_with_assistant(
    message: String,
    history: Option<Vec<ChatHistoryMessage>>,
    attachments: Option<Vec<AttachmentInput>>,
    request_id: Option<String>,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let storage = StorageManager::new();
    let config = storage.load_config().map_err(|e| e.to_string())?;
    let model_manager = ModelManager::new();
    let skill_manager = SkillManager::new();

    // 获取可用 skills 列表（用于自动发现和 Tool Use）
    let available_skills = get_available_skills_cached(&state, &skill_manager).await;

    // 分析用户问题，提取时间范围和关键词
    let use_context = should_use_screen_context(&config.storage.context_mode, &message);
    let detail_cutoff = build_detail_cutoff(&config);
    let context = if use_context {
        // 分析用户问题，提取时间范围和关键词
        let query = parse_user_query(&message);

        // 智能检索相关记录
        let mut search_result = storage.smart_search(&query)?;

        if search_result.records.is_empty() && !query.keywords.is_empty() {
            let mut relaxed = query.clone();
            relaxed.keywords.clear();
            if let Ok(relaxed_result) = storage.smart_search(&relaxed) {
                if !relaxed_result.records.is_empty() || !relaxed_result.aggregated.is_empty() {
                    search_result = relaxed_result;
                }
            }
        }

        if matches!(query.time_range, TimeRange::Recent(_))
            && search_result.records.len() < MIN_RECENT_DETAIL_RECORDS
        {
            let mut fallback = storage
                .get_recent_records(MIN_RECENT_DETAIL_RECORDS, config.storage.retention_days);
            if let Some(ref cutoff) = detail_cutoff {
                fallback.retain(|record| record.timestamp >= *cutoff);
            }
            if !fallback.is_empty() {
                search_result.records = merge_recent_records(
                    search_result.records,
                    fallback,
                    MIN_RECENT_DETAIL_RECORDS,
                );
            }
        }

        let include_detail = query.include_detail && config.storage.context_detail_hours != 0;
        // 构建上下文（使用配置中的最大字符数）
        let context = search_result.build_context(
            config.storage.max_context_chars,
            include_detail,
            detail_cutoff.as_deref(),
        );

        // 注入启用的全局提示词
        build_context_with_global_prompts(&config, context)
    } else {
        build_context_with_global_prompts(&config, String::new())
    };

    // 处理附件内容
    let attachment_payload = attachments
        .as_deref()
        .map(build_attachment_payload)
        .unwrap_or_default();
    let has_attachments = attachments
        .as_ref()
        .map_or(false, |items| !items.is_empty());
    let user_message = merge_user_message(&message, &attachment_payload.text, has_attachments);
    let inherited_skill_block = extract_latest_skill_instructions_block(history.as_ref());

    let request_id =
        request_id.unwrap_or_else(|| format!("req-{}", Local::now().timestamp_millis()));
    let cancel_token = register_cancel_token(&state, &request_id).await;
    let progress = ProgressEmitter::new(
        &app_handle,
        config.ui.show_progress,
        Some(request_id.clone()),
    );

    let response = (async {
        let response = if config.model.provider == "api" {
        let system_prompt = build_tool_system_prompt(&context, skill_manager.get_skills_dir(), &available_skills);
        let system_prompt =
            apply_skill_block_to_system_prompt(&system_prompt, inherited_skill_block.as_deref());
        let mut model_history = compress_history_if_needed(
            history.clone(),
            &system_prompt,
            &user_message,
            &config.storage,
            progress.as_ref(),
        );
        if let Some(ref progress) = progress {
            progress.emit_start("开始处理请求");
            progress.emit_info("Analyze request & plan".to_string(), None);
        }
        let history_candidates = build_overflow_recovery_histories(
            &model_history,
            &system_prompt,
            &user_message,
            &config.storage,
        );
        let total_candidates = history_candidates.len();
        let mut result: Option<ChatWithToolsResult> = None;
        let mut last_error: Option<String> = None;
        for (idx, candidate_history) in history_candidates.into_iter().enumerate() {
            let attempt = if attachment_payload.image_urls.is_empty()
                && attachment_payload.image_base64.is_empty()
            {
                let history_for_call = candidate_history.clone();
                retry_with_cancel(
                    &cancel_token,
                    progress.as_ref(),
                    "model",
                    || model_manager.chat_with_tools_with_system_prompt(
                        &config.model,
                        &system_prompt,
                        &user_message,
                        history_for_call.clone(),
                        &available_skills,
                    ),
                )
                .await
            } else {
                let history_for_call = candidate_history.clone();
                retry_with_cancel(
                    &cancel_token,
                    progress.as_ref(),
                    "model",
                    || model_manager.chat_with_tools_with_system_prompt_with_images(
                        &config.model,
                        &system_prompt,
                        &user_message,
                        history_for_call.clone(),
                        &available_skills,
                        attachment_payload.image_urls.clone(),
                        attachment_payload.image_base64.clone(),
                    ),
                )
                .await
            };

            match attempt {
                Ok(value) => {
                    result = Some(value);
                    break;
                }
                Err(err) => {
                    let can_retry = idx + 1 < total_candidates && is_context_overflow_error(&err);
                    if can_retry {
                        if let Some(ref progress) = progress {
                            progress.emit_info(
                                "Context overflow detected; retrying with tighter history".to_string(),
                                Some(format!("attempt {}/{}", idx + 2, total_candidates)),
                            );
                        }
                        last_error = Some(err);
                        continue;
                    }
                    return Err(err);
                }
            }
        }
        let result = if let Some(value) = result {
            value
        } else {
            return Err(last_error.unwrap_or_else(|| "model request failed".to_string()));
        };

        let tool_loop_result = run_tool_loop(
            &storage,
            &config,
            &model_manager,
            &skill_manager,
            &system_prompt,
            result,
            &available_skills,
            &None,
            None,
            Some(&cancel_token),
            progress.as_ref(),
        )
        .await;
        let (response, mut tool_context) = if let Ok(result) = tool_loop_result {
            let mut combined = result.response;
            let mut combined_context = result.tool_context;
            if MODEL_MAX_CONTINUES > 0 && response_looks_incomplete(&combined) {
                if let Some(ref progress) = progress {
                    progress.emit_info("Continuing incomplete response".to_string(), None);
                }
                let mut extended_history = model_history.clone().unwrap_or_default();
                extended_history.push(ChatHistoryMessage {
                    role: "user".to_string(),
                    content: user_message.clone(),
                    tool_call_id: None,
                    tool_calls: None,
                });
                extended_history.push(ChatHistoryMessage {
                    role: "assistant".to_string(),
                    content: combined.clone(),
                    tool_call_id: None,
                    tool_calls: None,
                });

                let followup = if attachment_payload.image_urls.is_empty()
                    && attachment_payload.image_base64.is_empty()
                {
                    retry_with_cancel(
                        &cancel_token,
                        progress.as_ref(),
                        "continue",
                        || model_manager.chat_with_tools_with_system_prompt(
                            &config.model,
                            &system_prompt,
                            "Continue the previous response.",
                            Some(extended_history.clone()),
                            &available_skills,
                        ),
                    )
                    .await
                } else {
                    retry_with_cancel(
                        &cancel_token,
                        progress.as_ref(),
                        "continue",
                        || model_manager.chat_with_tools_with_system_prompt_with_images(
                            &config.model,
                            &system_prompt,
                            "Continue the previous response.",
                            Some(extended_history.clone()),
                            &available_skills,
                            attachment_payload.image_urls.clone(),
                            attachment_payload.image_base64.clone(),
                        ),
                    )
                    .await
                };

                if let Ok(followup_result) = followup {
                    if let Ok(followup_loop_result) = run_tool_loop(
                        &storage,
                        &config,
                        &model_manager,
                        &skill_manager,
                        &system_prompt,
                        followup_result,
                        &available_skills,
                        &None,
                        None,
                        Some(&cancel_token),
                        progress.as_ref(),
                    )
                    .await
                    {
                        if !followup_loop_result.response.trim().is_empty() {
                            combined = format!(
                                "{}
{}",
                                combined.trim_end(),
                                followup_loop_result.response.trim_start()
                            );
                        }
                        combined_context.extend(followup_loop_result.tool_context);
                    }
                }
            }
            (Ok(combined), combined_context)
        } else {
            (tool_loop_result.map(|r| r.response), Vec::new())
        };
if let Some(ref progress) = progress {
            if response.is_ok() {
                progress.emit_done("处理完成");
            } else {
                progress.emit_error("处理失败");
            }
        }
        // 返回 JSON 格式的响应
        match response {
            Ok(text) => {
                let chat_response = ChatResponse {
                    response: text,
                    tool_context,
                    active_skill: None,
                };
                Ok(serde_json::to_string(&chat_response).unwrap_or_else(|_| chat_response.response))
            }
            Err(e) => Err(e),
        }
    } else {
        if let Some(ref progress) = progress {
            progress.emit_start("Begin processing request");
            progress.emit_info("Analyze request & plan".to_string(), None);
        }
        let skills_hint = if !available_skills.is_empty() {
            let skills_list: Vec<String> = available_skills
                .iter()
                .filter(|s| is_model_invocable_skill(s))
                .map(|s| format!("- /{}: {}", s.name, s.description))
                .collect();

            if skills_list.is_empty() {
                String::new()
            } else {
                format!(
                    "\n\n## 可用技能\n用户可以使用以下技能（输入 /技能名 调用）：\n{}\n\n如果用户的请求与某个技能相关，你可以建议用户使用该技能。",
                    skills_list.join("\n")
                )
            }
        } else {
            String::new()
        };

        let context_with_skills = format!("{}{}", context, skills_hint);
        let context_with_skills =
            apply_skill_block_to_system_prompt(&context_with_skills, inherited_skill_block.as_deref());
        let model_history = compress_history_if_needed(
            history.clone(),
            &context_with_skills,
            &user_message,
            &config.storage,
            progress.as_ref(),
        );
        let response = if attachment_payload.image_urls.is_empty()
            && attachment_payload.image_base64.is_empty()
        {
            retry_with_cancel(
                &cancel_token,
                progress.as_ref(),
                "model",
                || model_manager.chat_with_history(
                    &config.model,
                    &context_with_skills,
                    &user_message,
                    model_history.clone(),
                ),
            )
            .await
        } else {
            retry_with_cancel(
                &cancel_token,
                progress.as_ref(),
                "model",
                || model_manager.chat_with_history_with_images(
                    &config.model,
                    &context_with_skills,
                    &user_message,
                    model_history.clone(),
                    attachment_payload.image_urls.clone(),
                    attachment_payload.image_base64.clone(),
                ),
            )
            .await
        };
        let response = if let Ok(text) = response {
            let mut combined = text;
            if MODEL_MAX_CONTINUES > 0 && response_looks_incomplete(&combined) {
                if let Some(ref progress) = progress {
                    progress.emit_info("Continuing incomplete response".to_string(), None);
                }
                let mut extended_history = model_history.clone().unwrap_or_default();
                extended_history.push(ChatHistoryMessage {
                    role: "user".to_string(),
                    content: user_message.clone(),
                    tool_call_id: None,
                    tool_calls: None,
                });
                extended_history.push(ChatHistoryMessage {
                    role: "assistant".to_string(),
                    content: combined.clone(),
                    tool_call_id: None,
                    tool_calls: None,
                });
                let followup = if attachment_payload.image_urls.is_empty()
                    && attachment_payload.image_base64.is_empty()
                {
                    retry_with_cancel(
                        &cancel_token,
                        progress.as_ref(),
                        "continue",
                        || model_manager.chat_with_history(
                            &config.model,
                            &context_with_skills,
                            "Continue the previous response.",
                            Some(extended_history.clone()),
                        ),
                    )
                    .await
                } else {
                    retry_with_cancel(
                        &cancel_token,
                        progress.as_ref(),
                        "continue",
                        || model_manager.chat_with_history_with_images(
                            &config.model,
                            &context_with_skills,
                            "Continue the previous response.",
                            Some(extended_history.clone()),
                            attachment_payload.image_urls.clone(),
                            attachment_payload.image_base64.clone(),
                        ),
                    )
                    .await
                };

                if let Ok(followup_text) = followup {
                    if !followup_text.trim().is_empty() {
                        combined = format!(
                            "{}
{}",
                            combined.trim_end(),
                            followup_text.trim_start()
                        );
                    }
                }
            }
            Ok(combined)
        } else {
            response
        };
if let Some(ref progress) = progress {
            if response.is_ok() {
                progress.emit_done("处理完成");
            } else {
                progress.emit_error("处理失败");
            }
        }
        response
        };
        response
    })
    .await;
    clear_cancel_token(&state, &request_id).await;
    response
}

/// 内部执行 skill 的函数
async fn execute_skill_internal(
    storage: &StorageManager,
    config: &Config,
    model_manager: &ModelManager,
    skill_manager: &SkillManager,
    skill_name: &str,
    args: Option<String>,
    history: Option<Vec<ChatHistoryMessage>>,
    attachments: Option<Vec<AttachmentInput>>,
    cancel_token: Option<&CancellationToken>,
    progress: Option<&ProgressEmitter>,
) -> Result<String, String> {
    // 加载 skill
    let skill = skill_manager.load_skill(skill_name)?;
    let rendered_instructions = inject_skill_arguments(&skill.instructions, args.as_deref());
    check_cancel(cancel_token)?;
    if let Some(progress) = progress {
        progress.emit_info("Loaded skill file".to_string(), Some(skill.path.clone()));
    }

    let skill_dir = Path::new(&skill.path)
        .parent()
        .unwrap_or_else(|| Path::new(&skill.path));
    let skill_instruction_block = format_skill_instructions_block(
        skill.metadata.name.as_str(),
        skill.path.as_str(),
        &rendered_instructions,
    );

    // 构建用户消息（包含参数）
    let base_message = if let Some(ref args_str) = args {
        format!("执行技能 /{}: {}", skill_name, args_str)
    } else {
        format!("执行技能 /{}", skill_name)
    };

    let attachment_payload = attachments
        .as_deref()
        .map(build_attachment_payload)
        .unwrap_or_default();
    let has_attachments = attachments
        .as_ref()
        .map_or(false, |items| !items.is_empty());
    let user_message = merge_user_message(&base_message, &attachment_payload.text, has_attachments);

    // 根据 skill 的 context 设置决定是否包含屏幕记录
    let include_screen_context = skill.metadata.context.as_deref() == Some("screen");
    let screen_context = if include_screen_context {
        let query = parse_user_query(args.as_deref().unwrap_or_default());
        let search_result = storage.smart_search(&query).unwrap_or_default();
        let include_detail = config.storage.context_detail_hours != 0;
        let detail_cutoff = build_detail_cutoff(config);
        search_result.build_context(
            config.storage.max_context_chars,
            include_detail,
            detail_cutoff.as_deref(),
        )
    } else {
        String::new()
    };
    let context = build_context_with_global_prompts(config, screen_context);
    let available_skills: Vec<SkillMetadata> = Vec::new();
    let system_prompt = build_skill_execution_system_prompt(
        &context,
        skill_manager.get_skills_dir(),
        &skill_instruction_block,
    );
    let effective_allowed_tools = skill.metadata.allowed_tools.clone();

    if let Some(progress) = progress {
        progress.emit_step(
            "请求模型执行技能".to_string(),
            Some(format!("/{}", skill.metadata.name)),
        );
    }

    let model_history = compress_history_if_needed(
        history,
        &system_prompt,
        &user_message,
        &config.storage,
        progress,
    );

    if config.model.provider == "api" {
        let allowed_tools = &effective_allowed_tools;
        let history_candidates = build_overflow_recovery_histories(
            &model_history,
            &system_prompt,
            &user_message,
            &config.storage,
        );
        let total_candidates = history_candidates.len();
        let mut result: Option<ChatWithToolsResult> = None;
        let mut last_error: Option<String> = None;
        for (idx, candidate_history) in history_candidates.into_iter().enumerate() {
            let attempt = if attachment_payload.image_urls.is_empty()
                && attachment_payload.image_base64.is_empty()
            {
                let history_for_call = candidate_history.clone();
                if let Some(token) = cancel_token {
                    retry_with_cancel(token, progress, "model", || {
                        model_manager.chat_with_tools_with_system_prompt_filtered(
                            &config.model,
                            &system_prompt,
                            &user_message,
                            history_for_call.clone(),
                            &available_skills,
                            allowed_tools,
                        )
                    })
                    .await
                } else {
                    model_manager
                        .chat_with_tools_with_system_prompt_filtered(
                            &config.model,
                            &system_prompt,
                            &user_message,
                            history_for_call,
                            &available_skills,
                            allowed_tools,
                        )
                        .await
                }
            } else {
                let history_for_call = candidate_history.clone();
                if let Some(token) = cancel_token {
                    retry_with_cancel(token, progress, "model", || {
                        model_manager.chat_with_tools_with_system_prompt_with_images_filtered(
                            &config.model,
                            &system_prompt,
                            &user_message,
                            history_for_call.clone(),
                            &available_skills,
                            attachment_payload.image_urls.clone(),
                            attachment_payload.image_base64.clone(),
                            allowed_tools,
                        )
                    })
                    .await
                } else {
                    model_manager
                        .chat_with_tools_with_system_prompt_with_images_filtered(
                            &config.model,
                            &system_prompt,
                            &user_message,
                            history_for_call,
                            &available_skills,
                            attachment_payload.image_urls.clone(),
                            attachment_payload.image_base64.clone(),
                            allowed_tools,
                        )
                        .await
                }
            };

            match attempt {
                Ok(value) => {
                    result = Some(value);
                    break;
                }
                Err(err) => {
                    let can_retry = idx + 1 < total_candidates && is_context_overflow_error(&err);
                    if can_retry {
                        if let Some(progress) = progress {
                            progress.emit_info(
                                "Context overflow detected; retrying skill with tighter history"
                                    .to_string(),
                                Some(format!("attempt {}/{}", idx + 2, total_candidates)),
                            );
                        }
                        last_error = Some(err);
                        continue;
                    }
                    return Err(err);
                }
            }
        }
        let result = if let Some(value) = result {
            value
        } else {
            return Err(last_error.unwrap_or_else(|| "model request failed".to_string()));
        };

        return match Box::pin(run_tool_loop(
            storage,
            config,
            model_manager,
            skill_manager,
            &system_prompt,
            result,
            &available_skills,
            allowed_tools,
            Some(skill_dir),
            cancel_token,
            progress,
        ))
        .await
        {
            Ok(result) => {
                let mut tool_context = vec![ToolContextMessage {
                    role: "user".to_string(),
                    content: Some(skill_instruction_block.clone()),
                    tool_call_id: None,
                    tool_calls: None,
                }];
                tool_context.extend(result.tool_context);
                let chat_response = ChatResponse {
                    response: result.response,
                    tool_context,
                    active_skill: Some(skill_name.to_string()),
                };
                Ok(
                    serde_json::to_string(&chat_response)
                        .unwrap_or_else(|_| chat_response.response),
                )
            }
            Err(e) => Err(e),
        };
    }

    let response_text = if attachment_payload.image_urls.is_empty()
        && attachment_payload.image_base64.is_empty()
    {
        if let Some(token) = cancel_token {
            retry_with_cancel(token, progress, "model", || {
                model_manager.chat_with_system_prompt(
                    &config.model,
                    &system_prompt,
                    &user_message,
                    model_history.clone(),
                )
            })
            .await
        } else {
            model_manager
                .chat_with_system_prompt(
                    &config.model,
                    &system_prompt,
                    &user_message,
                    model_history,
                )
                .await
        }
    } else if let Some(token) = cancel_token {
        retry_with_cancel(token, progress, "model", || {
            model_manager.chat_with_system_prompt_with_images(
                &config.model,
                &system_prompt,
                &user_message,
                model_history.clone(),
                attachment_payload.image_urls.clone(),
                attachment_payload.image_base64.clone(),
            )
        })
        .await
    } else {
        model_manager
            .chat_with_system_prompt_with_images(
                &config.model,
                &system_prompt,
                &user_message,
                model_history,
                attachment_payload.image_urls,
                attachment_payload.image_base64,
            )
            .await
    }?;

    let chat_response = ChatResponse {
        response: response_text,
        tool_context: vec![ToolContextMessage {
            role: "user".to_string(),
            content: Some(skill_instruction_block),
            tool_call_id: None,
            tool_calls: None,
        }],
        active_skill: Some(skill_name.to_string()),
    };
    Ok(serde_json::to_string(&chat_response).unwrap_or_else(|_| chat_response.response))
}

/// 解析用户问题，提取时间范围和关键词
fn parse_user_query(message: &str) -> SearchQuery {
    let msg_lower = message.to_lowercase();

    // 提取时间范围
    let time_range = if msg_lower.contains("刚才") || msg_lower.contains("刚刚") {
        TimeRange::Recent(5) // 最近5分钟
    } else if msg_lower.contains("最近") && msg_lower.contains("分钟") {
        // 尝试提取分钟数
        let minutes = extract_number(&msg_lower).unwrap_or(10);
        TimeRange::Recent(minutes)
    } else if msg_lower.contains("今天") || msg_lower.contains("上午") || msg_lower.contains("下午")
    {
        TimeRange::Today
    } else if msg_lower.contains("昨天") {
        TimeRange::Days(2)
    } else if msg_lower.contains("这周") || msg_lower.contains("本周") {
        TimeRange::Days(7)
    } else {
        // 默认：最近10分钟 + 今天的聚合
        TimeRange::Recent(10)
    };

    // 提取关键词
    let keywords = extract_keywords(message);
    let include_detail = wants_detail(message) || matches!(time_range, TimeRange::Recent(_));

    SearchQuery {
        time_range,
        keywords,
        include_detail,
    }
}

fn extract_number(text: &str) -> Option<u32> {
    // 中文数字映射
    let cn_nums = [
        ("一", 1),
        ("二", 2),
        ("三", 3),
        ("四", 4),
        ("五", 5),
        ("六", 6),
        ("七", 7),
        ("八", 8),
        ("九", 9),
        ("十", 10),
        ("十五", 15),
        ("二十", 20),
        ("三十", 30),
    ];

    for (cn, num) in cn_nums {
        if text.contains(cn) {
            return Some(num);
        }
    }

    // 阿拉伯数字
    let re = regex::Regex::new(r"\d+").ok()?;
    re.find(text).and_then(|m| m.as_str().parse().ok())
}

fn extract_keywords(message: &str) -> Vec<String> {
    let mut keywords = Vec::new();

    // 提取引号中的内容
    let quote_chars = ['"', '“', '”', '「', '」', '\''];
    for quote in quote_chars {
        if let Some(start) = message.find(quote) {
            let rest = &message[start + quote.len_utf8()..];
            if let Some(end) = rest.find(|c| quote_chars.contains(&c)) {
                let candidate = rest[..end].trim();
                if !candidate.is_empty() {
                    keywords.push(candidate.to_string());
                }
            }
        }
    }

    // 提取技术关键词
    let tech_keywords = [
        "error", "错误", "报错", "bug", "异常", "代码", "文件", "函数", "编辑", "修改", ".rs",
        ".ts", ".js", ".py", ".vue", ".tsx", "Chrome", "VS Code", "Terminal",
    ];

    for kw in tech_keywords {
        if message.to_lowercase().contains(&kw.to_lowercase()) {
            keywords.push(kw.to_string());
        }
    }

    keywords
}

fn wants_detail(message: &str) -> bool {
    let msg = message.to_lowercase();
    let triggers = [
        "详细",
        "细节",
        "具体",
        "截图",
        "画面",
        "界面",
        "内容",
        "显示",
        "文本",
        "按钮",
        "输入",
        "输出",
        "哪一页",
        "哪个页面",
        "哪一个文件",
        "哪行",
        "哪一行",
        "日志",
        "报错内容",
        "报错",
        "错误",
        "失败",
        "异常",
        "无法",
        "连不上",
        "连接不上",
        "原因",
        "为什么",
        "提示",
        "配置",
        "detail",
        "details",
        "screenshot",
        "screen",
        "page",
        "error log",
    ];

    triggers.iter().any(|kw| msg.contains(kw))
}

fn should_use_screen_context(mode: &str, message: &str) -> bool {
    match mode {
        "always" => true,
        "off" => false,
        _ => wants_screen_context_auto(message),
    }
}

fn wants_screen_context_auto(message: &str) -> bool {
    if wants_detail(message) {
        return true;
    }

    let msg = message.to_lowercase();
    let context_triggers = [
        "??", "??", "??", "??", "??", "??", "??", "??", "??", "??", "??", "??", "??", "??", "??",
        "??", "??", "??", "??", "??", "??", "alert", "log", "error", "crash", "issue", "history",
        "record", "records", "activity",
    ];
    if context_triggers.iter().any(|kw| msg.contains(kw)) {
        return true;
    }

    let time_triggers = [
        "??",
        "??",
        "??",
        "??",
        "??",
        "??",
        "??",
        "??",
        "??",
        "??",
        "??",
        "??",
        "recent",
        "earlier",
        "before",
        "today",
        "yesterday",
        "this week",
        "last week",
    ];
    let action_triggers = [
        "??", "??", "??", "??", "??", "??", "??", "??", "??", "??", "??", "??", "??", "open",
        "click", "type", "input", "edit", "save",
    ];
    time_triggers.iter().any(|kw| msg.contains(kw))
        && action_triggers.iter().any(|kw| msg.contains(kw))
}

fn build_detail_cutoff(config: &Config) -> Option<String> {
    let hours = config.storage.context_detail_hours;
    if hours == 0 {
        return None;
    }
    let cutoff = Local::now() - Duration::hours(hours as i64);
    Some(cutoff.format("%Y-%m-%dT%H:%M:%S").to_string())
}

/// 构建包含全局提示词的上下文
fn build_context_with_global_prompts(config: &Config, context: String) -> String {
    let global_section = build_global_prompts_section(config);
    if global_section.is_empty() {
        context
    } else {
        format!("{}{}", global_section, context)
    }
}

/// 构建全局提示词部分
fn build_global_prompts_section(config: &Config) -> String {
    let enabled_prompts: Vec<&str> = config
        .global_prompt
        .items
        .iter()
        .filter(|item| item.enabled && !item.content.trim().is_empty())
        .map(|item| item.content.as_str())
        .collect();

    if enabled_prompts.is_empty() {
        String::new()
    } else {
        format!("## 用户预设信息\n{}\n\n", enabled_prompts.join("\n\n"))
    }
}

fn merge_recent_records(
    records: Vec<SummaryRecord>,
    fallback: Vec<SummaryRecord>,
    limit: usize,
) -> Vec<SummaryRecord> {
    if limit == 0 {
        return records;
    }

    let mut seen = HashSet::new();
    let mut merged = Vec::new();

    for record in records.into_iter().chain(fallback.into_iter()) {
        let key = format!("{}|{}|{}", record.timestamp, record.app, record.summary);
        if seen.insert(key) {
            merged.push(record);
        }
    }

    merged.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    if merged.len() > limit {
        let start = merged.len() - limit;
        merged = merged.split_off(start);
    }

    merged
}

#[tauri::command]
pub async fn get_summaries(date: String) -> Result<Vec<SummaryRecord>, String> {
    let storage = StorageManager::new();
    storage.get_summaries(&date).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_summaries(date: String) -> Result<usize, String> {
    let storage = StorageManager::new();
    storage
        .delete_summaries_for_date(&date)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_all_summaries() -> Result<usize, String> {
    let storage = StorageManager::new();
    storage.delete_all_summaries().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_screenshots_dir(app_handle: AppHandle) -> Result<(), String> {
    let storage = StorageManager::new();
    let dir = storage.screenshots_dir()?;
    let dir_str = dir.to_string_lossy().to_string();
    app_handle
        .shell()
        .open(dir_str, None)
        .map_err(|e| e.to_string())
}

/// 读取图片文件并返回 base64 编码
/// file_type: "attachment" | "screenshot"
#[tauri::command]
pub async fn read_image_base64(
    file_path: String,
    file_type: Option<String>,
) -> Result<String, String> {
    let storage = StorageManager::new();
    let data_dir = storage.get_data_dir().to_path_buf();

    // 根据类型确定基础目录
    let base_dir = match file_type.as_deref() {
        Some("screenshot") => data_dir.join("screenshots"),
        Some("attachment") => data_dir.join("attachments"),
        _ => data_dir.clone(),
    };

    // 构建完整路径
    let full_path = if Path::new(&file_path).is_absolute() {
        PathBuf::from(&file_path)
    } else {
        base_dir.join(&file_path)
    };

    // 安全检查：确保路径在数据目录内
    let canonical = full_path
        .canonicalize()
        .map_err(|e| format!("文件不存在: {}", e))?;
    let data_canonical = data_dir
        .canonicalize()
        .map_err(|e| format!("数据目录错误: {}", e))?;

    if !canonical.starts_with(&data_canonical) {
        return Err("不允许访问数据目录外的文件".to_string());
    }

    // 检查文件大小
    let metadata = fs::metadata(&canonical).map_err(|e| format!("读取文件信息失败: {}", e))?;
    if metadata.len() > MAX_ATTACHMENT_BYTES {
        return Err("文件过大".to_string());
    }

    // 读取文件并编码
    let bytes = fs::read(&canonical).map_err(|e| format!("读取文件失败: {}", e))?;

    // 根据扩展名确定 MIME 类型
    let ext = canonical
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "application/octet-stream",
    };

    let base64_str = BASE64.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, base64_str))
}

#[tauri::command]
pub async fn open_release_page(app_handle: AppHandle) -> Result<(), String> {
    app_handle
        .shell()
        .open(RELEASE_PAGE_URL.to_string(), None)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_external_url(app_handle: AppHandle, url: String) -> Result<(), String> {
    app_handle
        .shell()
        .open(url, None)
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct BashRuntimeEnsureResult {
    pub available: bool,
    pub attempted_install: bool,
    pub installed_now: bool,
    pub bash_path: Option<String>,
    pub message: String,
}

#[tauri::command]
pub async fn ensure_bash_runtime(
    auto_install: Option<bool>,
) -> Result<BashRuntimeEnsureResult, String> {
    #[cfg(not(target_os = "windows"))]
    {
        return Ok(BashRuntimeEnsureResult {
            available: true,
            attempted_install: false,
            installed_now: false,
            bash_path: Some("sh".to_string()),
            message: "Non-Windows platform uses default shell.".to_string(),
        });
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(path) = find_windows_bash_path() {
            return Ok(BashRuntimeEnsureResult {
                available: true,
                attempted_install: false,
                installed_now: false,
                bash_path: Some(path.to_string_lossy().to_string()),
                message: "Bash runtime detected.".to_string(),
            });
        }

        let should_install = auto_install.unwrap_or(true);
        if !should_install {
            return Ok(BashRuntimeEnsureResult {
                available: false,
                attempted_install: false,
                installed_now: false,
                bash_path: None,
                message: "Bash runtime not found.".to_string(),
            });
        }

        let mut logs = Vec::new();
        for args in [
            vec![
                "install",
                "--id",
                "Git.Git",
                "-e",
                "--silent",
                "--accept-package-agreements",
                "--accept-source-agreements",
                "--scope",
                "user",
            ],
            vec![
                "install",
                "--id",
                "Git.Git",
                "-e",
                "--silent",
                "--accept-package-agreements",
                "--accept-source-agreements",
            ],
        ] {
            let mut cmd = TokioCommand::new("winget");
            cmd.args(&args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let run = timeout(TokioDuration::from_secs(15 * 60), cmd.output()).await;
            match run {
                Ok(Ok(output)) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    logs.push(format!(
                        "winget {} -> exit {}\nstdout: {}\nstderr: {}",
                        args.join(" "),
                        output.status.code().unwrap_or(-1),
                        if stdout.is_empty() {
                            "(empty)"
                        } else {
                            &stdout
                        },
                        if stderr.is_empty() {
                            "(empty)"
                        } else {
                            &stderr
                        }
                    ));

                    if output.status.success() {
                        break;
                    }
                }
                Ok(Err(e)) => {
                    logs.push(format!("winget {} -> exec error: {}", args.join(" "), e));
                }
                Err(_) => {
                    logs.push(format!("winget {} -> timeout", args.join(" ")));
                }
            }
        }

        if let Some(path) = refresh_windows_bash_path_cache() {
            return Ok(BashRuntimeEnsureResult {
                available: true,
                attempted_install: true,
                installed_now: true,
                bash_path: Some(path.to_string_lossy().to_string()),
                message: "Bash runtime installed successfully.".to_string(),
            });
        }

        Ok(BashRuntimeEnsureResult {
            available: false,
            attempted_install: true,
            installed_now: false,
            bash_path: None,
            message: format!(
                "Silent install failed. Manual install may be required. {}",
                logs.join("\n---\n")
            ),
        })
    }
}

#[derive(serde::Serialize)]
pub struct AlertRecord {
    pub timestamp: String,
    pub issue_type: String,
    pub message: String,
    pub suggestion: String,
    pub confidence: f32,
    // 意图识别相关字段
    pub intent: String,
    pub scene: String,
    pub help_type: String,
    pub urgency: String,
    pub related_skill: String,
}

#[tauri::command]
pub async fn get_recent_alerts(since: Option<String>) -> Result<Vec<AlertRecord>, String> {
    let storage = StorageManager::new();
    let config = storage.load_config().map_err(|e| e.to_string())?;
    let threshold = config.capture.alert_confidence_threshold.clamp(0.0, 1.0);
    let cooldown = config.capture.alert_cooldown_seconds as i64;
    let days = config.storage.retention_days.max(1);

    let since_dt = since
        .as_deref()
        .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok())
        .and_then(|dt| Local.from_local_datetime(&dt).single());

    let mut records = Vec::new();
    for i in 0..days {
        let date = (Local::now() - Duration::days(i as i64))
            .format("%Y-%m-%d")
            .to_string();
        if let Ok(mut daily) = storage.get_summaries(&date) {
            records.append(&mut daily);
        }
    }

    records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    let mut last_seen: std::collections::HashMap<String, chrono::DateTime<Local>> =
        std::collections::HashMap::new();
    let mut alerts = Vec::new();

    for record in records {
        if !record.has_issue || record.confidence < threshold {
            continue;
        }
        let dt = match NaiveDateTime::parse_from_str(&record.timestamp, "%Y-%m-%dT%H:%M:%S")
            .ok()
            .and_then(|v| Local.from_local_datetime(&v).single())
        {
            Some(value) => value,
            None => continue,
        };
        if let Some(since_dt) = since_dt {
            if dt <= since_dt {
                continue;
            }
        }

        let message = if record.issue_summary.is_empty() {
            record.summary.clone()
        } else {
            record.issue_summary.clone()
        };
        let key = format!("{}:{}", record.issue_type, message);
        if let Some(prev) = last_seen.get(&key) {
            if dt.signed_duration_since(*prev).num_seconds() < cooldown {
                continue;
            }
        }
        last_seen.insert(key, dt);

        alerts.push(AlertRecord {
            timestamp: record.timestamp,
            issue_type: if record.issue_type.is_empty() {
                "unknown".to_string()
            } else {
                record.issue_type
            },
            message,
            suggestion: record.suggestion,
            confidence: record.confidence,
            intent: record.intent,
            scene: record.scene,
            help_type: if record.has_issue {
                "error".to_string()
            } else {
                "info".to_string()
            },
            urgency: record.urgency,
            related_skill: record.related_skill,
        });
    }

    Ok(alerts)
}

// ==================== Skills 相关命令 ====================

/// 列出所有可用的 skills
#[tauri::command]
pub async fn list_skills(state: State<'_, AppState>) -> Result<Vec<SkillMetadata>, String> {
    let skill_manager = SkillManager::new();
    Ok(get_available_skills_cached(&state, &skill_manager).await)
}

/// 获取完整的 skill 信息
#[tauri::command]
pub async fn get_skill(name: String) -> Result<Skill, String> {
    let skill_manager = SkillManager::new();
    skill_manager.load_skill(&name)
}

/// 调用 skill
#[tauri::command]
pub async fn invoke_skill(
    name: String,
    args: Option<String>,
    history: Option<Vec<ChatHistoryMessage>>,
    attachments: Option<Vec<AttachmentInput>>,
    request_id: Option<String>,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let storage = StorageManager::new();
    let config = storage.load_config().map_err(|e| e.to_string())?;
    let model_manager = ModelManager::new();
    let skill_manager = SkillManager::new();
    let request_id =
        request_id.unwrap_or_else(|| format!("req-{}", Local::now().timestamp_millis()));
    let cancel_token = register_cancel_token(&state, &request_id).await;
    let progress = ProgressEmitter::new(
        &app_handle,
        config.ui.show_progress,
        Some(request_id.clone()),
    );
    if let Some(ref progress) = progress {
        progress.emit_start(&format!("开始执行技能 /{}", name));
        progress.emit_info("Prepare to run skill".to_string(), None);
        progress.emit_step("调用技能".to_string(), Some(format!("/{}", name)));
    }
    let result = execute_skill_internal(
        &storage,
        &config,
        &model_manager,
        &skill_manager,
        &name,
        args,
        history,
        attachments,
        Some(&cancel_token),
        progress.as_ref(),
    )
    .await;
    if let Some(ref progress) = progress {
        if result.is_ok() {
            progress.emit_done("处理完成");
        } else {
            progress.emit_error("处理失败");
        }
    }
    clear_cancel_token(&state, &request_id).await;
    result
}

/// 创建新的 skill
#[tauri::command]
pub async fn create_skill(
    name: String,
    description: String,
    instructions: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let skill_manager = SkillManager::new();
    skill_manager.create_skill(&name, &description, &instructions)?;
    state.bump_skills_version();
    Ok(())
}

/// 删除 skill
#[tauri::command]
pub async fn delete_skill(name: String, state: State<'_, AppState>) -> Result<(), String> {
    let skill_manager = SkillManager::new();
    skill_manager.delete_skill(&name)?;
    state.bump_skills_version();
    Ok(())
}

/// 获取 skills 目录路径
#[tauri::command]
pub async fn get_skills_dir() -> Result<String, String> {
    let skill_manager = SkillManager::new();
    Ok(skill_manager.get_skills_dir().to_string_lossy().to_string())
}

/// 打开 skills 目录
#[tauri::command]
pub async fn open_skills_dir(app_handle: AppHandle) -> Result<(), String> {
    let skill_manager = SkillManager::new();
    let dir = skill_manager.get_skills_dir();

    // 确保目录存在
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建 skills 目录失败: {}", e))?;
    }

    let dir_str = dir.to_string_lossy().to_string();
    app_handle
        .shell()
        .open(dir_str, None)
        .map_err(|e| e.to_string())
}

// ==================== 通知窗口相关命令 ====================

/// 显示通知窗口
#[tauri::command]
pub async fn show_notification(
    app_handle: AppHandle,
    intent: String,
    scene: String,
    help_type: String,
    summary: String,
    suggestion: String,
    urgency: String,
) -> Result<(), String> {
    use tauri::{PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

    // 检查是否已存在通知窗口
    if let Some(window) = app_handle.get_webview_window("notification") {
        // 窗口已存在，发送更新事件
        let _ = window.emit(
            "notification-update",
            serde_json::json!({
                "intent": intent,
                "scene": scene,
                "help_type": help_type,
                "summary": summary,
                "suggestion": suggestion,
                "urgency": urgency,
            }),
        );
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }

    // 获取主显示器信息以定位窗口到右下角
    let window_width = 380u32;
    let window_height = 140u32;
    let margin = 20i32;

    // 创建新的通知窗口
    let notification_url = format!(
        "/notification?intent={}&scene={}&help_type={}&summary={}&suggestion={}&urgency={}",
        urlencoding::encode(&intent),
        urlencoding::encode(&scene),
        urlencoding::encode(&help_type),
        urlencoding::encode(&summary),
        urlencoding::encode(&suggestion),
        urlencoding::encode(&urgency),
    );

    let window = WebviewWindowBuilder::new(
        &app_handle,
        "notification",
        WebviewUrl::App(notification_url.into()),
    )
    .title("OpenCowork 提醒")
    .inner_size(window_width as f64, window_height as f64)
    .resizable(false)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .build()
    .map_err(|e| format!("创建通知窗口失败: {}", e))?;

    // 尝试将窗口定位到右下角
    if let Some(monitor) = window.current_monitor().ok().flatten() {
        let monitor_size = monitor.size();
        let monitor_position = monitor.position();
        let x = monitor_position.x + monitor_size.width as i32 - window_width as i32 - margin;
        let y =
            monitor_position.y + monitor_size.height as i32 - window_height as i32 - margin - 40; // 40 for taskbar
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }

    Ok(())
}

/// 关闭通知窗口
#[tauri::command]
pub async fn close_notification(app_handle: AppHandle) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("notification") {
        window
            .close()
            .map_err(|e| format!("关闭通知窗口失败: {}", e))?;
    }
    Ok(())
}

/// 聚焦主窗口
#[tauri::command]
pub async fn focus_main_window(app_handle: AppHandle) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        window
            .set_focus()
            .map_err(|e| format!("聚焦主窗口失败: {}", e))?;
    }
    Ok(())
}

const MAX_ATTACHMENT_BYTES: u64 = 5 * 1024 * 1024;
const MAX_ATTACHMENT_TEXT_CHARS: usize = 8000;
const MAX_ATTACHMENT_IMAGES: usize = 4;

fn merge_user_message(message: &str, attachment_text: &str, has_attachments: bool) -> String {
    let mut merged = message.trim().to_string();
    if merged.is_empty() && has_attachments {
        merged = "用户发送了附件，请阅读附件内容并回答。".to_string();
    }
    if !attachment_text.trim().is_empty() {
        if !merged.is_empty() {
            merged.push_str("\n\n");
        }
        merged.push_str(attachment_text.trim());
    }
    merged
}

fn build_attachment_payload(attachments: &[AttachmentInput]) -> AttachmentPayload {
    if attachments.is_empty() {
        return AttachmentPayload::default();
    }

    let mut image_urls = Vec::new();
    let mut image_base64 = Vec::new();
    let mut image_names = Vec::new();
    let mut doc_sections = Vec::new();
    let mut notes = Vec::new();

    for attachment in attachments {
        if attachment.path.trim().is_empty() {
            continue;
        }

        let name = attachment_name(&attachment.path, &attachment.name);
        let ext = attachment_extension(&attachment.path);

        if let Ok(meta) = fs::metadata(&attachment.path) {
            if meta.len() > MAX_ATTACHMENT_BYTES {
                notes.push(format!("- {} (文件过大，已跳过内容)", name));
                continue;
            }
        }

        let is_image = matches!(attachment.kind.as_deref(), Some("image")) || is_image_ext(&ext);
        let is_text_doc = is_text_doc_ext(&ext);
        let is_office_doc = is_office_doc_ext(&ext);

        if is_image {
            if image_urls.len() >= MAX_ATTACHMENT_IMAGES {
                notes.push(format!("- {} (图片数量超过限制)", name));
                continue;
            }
            match fs::read(&attachment.path) {
                Ok(bytes) => {
                    let encoded = BASE64.encode(bytes);
                    let mime = image_mime(&ext);
                    image_urls.push(format!("data:{};base64,{}", mime, encoded));
                    image_base64.push(encoded);
                    image_names.push(name);
                }
                Err(err) => {
                    notes.push(format!("- {} (读取失败: {})", name, err));
                }
            }
            continue;
        }

        if is_text_doc {
            match fs::read(&attachment.path) {
                Ok(bytes) => {
                    let mut content = String::from_utf8_lossy(&bytes).to_string();
                    if content.len() > MAX_ATTACHMENT_TEXT_CHARS {
                        content.truncate(MAX_ATTACHMENT_TEXT_CHARS);
                        content.push_str("\n...(已截断)");
                    }
                    let trimmed = content.trim();
                    if trimmed.is_empty() {
                        notes.push(format!("- {} (文件内容为空)", name));
                    } else {
                        doc_sections.push(format!("### {}\n{}", name, trimmed));
                    }
                }
                Err(err) => {
                    notes.push(format!("- {} (读取失败: {})", name, err));
                }
            }
            continue;
        }

        if is_office_doc {
            let parsed = match ext.as_str() {
                "docx" => extract_docx_text(&attachment.path, MAX_ATTACHMENT_TEXT_CHARS),
                "xlsx" => extract_xlsx_text(&attachment.path, MAX_ATTACHMENT_TEXT_CHARS),
                _ => Err(format!("不支持的 Office 格式: {}", ext)),
            };
            match parsed {
                Ok(mut content) => {
                    if content.len() > MAX_ATTACHMENT_TEXT_CHARS {
                        content.truncate(MAX_ATTACHMENT_TEXT_CHARS);
                        content.push_str("\n...(已截断)");
                    }
                    let trimmed = content.trim();
                    if trimmed.is_empty() {
                        notes.push(format!("- {} (文件内容为空)", name));
                    } else {
                        doc_sections.push(format!("### {}\n{}", name, trimmed));
                    }
                }
                Err(err) => {
                    notes.push(format!("- {} (解析失败: {})", name, err));
                }
            }
            continue;
        }

        notes.push(format!("- {} (二进制附件，未解析内容)", name));
    }

    let mut text = String::new();
    if !image_names.is_empty() {
        text.push_str("图片附件:\n");
        for name in &image_names {
            text.push_str(&format!("- {}\n", name));
        }
    }

    if !doc_sections.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str("文档内容:\n");
        text.push_str(&doc_sections.join("\n\n"));
    }

    if !notes.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str("附件备注:\n");
        for note in notes {
            text.push_str(&format!("{}\n", note));
        }
    }

    let text = if text.trim().is_empty() {
        String::new()
    } else {
        format!("## 附件\n{}", text.trim())
    };

    AttachmentPayload {
        text,
        image_urls,
        image_base64,
    }
}

fn sanitize_attachment_name(name: Option<&str>) -> String {
    let Some(name) = name else {
        return String::new();
    };
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

fn attachment_name(path: &str, name: &str) -> String {
    let trimmed = name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

fn attachment_extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase()
}

fn is_image_ext(ext: &str) -> bool {
    matches!(ext, "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp")
}

fn is_text_doc_ext(ext: &str) -> bool {
    matches!(ext, "txt" | "md" | "json" | "csv" | "log" | "yaml" | "yml")
}

fn is_office_doc_ext(ext: &str) -> bool {
    matches!(ext, "docx" | "xlsx")
}

fn extract_docx_text(path: &str, max_chars: usize) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|e| format!("读取失败: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("打开压缩失败: {}", e))?;
    let doc_file = archive
        .by_name("word/document.xml")
        .map_err(|_| "未找到 word/document.xml".to_string())?;

    let mut reader = Reader::from_reader(BufReader::new(doc_file));
    reader.trim_text(true);

    let mut buf = Vec::new();
    let mut text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if e.name().as_ref() == b"w:p" {
                    if !text.ends_with('\n') && !text.is_empty() {
                        text.push('\n');
                    }
                } else if e.name().as_ref() == b"w:tab" {
                    text.push('\t');
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"w:p" {
                    text.push('\n');
                }
            }
            Ok(Event::Text(e)) => {
                let content = e
                    .unescape()
                    .map_err(|err| format!("解析 Word 失败: {}", err))?;
                if !content.is_empty() {
                    text.push_str(&content);
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(format!("解析 Word 失败: {}", err)),
            _ => {}
        }
        if text.len() >= max_chars {
            break;
        }
        buf.clear();
    }

    Ok(text)
}

fn extract_xlsx_text(path: &str, max_chars: usize) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|e| format!("读取失败: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("打开压缩失败: {}", e))?;
    let shared_strings = read_shared_strings(&mut archive).unwrap_or_default();

    let mut sheet_names = Vec::new();
    for i in 0..archive.len() {
        let name = {
            let file = archive
                .by_index(i)
                .map_err(|e| format!("读取工作表失败: {}", e))?;
            file.name().to_string()
        };
        if name.starts_with("xl/worksheets/") && name.ends_with(".xml") && !name.contains("_rels/")
        {
            sheet_names.push(name);
        }
    }
    sheet_names.sort();

    let mut output = String::new();
    for sheet_name in sheet_names {
        if output.len() >= max_chars {
            break;
        }

        let display = sheet_name.rsplit('/').next().unwrap_or(sheet_name.as_str());
        output.push_str(&format!("Sheet: {}\n", display));

        let sheet_file = archive
            .by_name(&sheet_name)
            .map_err(|e| format!("读取工作表失败: {}", e))?;
        let mut reader = Reader::from_reader(BufReader::new(sheet_file));
        reader.trim_text(true);

        let mut buf = Vec::new();
        let mut cell_ref = String::new();
        let mut cell_type: Option<String> = None;
        let mut value = String::new();
        let mut in_value = false;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => match e.name().as_ref() {
                    b"c" => {
                        cell_ref.clear();
                        cell_type = None;
                        value.clear();
                        for attr in e.attributes().with_checks(false) {
                            let attr =
                                attr.map_err(|err| format!("解析 Excel 属性失败: {}", err))?;
                            let key = attr.key.as_ref();
                            if key == b"r" {
                                cell_ref = attr
                                    .unescape_value()
                                    .map_err(|err| format!("解析 Excel 单元格失败: {}", err))?
                                    .to_string();
                            } else if key == b"t" {
                                cell_type = Some(
                                    attr.unescape_value()
                                        .map_err(|err| format!("解析 Excel 单元格失败: {}", err))?
                                        .to_string(),
                                );
                            }
                        }
                    }
                    b"v" | b"t" => {
                        in_value = true;
                        value.clear();
                    }
                    _ => {}
                },
                Ok(Event::End(e)) => match e.name().as_ref() {
                    b"v" | b"t" => {
                        in_value = false;
                    }
                    b"c" => {
                        if !value.trim().is_empty() {
                            let resolved = resolve_xlsx_cell_value(
                                value.trim(),
                                cell_type.as_deref(),
                                &shared_strings,
                            );
                            if !resolved.trim().is_empty() {
                                if cell_ref.is_empty() {
                                    output.push_str(&resolved);
                                } else {
                                    output.push_str(&format!("{}: {}", cell_ref, resolved));
                                }
                                output.push('\n');
                            }
                        }
                        cell_ref.clear();
                        cell_type = None;
                        value.clear();
                    }
                    _ => {}
                },
                Ok(Event::Text(e)) => {
                    if in_value {
                        let content = e
                            .unescape()
                            .map_err(|err| format!("解析 Excel 失败: {}", err))?;
                        if !content.is_empty() {
                            value.push_str(&content);
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(err) => return Err(format!("解析 Excel 失败: {}", err)),
                _ => {}
            }

            if output.len() >= max_chars {
                break;
            }
            buf.clear();
        }

        output.push('\n');
    }

    Ok(output)
}

fn read_shared_strings(archive: &mut ZipArchive<fs::File>) -> Result<Vec<String>, String> {
    let file = match archive.by_name("xl/sharedStrings.xml") {
        Ok(file) => file,
        Err(_) => return Ok(Vec::new()),
    };

    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.trim_text(true);
    let mut buf = Vec::new();
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut in_si = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if e.name().as_ref() == b"si" {
                    in_si = true;
                    current.clear();
                }
            }
            Ok(Event::End(e)) => {
                if e.name().as_ref() == b"si" {
                    strings.push(current.clone());
                    current.clear();
                    in_si = false;
                }
            }
            Ok(Event::Text(e)) => {
                if in_si {
                    let content = e
                        .unescape()
                        .map_err(|err| format!("解析 Excel 失败: {}", err))?;
                    current.push_str(&content);
                }
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(format!("解析 Excel 失败: {}", err)),
            _ => {}
        }
        buf.clear();
    }

    Ok(strings)
}

fn resolve_xlsx_cell_value(
    value: &str,
    cell_type: Option<&str>,
    shared_strings: &[String],
) -> String {
    match cell_type {
        Some("s") => value
            .parse::<usize>()
            .ok()
            .and_then(|idx| shared_strings.get(idx).cloned())
            .unwrap_or_else(|| value.to_string()),
        _ => value.to_string(),
    }
}

fn image_mime(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/png",
    }
}

fn normalize_tool_mode(mode: &str) -> String {
    match mode.trim().to_lowercase().as_str() {
        "whitelist" => "whitelist".to_string(),
        "allow_all" => "allow_all".to_string(),
        "unset" => "unset".to_string(),
        _ => "unset".to_string(),
    }
}

fn build_tool_access(
    config: &Config,
    storage: &StorageManager,
    preferred_base_dir: Option<&Path>,
) -> ToolAccess {
    let mode = normalize_tool_mode(&config.tools.mode);
    let data_dir = storage.get_data_dir().to_path_buf();
    let mut allowed_dirs = Vec::new();

    for dir in &config.tools.allowed_dirs {
        let trimmed = dir.trim();
        if trimmed.is_empty() {
            continue;
        }
        let raw = PathBuf::from(trimmed);
        let resolved = if raw.is_absolute() {
            raw
        } else {
            data_dir.join(raw)
        };
        allowed_dirs.push(normalize_path(&resolved));
    }

    if allowed_dirs.is_empty() {
        allowed_dirs.push(normalize_path(&data_dir));
    }

    let default_base_dir = allowed_dirs
        .get(0)
        .cloned()
        .unwrap_or_else(|| normalize_path(&data_dir));
    let base_dir = if let Some(dir) = preferred_base_dir {
        let preferred = normalize_path(dir);
        if mode == "allow_all"
            || allowed_dirs
                .iter()
                .any(|allowed| preferred.starts_with(allowed))
        {
            preferred
        } else {
            default_base_dir
        }
    } else {
        default_base_dir
    };

    ToolAccess {
        mode,
        allowed_commands: config.tools.allowed_commands.clone(),
        allowed_dirs,
        tasks_dir: base_dir.join(".task_outputs"),
        base_dir,
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => result.push(comp.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(os) => result.push(os),
        }
    }
    result
}

fn resolve_path(access: &ToolAccess, path: &str) -> PathBuf {
    let raw = PathBuf::from(path.trim());
    let resolved = if raw.is_absolute() {
        raw
    } else {
        access.base_dir.join(raw)
    };
    normalize_path(&resolved)
}

fn path_is_allowed(access: &ToolAccess, path: &Path) -> bool {
    if access.mode == "allow_all" {
        return true;
    }
    let normalized = normalize_path(path);
    access
        .allowed_dirs
        .iter()
        .any(|dir| normalized.starts_with(dir))
}

fn ensure_path_allowed(access: &ToolAccess, path: &str) -> Result<PathBuf, String> {
    let resolved = resolve_path(access, path);
    if access.mode == "whitelist" && !path_is_allowed(access, &resolved) {
        return Err(format!("路径不在允许范围内: {}", resolved.display()));
    }
    Ok(resolved)
}

fn tool_allowed_in_skill(tool_name: &str, allowed_tools: &Option<Vec<String>>) -> bool {
    let Some(list) = allowed_tools else {
        return true;
    };
    if list.is_empty() {
        return false;
    }

    let target = normalize_tool_name(tool_name);
    for item in list {
        let trimmed = item.trim();
        if trimmed == "*" {
            return true;
        }
        let name = trimmed.split('(').next().unwrap_or(trimmed).trim();
        if normalize_tool_name(name) == target {
            return true;
        }
    }
    false
}

fn parse_optional_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|v| v.as_str())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn parse_string_list(value: Option<&serde_json::Value>) -> Option<Vec<String>> {
    let value = value?;
    let mut items = Vec::new();
    match value {
        serde_json::Value::Array(values) => {
            for entry in values {
                if let Some(s) = entry.as_str() {
                    let s = s.trim();
                    if !s.is_empty() {
                        items.push(s.to_string());
                    }
                }
            }
        }
        serde_json::Value::String(text) => {
            for part in text.split(|c: char| c == ',' || c == '\n' || c == '\t') {
                for token in part.split_whitespace() {
                    let token = token.trim();
                    if !token.is_empty() {
                        items.push(token.to_string());
                    }
                }
            }
        }
        _ => return None,
    }

    if items.is_empty() {
        None
    } else {
        Some(items)
    }
}

fn parse_metadata_map(value: Option<&serde_json::Value>) -> Option<HashMap<String, String>> {
    let value = value?;
    let obj = value.as_object()?;
    let mut map = HashMap::new();
    for (key, val) in obj {
        if let Some(s) = val.as_str() {
            let s = s.trim();
            if !s.is_empty() {
                map.insert(key.clone(), s.to_string());
            }
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

fn normalize_tool_name(name: &str) -> String {
    match name.trim().to_lowercase().as_str() {
        "update" => "edit".to_string(),
        "run_command" => "bash".to_string(),
        other => other.to_string(),
    }
}

fn is_model_invocable_skill(skill: &SkillMetadata) -> bool {
    skill.user_invocable.unwrap_or(true) && !skill.disable_model_invocation.unwrap_or(false)
}

fn tokenize_skill_args(args: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut chars = args.chars().peekable();

    while let Some(ch) = chars.next() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else if ch == '\\' {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    } else {
                        current.push(ch);
                    }
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '"' | '\'' => {
                    quote = Some(ch);
                }
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    } else {
                        current.push(ch);
                    }
                }
                c if c.is_whitespace() => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                }
                _ => current.push(ch),
            },
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn inject_skill_arguments(instructions: &str, args: Option<&str>) -> String {
    let raw = args.unwrap_or("").trim();
    let tokens = if raw.is_empty() {
        Vec::new()
    } else {
        tokenize_skill_args(raw)
    };

    let mut rendered = instructions.replace("$ARGUMENTS", raw);
    let args_indexed = Regex::new(r"\$ARGUMENTS\[(\d+)\]").unwrap();
    rendered = args_indexed
        .replace_all(&rendered, |caps: &regex::Captures<'_>| {
            let idx = caps
                .get(1)
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .unwrap_or(0);
            tokens.get(idx).cloned().unwrap_or_default()
        })
        .into_owned();

    let numeric = Regex::new(r"\$(\d+)").unwrap();
    numeric
        .replace_all(&rendered, |caps: &regex::Captures<'_>| {
            let idx = caps
                .get(1)
                .and_then(|m| m.as_str().parse::<usize>().ok())
                .unwrap_or(0);
            if idx == 0 {
                return raw.to_string();
            }
            tokens
                .get(idx.saturating_sub(1))
                .cloned()
                .unwrap_or_default()
        })
        .into_owned()
}

fn format_skill_instructions_block(skill_name: &str, skill_path: &str, instructions: &str) -> String {
    format!(
        "<skill>\n<name>{}</name>\n<path>{}</path>\n{}\n</skill>",
        skill_name, skill_path, instructions
    )
}

fn extract_latest_skill_instructions_block(
    history: Option<&Vec<ChatHistoryMessage>>,
) -> Option<String> {
    let messages = history?;
    for msg in messages.iter().rev() {
        let content = msg.content.trim();
        if content.is_empty() {
            continue;
        }
        let Some(start) = content.find("<skill>") else {
            continue;
        };
        let Some(end_tag_idx) = content.rfind("</skill>") else {
            continue;
        };
        let end = end_tag_idx + "</skill>".len();
        if end <= start || end > content.len() {
            continue;
        }
        let block = content[start..end].trim();
        if !block.is_empty() {
            return Some(block.to_string());
        }
    }
    None
}

fn apply_skill_block_to_system_prompt(base_prompt: &str, skill_block: Option<&str>) -> String {
    if let Some(block) = skill_block {
        return format!(
            "{}\n\n## Active Skill\nA skill is active in this conversation. Continue following these skill instructions unless the user explicitly switches skills.\n{}",
            base_prompt, block
        );
    }
    base_prompt.to_string()
}

fn extract_command_token(command: &str) -> String {
    let trimmed = command.trim_start();
    if trimmed.starts_with('"') {
        if let Some(end) = trimmed[1..].find('"') {
            return trimmed[1..=end].to_string();
        }
    }
    trimmed.split_whitespace().next().unwrap_or("").to_string()
}

fn command_allowed(access: &ToolAccess, command: &str) -> bool {
    if access.mode == "allow_all" {
        return true;
    }
    let token = extract_command_token(command);
    let token_lower = token.to_lowercase();
    let base_lower = Path::new(&token)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&token)
        .to_lowercase();

    for entry in &access.allowed_commands {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "*" {
            return true;
        }
        let pattern = trimmed.to_lowercase();
        if pattern.contains('*') || pattern.contains('?') {
            if let Ok(glob_pattern) = glob::Pattern::new(&pattern) {
                if glob_pattern.matches(&token_lower) || glob_pattern.matches(&base_lower) {
                    return true;
                }
            }
        } else if pattern == token_lower || pattern == base_lower {
            return true;
        }
    }
    false
}

fn truncate_string(value: &str, max_chars: usize) -> (String, bool) {
    if value.chars().count() <= max_chars {
        return (value.to_string(), false);
    }
    let truncated: String = value.chars().take(max_chars).collect();
    (truncated, true)
}

fn compact_tool_context_content(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let (truncated, cut) = truncate_string(trimmed, max_chars);
    if cut {
        format!("{}\n...(truncated for conversation history)", truncated)
    } else {
        truncated
    }
}

fn command_requests_background(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.ends_with('&') {
        return true;
    }

    let lower = trimmed.to_lowercase();
    lower.starts_with("start ")
        || lower.starts_with("cmd /c start ")
        || lower.starts_with("powershell -command \"start-process")
        || lower.starts_with("powershell -noprofile -command \"start-process")
}

fn next_background_task_id() -> String {
    let seq = BACKGROUND_TASK_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("bg-{}-{}", Local::now().timestamp_millis(), seq)
}

fn command_mentions_script(command: &str) -> bool {
    let lower = command.to_lowercase();
    if lower.contains("scripts/") || lower.contains("scripts\\") {
        return true;
    }
    let script_exts = [".ps1", ".py", ".sh", ".bat", ".cmd"];
    script_exts.iter().any(|ext| lower.contains(ext))
}

fn default_timeout_for_command(command: &str) -> u64 {
    let lower = command.trim_start().to_lowercase();
    if lower.starts_with("agent-browser ") {
        DEFAULT_AGENT_BROWSER_TIMEOUT_MS
    } else {
        DEFAULT_COMMAND_TIMEOUT_MS
    }
}

fn read_file_tool(access: &ToolAccess, args: ReadArgs) -> Result<String, String> {
    if access.mode == "unset" {
        return Err(TOOL_MODE_UNSET_ERROR.to_string());
    }
    let path = ensure_path_allowed(access, &args.path)?;
    let max_bytes = args.max_bytes.unwrap_or(DEFAULT_MAX_READ_BYTES);
    let data = fs::read(&path).map_err(|e| format!("读取失败: {}", e))?;
    let truncated = data.len() > max_bytes;
    let slice = if truncated {
        &data[..max_bytes]
    } else {
        &data[..]
    };
    let mut text = String::from_utf8_lossy(slice).to_string();
    if truncated {
        text.push_str(&format!("\n\n[truncated {} bytes]", data.len() - max_bytes));
    }
    Ok(text)
}

fn write_file_tool(access: &ToolAccess, args: WriteArgs) -> Result<String, String> {
    if access.mode == "unset" {
        return Err(TOOL_MODE_UNSET_ERROR.to_string());
    }
    let path = ensure_path_allowed(access, &args.path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    if args.append.unwrap_or(false) {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("写入失败: {}", e))?;
        file.write_all(args.content.as_bytes())
            .map_err(|e| format!("写入失败: {}", e))?;
    } else {
        fs::write(&path, args.content.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    }
    Ok(format!("写入成功: {}", path.display()))
}

fn edit_file_tool(access: &ToolAccess, args: EditArgs) -> Result<String, String> {
    if access.mode == "unset" {
        return Err(TOOL_MODE_UNSET_ERROR.to_string());
    }
    let path = ensure_path_allowed(access, &args.path)?;
    let content = fs::read_to_string(&path).map_err(|e| format!("读取失败: {}", e))?;
    let count = content.matches(&args.old).count();
    let replace_all = args.replace_all.unwrap_or(true);
    let updated = if replace_all {
        content.replace(&args.old, &args.new)
    } else {
        content.replacen(&args.old, &args.new, 1)
    };
    if updated == content {
        return Ok("未找到可替换内容".to_string());
    }
    fs::write(&path, updated.as_bytes()).map_err(|e| format!("写入失败: {}", e))?;
    Ok(format!("替换完成: {} 处", count))
}

fn glob_files_tool(access: &ToolAccess, args: GlobArgs) -> Result<String, String> {
    if access.mode == "unset" {
        return Err(TOOL_MODE_UNSET_ERROR.to_string());
    }
    let max_results = args.max_results.unwrap_or(DEFAULT_MAX_GLOB_RESULTS);
    let pattern_path = if Path::new(&args.pattern).is_absolute() {
        args.pattern.clone()
    } else {
        access
            .base_dir
            .join(&args.pattern)
            .to_string_lossy()
            .to_string()
    };

    let mut results = Vec::new();
    for entry in glob(&pattern_path).map_err(|e| format!("glob 解析失败: {}", e))? {
        if results.len() >= max_results {
            break;
        }
        if let Ok(path) = entry {
            if access.mode == "whitelist" && !path_is_allowed(access, &path) {
                continue;
            }
            results.push(path.to_string_lossy().to_string());
        }
    }

    if results.is_empty() {
        Ok("未找到匹配文件".to_string())
    } else {
        Ok(results.join("\n"))
    }
}

fn grep_files_tool(access: &ToolAccess, args: GrepArgs) -> Result<String, String> {
    if access.mode == "unset" {
        return Err(TOOL_MODE_UNSET_ERROR.to_string());
    }
    let max_results = args.max_results.unwrap_or(DEFAULT_MAX_GREP_RESULTS);
    let mut files = Vec::new();

    if let Some(path_str) = args.path.clone() {
        let path = ensure_path_allowed(access, &path_str)?;
        let filter = args
            .glob
            .as_deref()
            .and_then(|pat| glob::Pattern::new(pat).ok());
        if path.is_file() {
            if let Some(pattern) = &filter {
                if pattern.matches_path(&path) {
                    files.push(path);
                }
            } else {
                files.push(path);
            }
        } else if path.is_dir() {
            for entry in WalkDir::new(&path).into_iter().filter_map(Result::ok) {
                if !entry.file_type().is_file() {
                    continue;
                }
                if let Some(pattern) = &filter {
                    if let Ok(rel) = entry.path().strip_prefix(&path) {
                        if !pattern.matches_path(rel) {
                            continue;
                        }
                    }
                }
                files.push(entry.into_path());
            }
        }
    } else if let Some(glob_pattern) = args.glob.clone() {
        let base_dirs = if access.mode == "allow_all" {
            vec![access.base_dir.clone()]
        } else {
            access.allowed_dirs.clone()
        };
        for base in base_dirs {
            let pattern = base.join(&glob_pattern).to_string_lossy().to_string();
            for entry in glob(&pattern).map_err(|e| format!("glob 解析失败: {}", e))? {
                if let Ok(path) = entry {
                    files.push(path);
                }
            }
        }
    } else {
        let base = access.base_dir.clone();
        for entry in WalkDir::new(base).into_iter().filter_map(Result::ok) {
            if entry.file_type().is_file() {
                files.push(entry.into_path());
            }
        }
    }

    let use_regex = args.regex.unwrap_or(false);
    let case_sensitive = args.case_sensitive.unwrap_or(true);
    let regex = if use_regex {
        RegexBuilder::new(&args.pattern)
            .case_insensitive(!case_sensitive)
            .build()
            .map_err(|e| format!("正则解析失败: {}", e))?
    } else {
        RegexBuilder::new(&regex::escape(&args.pattern))
            .case_insensitive(!case_sensitive)
            .build()
            .map_err(|e| format!("正则解析失败: {}", e))?
    };

    let mut results = Vec::new();
    for path in files {
        if access.mode == "whitelist" && !path_is_allowed(access, &path) {
            continue;
        }
        if results.len() >= max_results {
            break;
        }
        if let Ok(meta) = fs::metadata(&path) {
            if meta.len() > MAX_GREP_FILE_BYTES {
                continue;
            }
        }
        let file = fs::File::open(&path).map_err(|e| format!("读取失败: {}", e))?;
        let reader = io::BufReader::new(file);
        for (idx, line) in reader.lines().enumerate() {
            if results.len() >= max_results {
                break;
            }
            let line = line.unwrap_or_default();
            if regex.is_match(&line) {
                results.push(format!("{}:{}:{}", path.to_string_lossy(), idx + 1, line));
            }
        }
    }

    if results.is_empty() {
        Ok("未找到匹配内容".to_string())
    } else {
        Ok(results.join("\n"))
    }
}

async fn run_command_tool(access: &ToolAccess, args: BashArgs) -> Result<String, String> {
    if access.mode == "unset" {
        return Err(TOOL_MODE_UNSET_ERROR.to_string());
    }
    if access.mode == "whitelist" && !command_allowed(access, &args.command) {
        return Ok("命令不在允许列表中".to_string());
    }

    let cwd = args
        .cwd
        .as_deref()
        .map(|dir| resolve_path(access, dir))
        .unwrap_or_else(|| access.base_dir.clone());

    if access.mode == "whitelist" && !path_is_allowed(access, &cwd) {
        return Ok(format!("工作目录不在允许范围内: {}", cwd.display()));
    }

    let timeout_ms = args
        .timeout_ms
        .unwrap_or_else(|| default_timeout_for_command(&args.command))
        .min(MAX_COMMAND_TIMEOUT_MS)
        .max(1_000);

    if command_requests_background(&args.command) {
        fs::create_dir_all(&access.tasks_dir)
            .map_err(|e| format!("create tasks dir failed: {}", e))?;
        let task_id = next_background_task_id();
        let output_path = access.tasks_dir.join(format!("{}.output", task_id));
        let stdout_file =
            fs::File::create(&output_path).map_err(|e| format!("create output file failed: {}", e))?;
        let stderr_file = stdout_file
            .try_clone()
            .map_err(|e| format!("prepare stderr output file failed: {}", e))?;

        let mut bg_cmd = build_shell_command(&args.command);
        bg_cmd
            .current_dir(&cwd)
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file));
        bg_cmd
            .spawn()
            .map_err(|e| format!("start background command failed: {}", e))?;

        return Ok(format!(
            "Command running in background with ID: {}. Output is being written to: {}",
            task_id,
            output_path.display()
        ));
    }

    let mut cmd = build_shell_command(&args.command);
    cmd.current_dir(&cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = timeout(TokioDuration::from_millis(timeout_ms), cmd.output())
        .await
        .map_err(|_| "命令超时".to_string())?
        .map_err(|e| format!("执行失败: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let mut response = format!("exit_code: {}\n", output.status.code().unwrap_or(-1));

    if !stdout.trim().is_empty() {
        let (truncated, cut) = truncate_string(stdout.trim_end(), MAX_COMMAND_OUTPUT_CHARS);
        response.push_str("stdout:\n");
        response.push_str(&truncated);
        if cut {
            response.push_str("\n[stdout truncated]");
        }
        response.push('\n');
    }

    if !stderr.trim().is_empty() {
        let (truncated, cut) = truncate_string(stderr.trim_end(), MAX_COMMAND_OUTPUT_CHARS);
        response.push_str("stderr:\n");
        response.push_str(&truncated);
        if cut {
            response.push_str("\n[stderr truncated]");
        }
    }

    Ok(response.trim_end().to_string())
}

#[cfg(target_os = "windows")]
fn build_shell_command(command: &str) -> TokioCommand {
    if let Some(bash_path) = find_windows_bash_path() {
        let mut cmd = TokioCommand::new(bash_path);
        cmd.arg("-lc").arg(command);
        return cmd;
    }

    let mut cmd = TokioCommand::new("cmd");
    cmd.arg("/C").arg(command);
    cmd
}

#[cfg(not(target_os = "windows"))]
fn build_shell_command(command: &str) -> TokioCommand {
    let mut cmd = TokioCommand::new("sh");
    cmd.arg("-c").arg(command);
    cmd
}

#[cfg(target_os = "windows")]
fn find_windows_bash_path() -> Option<PathBuf> {
    let cache = windows_bash_path_cache();
    {
        let guard = cache.lock().unwrap();
        if let Some(cached) = guard.as_ref() {
            return cached.clone();
        }
    }

    let detected = detect_windows_bash_path();
    {
        let mut guard = cache.lock().unwrap();
        *guard = Some(detected.clone());
    }
    detected
}

#[cfg(target_os = "windows")]
fn refresh_windows_bash_path_cache() -> Option<PathBuf> {
    let detected = detect_windows_bash_path();
    let mut guard = windows_bash_path_cache().lock().unwrap();
    *guard = Some(detected.clone());
    detected
}

#[cfg(target_os = "windows")]
fn windows_bash_path_cache() -> &'static Mutex<Option<Option<PathBuf>>> {
    static WINDOWS_BASH_PATH_CACHE: OnceLock<Mutex<Option<Option<PathBuf>>>> = OnceLock::new();
    WINDOWS_BASH_PATH_CACHE.get_or_init(|| Mutex::new(None))
}

#[cfg(target_os = "windows")]
fn detect_windows_bash_path() -> Option<PathBuf> {
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("bash.exe");
            if candidate.is_file() && !is_windows_system_bash(&candidate) {
                return Some(candidate);
            }
        }
    }

    for candidate in [
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files\Git\usr\bin\bash.exe",
    ] {
        let path = PathBuf::from(candidate);
        if path.is_file() && !is_windows_system_bash(&path) {
            return Some(path);
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn is_windows_system_bash(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('/', "\\").to_lowercase();
    normalized.ends_with("\\windows\\system32\\bash.exe")
}

fn parse_exit_code(output: &str) -> Option<i32> {
    let mut lines = output.lines();
    let first = lines.next()?.trim();
    let prefix = "exit_code:";
    if !first.starts_with(prefix) {
        return None;
    }
    first[prefix.len()..].trim().parse::<i32>().ok()
}

fn is_tool_failure(output: &str) -> bool {
    let trimmed = output.trim_start();
    if trimmed.starts_with(TOOL_ERROR_PREFIX) {
        return true;
    }
    parse_exit_code(output).map_or(false, |code| code != 0)
}

fn build_skill_execution_system_prompt(context: &str, skills_dir: &Path, skill_block: &str) -> String {
    format!(
        r#"You are executing a user-invoked skill.

{}

## Environment
- App skills directory: {}
- Use the app skills directory above for skill files.
- Do not assume ~/.kiro/skills or ~/.codex/skills.

## Execution Rules
- Follow the active skill instructions below as the primary procedure for this request.
- Prefer exact commands/URLs/paths from the skill over generic placeholders.
- If required info is missing, ask for only the minimum missing info.

{}
"#,
        context,
        skills_dir.to_string_lossy(),
        skill_block
    )
}

fn build_tool_system_prompt(
    context: &str,
    skills_dir: &Path,
    available_skills: &[SkillMetadata],
) -> String {
    // 构建可用技能列表
    let skills_section = if available_skills.is_empty() {
        "当前没有已安装的技能。你可以使用 manage_skill 工具创建新技能。".to_string()
    } else {
        let skills_list: Vec<String> = available_skills
            .iter()
            .filter(|s| is_model_invocable_skill(s))
            .map(|s| format!("- {}: {}", s.name, s.description))
            .collect();
        if skills_list.is_empty() {
            "当前没有用户可调用的技能。".to_string()
        } else {
            format!(
                "以下是已安装的技能，可通过 invoke_skill 工具调用：\n{}",
                skills_list.join("\n")
            )
        }
    };

    let context = format!(
        "{}\n\n## Environment\n- App skills directory: {}\n- Do not assume ~/.kiro/skills or ~/.codex/skills. Use the app skills directory above for skill files.",
        context,
        skills_dir.to_string_lossy()
    );
    format!(
        r#"你是一个屏幕监控助手，帮助用户回忆和理解他们的操作历史。

{}

请根据上面的操作记录回答用户的问题。如果记录中没有相关信息，请如实说明。

## 可用技能
{}

## 任务处理方式
1. 先确认目标和约束；信息不足时先问 1-2 个关键问题。
2. 判断是否需要技能/工具：
   - 有合适技能就调用 invoke_skill，传入 skill_name 参数。
   - 需要新增/调整能力就用 manage_skill（create/update），并尽量最小化 allowed_tools，选择合适 context（screen/none），必要时设置 user_invocable。
3. 需要工具时先给简短计划（1-3 步），再调用工具。
4. 工具返回后先检查错误：有失败就解释原因、换方案或请用户授权/补充信息；不要重复相同工具+参数超过 2 次。
5. 输出结果：结论 + 关键依据/步骤 + 可选下一步。
6. 若被中断/取消：给出已完成步骤、当前状态和继续所需信息。

## Execution transparency
- If the request is ambiguous, ask 1-3 clarifying questions before any tool/file/action. Do not proceed until the user answers.
- For multi-step tasks, provide a brief plan (1-3 steps) before using tools. If confirmation is needed, ask for it.
- Use the progress_update tool to report the plan and major milestones so the user can see what is happening.

## Error recovery and capability expansion
- Treat tool errors as normal; diagnose (paths/permissions/params) and retry with adjusted inputs.
- Prefer existing tools/skills first; if capability is missing, use /skill-creator or manage_skill to create/update with minimal allowed_tools.
- For new skills, ask the user which path to take: (A) find an existing skill online, or (B) wrap an open-source GitHub project as a skill. Only proceed with downloads/installs after explicit approval.
- If an external tool/project is needed, prefer mature options, ask the user for approval before download/install, then encapsulate it as a skill.
- If still blocked, report what was tried and ask for the missing info or permission.

## 你拥有以下能力
1. 如果需要某个技能完成任务，请调用 invoke_skill，skill_name 必须是上面列出的技能名称之一。
2. 如果需要创建/更新/删除技能，请调用 manage_skill。
3. 可用 Read/Write/Edit/Update/Glob/Grep 读取与搜索文件。
4. 可用 Bash/run_command 运行命令（受权限限制）。"#,
        context, skills_section
    )
}

/// Tool loop 的返回结果，包含响应文本和工具上下文
struct ToolLoopResult {
    response: String,
    tool_context: Vec<ToolContextMessage>,
}

async fn run_tool_loop(
    storage: &StorageManager,
    config: &Config,
    model_manager: &ModelManager,
    skill_manager: &SkillManager,
    system_prompt: &str,
    mut result: ChatWithToolsResult,
    available_skills: &[SkillMetadata],
    allowed_tools: &Option<Vec<String>>,
    preferred_base_dir: Option<&Path>,
    cancel_token: Option<&CancellationToken>,
    progress: Option<&ProgressEmitter>,
) -> Result<ToolLoopResult, String> {
    let access = build_tool_access(config, storage, preferred_base_dir);
    let mut loops = 0usize;
    let mut last_tool_calls: Option<Vec<(String, String)>> = None;
    let mut repeat_loops = 0usize;
    let mut collected_tool_context: Vec<ToolContextMessage> = Vec::new();

    loop {
        check_cancel(cancel_token)?;
        match result {
            ChatWithToolsResult::Text(text) => {
                if loops == 0 {
                    if let Some(progress) = progress {
                        progress.emit_info("未调用工具，直接给出回答".to_string(), None);
                    }
                }
                return Ok(ToolLoopResult {
                    response: text,
                    tool_context: collected_tool_context,
                });
            }
            ChatWithToolsResult::ToolCalls { calls, messages } => {
                if loops >= MAX_TOOL_LOOPS {
                    let pending: Vec<String> = calls
                        .iter()
                        .map(|call| call.function.name.clone())
                        .collect();
                    let pending_hint = if pending.is_empty() {
                        String::new()
                    } else {
                        format!("未执行的工具: {}", pending.join(", "))
                    };
                    return Ok(ToolLoopResult {
                        response: format!(
                            "已停止工具调用以避免循环（上限 {} 次）。{}\\n你可以：1) 缩小任务范围 2) 指定下一步要做的操作 3) 检查工具权限/路径。",
                            MAX_TOOL_LOOPS, pending_hint
                        ),
                        tool_context: collected_tool_context,
                    });
                }

                // 收集 assistant 的 tool_calls 到上下文
                let tool_call_infos: Vec<ToolCallInfo> = calls
                    .iter()
                    .map(|c| ToolCallInfo {
                        id: c.id.clone(),
                        name: c.function.name.clone(),
                        arguments: c.function.arguments.clone(),
                    })
                    .collect();
                collected_tool_context.push(ToolContextMessage {
                    role: "assistant".to_string(),
                    content: None,
                    tool_call_id: None,
                    tool_calls: Some(tool_call_infos),
                });

                let signature: Vec<(String, String)> = calls
                    .iter()
                    .map(|call| (call.function.name.clone(), call.function.arguments.clone()))
                    .collect();

                let mut tool_results = Vec::new();
                for call in &calls {
                    check_cancel(cancel_token)?;
                    let output_result = if let Some(token) = cancel_token {
                        await_with_cancel(
                            token,
                            execute_tool_call(
                                &call,
                                &access,
                                storage,
                                config,
                                model_manager,
                                skill_manager,
                                available_skills,
                                allowed_tools,
                                Some(token),
                                progress,
                            ),
                        )
                        .await
                    } else {
                        execute_tool_call(
                            &call,
                            &access,
                            storage,
                            config,
                            model_manager,
                            skill_manager,
                            available_skills,
                            allowed_tools,
                            None,
                            progress,
                        )
                        .await
                    };
                    let output = match output_result {
                        Ok(text) => text,
                        Err(err) => {
                            if err == TOOL_MODE_UNSET_ERROR || err == REQUEST_CANCELLED_ERROR {
                                return Err(err);
                            }
                            format!("{} {}", TOOL_ERROR_PREFIX, err)
                        }
                    };
                    tool_results.push((call.id.clone(), output.clone()));

                    let persisted_output =
                        compact_tool_context_content(&output, MAX_PERSISTED_TOOL_CONTEXT_CHARS);
                    collected_tool_context.push(ToolContextMessage {
                        role: "tool".to_string(),
                        content: Some(persisted_output),
                        tool_call_id: Some(call.id.clone()),
                        tool_calls: None,
                    });
                }
                let has_failure = tool_results
                    .iter()
                    .any(|(_, output)| is_tool_failure(output));
                let is_repeat = last_tool_calls.as_ref() == Some(&signature);
                if is_repeat && has_failure {
                    repeat_loops += 1;
                } else {
                    repeat_loops = 0;
                }
                last_tool_calls = Some(signature);

                if repeat_loops >= MAX_REPEAT_TOOL_LOOPS {
                    let pending: Vec<String> = calls
                        .iter()
                        .map(|call| call.function.name.clone())
                        .collect();
                    let pending_hint = if pending.is_empty() {
                        String::new()
                    } else {
                        format!("重复失败的工具: {}", pending.join(", "))
                    };
                    return Ok(ToolLoopResult {
                        response: format!(
                            "检测到工具重复失败，已暂停以避免循环。{}\\n建议：确认参数/权限，或提供替代方案与更多信息。",
                            pending_hint
                        ),
                        tool_context: collected_tool_context,
                    });
                }

                let next_result = if let Some(token) = cancel_token {
                    retry_with_cancel(token, progress, "model", || {
                        model_manager.continue_with_tool_results_filtered(
                            &config.model,
                            system_prompt,
                            messages.clone(),
                            tool_results.clone(),
                            available_skills,
                            allowed_tools,
                        )
                    })
                    .await
                } else {
                    model_manager
                        .continue_with_tool_results_filtered(
                            &config.model,
                            system_prompt,
                            messages.clone(),
                            tool_results.clone(),
                            available_skills,
                            allowed_tools,
                        )
                        .await
                };
                result = match next_result {
                    Ok(value) => value,
                    Err(err) if is_context_overflow_error(&err) => {
                        if let Some(progress) = progress {
                            progress.emit_info(
                                "Tool context too large; retrying with truncated tool output"
                                    .to_string(),
                                None,
                            );
                        }
                        let truncated_results: Vec<(String, String)> = tool_results
                            .iter()
                            .map(|(id, output)| {
                                let (shortened, truncated) = truncate_string(output, 2400);
                                let normalized = if truncated {
                                    format!("{}\n...(tool output truncated)", shortened)
                                } else {
                                    shortened
                                };
                                (id.clone(), normalized)
                            })
                            .collect();
                        if let Some(token) = cancel_token {
                            retry_with_cancel(token, progress, "model", || {
                                model_manager.continue_with_tool_results_filtered(
                                    &config.model,
                                    system_prompt,
                                    messages.clone(),
                                    truncated_results.clone(),
                                    available_skills,
                                    allowed_tools,
                                )
                            })
                            .await?
                        } else {
                            model_manager
                                .continue_with_tool_results_filtered(
                                    &config.model,
                                    system_prompt,
                                    messages.clone(),
                                    truncated_results,
                                    available_skills,
                                    allowed_tools,
                                )
                                .await?
                        }
                    }
                    Err(err) => return Err(err),
                };
                loops += 1;
            }
        }
    }
}

async fn execute_tool_call(
    tool_call: &ToolCall,
    access: &ToolAccess,
    storage: &StorageManager,
    config: &Config,
    model_manager: &ModelManager,
    skill_manager: &SkillManager,
    _available_skills: &[SkillMetadata],
    allowed_tools: &Option<Vec<String>>,
    cancel_token: Option<&CancellationToken>,
    progress: Option<&ProgressEmitter>,
) -> Result<String, String> {
    let tool_name = tool_call.function.name.as_str();
    let args_value: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)
        .map_err(|e| format!("解析工具参数失败: {}", e))?;
    check_cancel(cancel_token)?;

    let needs_skill_permission = matches!(
        tool_name,
        "Read" | "Write" | "Edit" | "Update" | "Glob" | "Grep" | "Bash" | "run_command"
    );
    if needs_skill_permission && !tool_allowed_in_skill(tool_name, allowed_tools) {
        return Err(format!("工具未被 skill 允许: {}", tool_name));
    }

    match tool_name {
        "Read" => {
            let args: ReadArgs =
                serde_json::from_value(args_value).map_err(|e| format!("Read 参数错误: {}", e))?;
            if let Some(progress) = progress {
                progress.emit_step("读取文件".to_string(), Some(args.path.clone()));
            }
            read_file_tool(access, args)
        }
        "Write" => {
            let args: WriteArgs =
                serde_json::from_value(args_value).map_err(|e| format!("Write 参数错误: {}", e))?;
            if let Some(progress) = progress {
                progress.emit_step("写入文件".to_string(), Some(args.path.clone()));
            }
            write_file_tool(access, args)
        }
        "Edit" | "Update" => {
            let args: EditArgs =
                serde_json::from_value(args_value).map_err(|e| format!("Edit 参数错误: {}", e))?;
            if let Some(progress) = progress {
                progress.emit_step("修改文件".to_string(), Some(args.path.clone()));
            }
            edit_file_tool(access, args)
        }
        "Glob" => {
            let args: GlobArgs =
                serde_json::from_value(args_value).map_err(|e| format!("Glob 参数错误: {}", e))?;
            if let Some(progress) = progress {
                let (detail, _) = truncate_string(&args.pattern, 200);
                progress.emit_step("匹配文件".to_string(), Some(detail));
            }
            glob_files_tool(access, args)
        }
        "Grep" => {
            let args: GrepArgs =
                serde_json::from_value(args_value).map_err(|e| format!("Grep 参数错误: {}", e))?;
            if let Some(progress) = progress {
                let mut detail = args.pattern.clone();
                if let Some(path) = &args.path {
                    detail = format!("{} ({})", detail, path);
                } else if let Some(glob) = &args.glob {
                    detail = format!("{} ({})", detail, glob);
                }
                let (detail, _) = truncate_string(&detail, 200);
                progress.emit_step("搜索内容".to_string(), Some(detail));
            }
            grep_files_tool(access, args)
        }
        "Bash" | "run_command" => {
            let args: BashArgs =
                serde_json::from_value(args_value).map_err(|e| format!("Bash 参数错误: {}", e))?;
            if let Some(progress) = progress {
                let (detail, _) = truncate_string(&args.command, 200);
                let step_label = if command_mentions_script(&args.command) {
                    "运行脚本"
                } else {
                    "执行命令"
                };
                progress.emit_step(step_label.to_string(), Some(detail));
            }
            run_command_tool(access, args).await
        }
        "invoke_skill" => {
            let skill_name = args_value
                .get("skill_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "缺少 skill_name 参数".to_string())?;
            let skill_args = args_value
                .get("args")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if let Some(progress) = progress {
                progress.emit_step("调用技能".to_string(), Some(format!("/{}", skill_name)));
            }
            execute_skill_internal(
                storage,
                config,
                model_manager,
                skill_manager,
                skill_name,
                skill_args,
                None,
                None,
                cancel_token,
                progress,
            )
            .await
        }
        "manage_skill" => {
            let action = args_value
                .get("action")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "缺少 action 参数".to_string())?;
            let name = args_value
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "缺少 name 参数".to_string())?;

            if let Some(progress) = progress {
                let detail = format!("{} {}", action, name);
                progress.emit_step("管理技能".to_string(), Some(detail));
            }
            let overrides = SkillFrontmatterOverrides {
                allowed_tools: parse_string_list(args_value.get("allowed_tools")),
                model: parse_optional_string(args_value.get("model")),
                context: parse_optional_string(args_value.get("context")),
                user_invocable: args_value.get("user_invocable").and_then(|v| v.as_bool()),
                disable_model_invocation: args_value
                    .get("disable_model_invocation")
                    .and_then(|v| v.as_bool()),
                metadata: parse_metadata_map(args_value.get("metadata")),
            };

            match action {
                "create" => {
                    let description = args_value
                        .get("description")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "创建技能需要 description 参数".to_string())?;
                    let instructions = args_value
                        .get("instructions")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "创建技能需要 instructions 参数".to_string())?;
                    match skill_manager.create_skill_with_meta(
                        name,
                        description,
                        instructions,
                        overrides.clone(),
                    ) {
                        Ok(_) => Ok(format!("技能 `{}` 创建成功。", name)),
                        Err(e) => Ok(format!("创建技能失败: {}", e)),
                    }
                }
                "update" => {
                    let description = args_value
                        .get("description")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "更新技能需要 description 参数".to_string())?;
                    let instructions = args_value
                        .get("instructions")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "更新技能需要 instructions 参数".to_string())?;
                    match skill_manager.update_skill_with_meta(
                        name,
                        description,
                        instructions,
                        overrides.clone(),
                    ) {
                        Ok(_) => Ok(format!("技能 `{}` 更新成功。", name)),
                        Err(e) => Ok(format!("更新技能失败: {}", e)),
                    }
                }
                "delete" => match skill_manager.delete_skill(name) {
                    Ok(_) => Ok(format!("技能 `{}` 已删除。", name)),
                    Err(e) => Ok(format!("删除技能失败: {}", e)),
                },
                _ => Ok(format!("未知操作: {}", action)),
            }
        }
        "progress_update" => {
            let message = args_value
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing message parameter".to_string())?;
            let detail = args_value
                .get("detail")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            if let Some(progress) = progress {
                progress.emit_info(message.to_string(), detail);
            }
            Ok("ok".to_string())
        }
        _ => Ok(format!("未知工具: {}", tool_name)),
    }
}

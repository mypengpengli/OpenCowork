use crate::capture::CaptureManager;
use crate::model::{ModelManager, ChatWithToolsResult, ToolCall};
use crate::storage::{Config, StorageManager, SummaryRecord, SearchQuery, TimeRange};
use crate::skills::{SkillManager, SkillMetadata, Skill};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{Duration, Local, NaiveDateTime, TimeZone};
use std::collections::HashSet;
use std::fs;
use std::io::{self, BufRead};
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tauri_plugin_shell::ShellExt;
use tokio::process::Command as TokioCommand;
use tokio::sync::Mutex as TokioMutex;
use tokio::time::{timeout, Duration as TokioDuration};
use glob::glob;
use regex::RegexBuilder;
use walkdir::WalkDir;

pub struct AppState {
    pub capture_manager: Arc<TokioMutex<CaptureManager>>,
    pub storage_manager: Arc<StorageManager>,
}

const MIN_RECENT_DETAIL_RECORDS: usize = 20;
const RELEASE_PAGE_URL: &str = "https://github.com/mypengpengli/OpenCowork/releases/latest";
const TOOL_MODE_UNSET_ERROR: &str = "TOOLS_MODE_UNSET";
const MAX_TOOL_LOOPS: usize = 999;
const DEFAULT_MAX_READ_BYTES: usize = 200_000;
const DEFAULT_MAX_GLOB_RESULTS: usize = 500;
const DEFAULT_MAX_GREP_RESULTS: usize = 200;
const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 120_000;
const MAX_COMMAND_OUTPUT_CHARS: usize = 20_000;
const MAX_GREP_FILE_BYTES: u64 = 2_000_000;

impl AppState {
    pub fn new() -> Self {
        Self {
            capture_manager: Arc::new(TokioMutex::new(CaptureManager::new())),
            storage_manager: Arc::new(StorageManager::new()),
        }
    }
}

#[tauri::command]
pub async fn get_config() -> Result<Config, String> {
    let storage = StorageManager::new();
    storage.load_config().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_system_locale() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        match windows_ui_is_zh() {
            Some(is_zh) => {
                let resolved = if is_zh { "zh".to_string() } else { "en".to_string() };
                println!("[locale] get_system_locale windows_ui_is_zh={} -> {}", is_zh, resolved);
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
    storage.save_profile(&name, &config).map_err(|e| e.to_string())
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
}

#[tauri::command]
pub async fn chat_with_assistant(
    message: String,
    history: Option<Vec<ChatHistoryMessage>>,
    attachments: Option<Vec<AttachmentInput>>,
) -> Result<String, String> {
    let storage = StorageManager::new();
    let config = storage.load_config().map_err(|e| e.to_string())?;
    let model_manager = ModelManager::new();
    let skill_manager = SkillManager::new();

    // 获取可用 skills 列表（用于自动发现和 Tool Use）
    let available_skills = skill_manager.discover_skills().unwrap_or_default();

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
        let fallback = storage.get_recent_records(
            MIN_RECENT_DETAIL_RECORDS,
            config.storage.retention_days,
        );
        if !fallback.is_empty() {
            search_result.records = merge_recent_records(
                search_result.records,
                fallback,
                MIN_RECENT_DETAIL_RECORDS,
            );
        }
    }

    // 构建上下文（使用配置中的最大字符数）
    let context = search_result.build_context(config.storage.max_context_chars, query.include_detail);

    // 注入启用的全局提示词
    let context = build_context_with_global_prompts(&config, context);

    // 处理附件内容
    let attachment_payload = attachments
        .as_deref()
        .map(build_attachment_payload)
        .unwrap_or_default();
    let has_attachments = attachments.as_ref().map_or(false, |items| !items.is_empty());
    let user_message = merge_user_message(&message, &attachment_payload.text, has_attachments);

    // 使用 API 模式时启用 Tool Use
    if config.model.provider == "api" {
        let system_prompt = build_tool_system_prompt(&context);
        let result = if attachment_payload.image_urls.is_empty()
            && attachment_payload.image_base64.is_empty()
        {
            model_manager
                .chat_with_tools_with_system_prompt(
                    &config.model,
                    &system_prompt,
                    &user_message,
                    history.clone(),
                    &available_skills,
                )
                .await?
        } else {
            model_manager
                .chat_with_tools_with_system_prompt_with_images(
                    &config.model,
                    &system_prompt,
                    &user_message,
                    history.clone(),
                    &available_skills,
                    attachment_payload.image_urls.clone(),
                    attachment_payload.image_base64.clone(),
                )
                .await?
        };

        return run_tool_loop(
            &storage,
            &config,
            &model_manager,
            &skill_manager,
            &system_prompt,
            result,
            &available_skills,
            &None,
        )
        .await;
    }

    // 回退到普通对话（无 Tool Use 或 Ollama 模式）
    let skills_hint = if !available_skills.is_empty() {
        let skills_list: Vec<String> = available_skills
            .iter()
            .filter(|s| s.user_invocable.unwrap_or(true))
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
    if attachment_payload.image_urls.is_empty() && attachment_payload.image_base64.is_empty() {
        model_manager
            .chat_with_history(&config.model, &context_with_skills, &user_message, history)
            .await
    } else {
        model_manager
            .chat_with_history_with_images(
                &config.model,
                &context_with_skills,
                &user_message,
                history,
                attachment_payload.image_urls,
                attachment_payload.image_base64,
            )
            .await
    }
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
) -> Result<String, String> {
    // 加载 skill
    let skill = skill_manager.load_skill(skill_name)?;

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
    let has_attachments = attachments.as_ref().map_or(false, |items| !items.is_empty());
    let user_message = merge_user_message(&base_message, &attachment_payload.text, has_attachments);

    // 获取屏幕记录上下文
    let query = parse_user_query(&args.unwrap_or_default());
    let search_result = storage.smart_search(&query).unwrap_or_default();
    let screen_context = search_result.build_context(config.storage.max_context_chars, true);

    // 获取启用的全局提示词
    let global_prompts_section = build_global_prompts_section(config);

    let skill_dir = Path::new(&skill.path)
        .parent()
        .unwrap_or_else(|| Path::new(&skill.path));
    let skill_dir_display = skill_dir.to_string_lossy();
    let allowed_tools_hint = match &skill.metadata.allowed_tools {
        Some(list) if !list.is_empty() => format!("\n## 允许的工具\n{}\n", list.join(", ")),
        _ => String::new(),
    };

    // 构建 system prompt，注入 skill 指令
    let system_prompt = format!(
        r#"你是一个屏幕监控助手。现在用户调用了技能 "{}"。
{}
## 技能说明
{}

## 技能指令
{}

## 技能目录
{}
该目录下可能有 scripts/ references/ assets/ 。如果需要运行脚本，请使用 Bash/run_command 工具，并把 cwd 设置为技能目录。{}

## 屏幕活动记录
{}

请根据技能指令和屏幕活动记录，完成用户的请求。"#,
        skill.metadata.name,
        global_prompts_section,
        skill.metadata.description,
        skill.instructions,
        skill_dir_display,
        allowed_tools_hint,
        screen_context
    );

    if config.model.provider == "api" {
        let available_skills = skill_manager.discover_skills().unwrap_or_default();
        let result = if attachment_payload.image_urls.is_empty()
            && attachment_payload.image_base64.is_empty()
        {
            model_manager
                .chat_with_tools_with_system_prompt(
                    &config.model,
                    &system_prompt,
                    &user_message,
                    history.clone(),
                    &available_skills,
                )
                .await?
        } else {
            model_manager
                .chat_with_tools_with_system_prompt_with_images(
                    &config.model,
                    &system_prompt,
                    &user_message,
                    history.clone(),
                    &available_skills,
                    attachment_payload.image_urls.clone(),
                    attachment_payload.image_base64.clone(),
                )
                .await?
        };

        return Box::pin(run_tool_loop(
            storage,
            config,
            model_manager,
            skill_manager,
            &system_prompt,
            result,
            &available_skills,
            &skill.metadata.allowed_tools,
        ))
        .await;
    }

    if attachment_payload.image_urls.is_empty() && attachment_payload.image_base64.is_empty() {
        model_manager
            .chat_with_system_prompt(&config.model, &system_prompt, &user_message, history)
            .await
    } else {
        model_manager
            .chat_with_system_prompt_with_images(
                &config.model,
                &system_prompt,
                &user_message,
                history,
                attachment_payload.image_urls,
                attachment_payload.image_base64,
            )
            .await
    }
}

/// 解析用户问题，提取时间范围和关键词
fn parse_user_query(message: &str) -> SearchQuery {
    let msg_lower = message.to_lowercase();

    // 提取时间范围
    let time_range = if msg_lower.contains("刚才") || msg_lower.contains("刚刚") {
        TimeRange::Recent(5)  // 最近5分钟
    } else if msg_lower.contains("最近") && msg_lower.contains("分钟") {
        // 尝试提取分钟数
        let minutes = extract_number(&msg_lower).unwrap_or(10);
        TimeRange::Recent(minutes)
    } else if msg_lower.contains("今天") || msg_lower.contains("上午") || msg_lower.contains("下午") {
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
        ("一", 1), ("二", 2), ("三", 3), ("四", 4), ("五", 5),
        ("六", 6), ("七", 7), ("八", 8), ("九", 9), ("十", 10),
        ("十五", 15), ("二十", 20), ("三十", 30),
    ];

    for (cn, num) in cn_nums {
        if text.contains(cn) {
            return Some(num);
        }
    }

    // 阿拉伯数字
    let re = regex::Regex::new(r"\d+").ok()?;
    re.find(text)
        .and_then(|m| m.as_str().parse().ok())
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
        "error", "错误", "报错", "bug", "异常",
        "代码", "文件", "函数", "编辑", "修改",
        ".rs", ".ts", ".js", ".py", ".vue", ".tsx",
        "Chrome", "VS Code", "Terminal",
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
        "详细", "细节", "具体", "截图", "画面", "界面", "内容", "显示", "文本", "按钮", "输入", "输出",
        "哪一页", "哪个页面", "哪一个文件", "哪行", "哪一行", "日志", "报错内容",
        "报错", "错误", "失败", "异常", "无法", "连不上", "连接不上", "原因", "为什么", "提示", "配置",
        "detail", "details", "screenshot", "screen", "page", "error log",
    ];

    triggers.iter().any(|kw| msg.contains(kw))
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
    let enabled_prompts: Vec<&str> = config.global_prompt.items
        .iter()
        .filter(|item| item.enabled && !item.content.trim().is_empty())
        .map(|item| item.content.as_str())
        .collect();

    if enabled_prompts.is_empty() {
        String::new()
    } else {
        format!(
            "## 用户预设信息\n{}\n\n",
            enabled_prompts.join("\n\n")
        )
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
    storage.delete_summaries_for_date(&date).map_err(|e| e.to_string())
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

#[tauri::command]
pub async fn open_release_page(app_handle: AppHandle) -> Result<(), String> {
    app_handle
        .shell()
        .open(RELEASE_PAGE_URL.to_string(), None)
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
pub struct AlertRecord {
    pub timestamp: String,
    pub issue_type: String,
    pub message: String,
    pub suggestion: String,
    pub confidence: f32,
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
        });
    }

    Ok(alerts)
}

// ==================== Skills 相关命令 ====================

/// 列出所有可用的 skills
#[tauri::command]
pub async fn list_skills() -> Result<Vec<SkillMetadata>, String> {
    let skill_manager = SkillManager::new();
    skill_manager.discover_skills()
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
) -> Result<String, String> {
    let storage = StorageManager::new();
    let config = storage.load_config().map_err(|e| e.to_string())?;
    let model_manager = ModelManager::new();
    let skill_manager = SkillManager::new();
    execute_skill_internal(
        &storage,
        &config,
        &model_manager,
        &skill_manager,
        &name,
        args,
        history,
        attachments,
    )
    .await
}

/// 创建新的 skill
#[tauri::command]
pub async fn create_skill(
    name: String,
    description: String,
    instructions: String,
) -> Result<(), String> {
    let skill_manager = SkillManager::new();
    skill_manager.create_skill(&name, &description, &instructions)
}

/// 删除 skill
#[tauri::command]
pub async fn delete_skill(name: String) -> Result<(), String> {
    let skill_manager = SkillManager::new();
    skill_manager.delete_skill(&name)
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
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("创建 skills 目录失败: {}", e))?;
    }

    let dir_str = dir.to_string_lossy().to_string();
    app_handle
        .shell()
        .open(dir_str, None)
        .map_err(|e| e.to_string())
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
        let is_text_doc = matches!(attachment.kind.as_deref(), Some("document")) || is_text_doc_ext(&ext);

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

fn build_tool_access(config: &Config, storage: &StorageManager) -> ToolAccess {
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

    let base_dir = allowed_dirs
        .get(0)
        .cloned()
        .unwrap_or_else(|| normalize_path(&data_dir));

    ToolAccess {
        mode,
        allowed_commands: config.tools.allowed_commands.clone(),
        allowed_dirs,
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
        let name = trimmed
            .split('(')
            .next()
            .unwrap_or(trimmed)
            .trim();
        if normalize_tool_name(name) == target {
            return true;
        }
    }
    false
}

fn normalize_tool_name(name: &str) -> String {
    match name.trim().to_lowercase().as_str() {
        "update" => "edit".to_string(),
        "run_command" => "bash".to_string(),
        other => other.to_string(),
    }
}

fn extract_command_token(command: &str) -> String {
    let trimmed = command.trim_start();
    if trimmed.starts_with('"') {
        if let Some(end) = trimmed[1..].find('"') {
            return trimmed[1..=end].to_string();
        }
    }
    trimmed
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
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

fn read_file_tool(access: &ToolAccess, args: ReadArgs) -> Result<String, String> {
    if access.mode == "unset" {
        return Err(TOOL_MODE_UNSET_ERROR.to_string());
    }
    let path = ensure_path_allowed(access, &args.path)?;
    let max_bytes = args.max_bytes.unwrap_or(DEFAULT_MAX_READ_BYTES);
    let data = fs::read(&path).map_err(|e| format!("读取失败: {}", e))?;
    let truncated = data.len() > max_bytes;
    let slice = if truncated { &data[..max_bytes] } else { &data[..] };
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
                results.push(format!(
                    "{}:{}:{}",
                    path.to_string_lossy(),
                    idx + 1,
                    line
                ));
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

    let timeout_ms = args.timeout_ms.unwrap_or(DEFAULT_COMMAND_TIMEOUT_MS);

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
    let mut response = format!(
        "exit_code: {}\n",
        output.status.code().unwrap_or(-1)
    );

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

fn build_tool_system_prompt(context: &str) -> String {
    format!(
        r#"你是一个屏幕监控助手，帮助用户回顾和理解他们的操作历史。

{}

请根据上述操作记录，回答用户的问题。如果记录中没有相关信息，请如实告知。

你有以下能力：
1. 如果用户的请求需要使用某个技能来完成，请调用 invoke_skill 工具。
2. 如果用户想要创建、修改或删除技能，请调用 manage_skill 工具。
3. 你可以使用 Read/Write/Edit/Update/Glob/Grep 工具读写和搜索文件。
4. 你可以使用 Bash/run_command 工具运行命令（受权限限制）。"#,
        context
    )
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
) -> Result<String, String> {
    let access = build_tool_access(config, storage);
    let mut loops = 0usize;

    loop {
        match result {
            ChatWithToolsResult::Text(text) => return Ok(text),
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
                    return Ok(format!(
                        "提示: 工具调用已达到上限({}次)，本次不再继续调用工具。{}",
                        MAX_TOOL_LOOPS, pending_hint
                    ));
                }

                let mut tool_results = Vec::new();
                for call in calls {
                    let output = execute_tool_call(
                        &call,
                        &access,
                        storage,
                        config,
                        model_manager,
                        skill_manager,
                        available_skills,
                        allowed_tools,
                    )
                    .await?;
                    tool_results.push((call.id.clone(), output));
                }

                result = model_manager
                    .continue_with_tool_results(
                        &config.model,
                        system_prompt,
                        messages,
                        tool_results,
                        available_skills,
                    )
                    .await?;
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
) -> Result<String, String> {
    let tool_name = tool_call.function.name.as_str();
    let args_value: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)
        .map_err(|e| format!("解析工具参数失败: {}", e))?;

    let needs_skill_permission = matches!(
        tool_name,
        "Read" | "Write" | "Edit" | "Update" | "Glob" | "Grep" | "Bash" | "run_command"
    );
    if needs_skill_permission && !tool_allowed_in_skill(tool_name, allowed_tools) {
        return Ok(format!("工具未被 skill 允许: {}", tool_name));
    }

    match tool_name {
        "Read" => {
            let args: ReadArgs = serde_json::from_value(args_value)
                .map_err(|e| format!("Read 参数错误: {}", e))?;
            read_file_tool(access, args)
        }
        "Write" => {
            let args: WriteArgs = serde_json::from_value(args_value)
                .map_err(|e| format!("Write 参数错误: {}", e))?;
            write_file_tool(access, args)
        }
        "Edit" | "Update" => {
            let args: EditArgs = serde_json::from_value(args_value)
                .map_err(|e| format!("Edit 参数错误: {}", e))?;
            edit_file_tool(access, args)
        }
        "Glob" => {
            let args: GlobArgs = serde_json::from_value(args_value)
                .map_err(|e| format!("Glob 参数错误: {}", e))?;
            glob_files_tool(access, args)
        }
        "Grep" => {
            let args: GrepArgs = serde_json::from_value(args_value)
                .map_err(|e| format!("Grep 参数错误: {}", e))?;
            grep_files_tool(access, args)
        }
        "Bash" | "run_command" => {
            let args: BashArgs = serde_json::from_value(args_value)
                .map_err(|e| format!("Bash 参数错误: {}", e))?;
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

            execute_skill_internal(
                storage,
                config,
                model_manager,
                skill_manager,
                skill_name,
                skill_args,
                None,
                None,
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
                    match skill_manager.create_skill(name, description, instructions) {
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
                    match skill_manager.update_skill(name, description, instructions) {
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
        _ => Ok(format!("未知工具: {}", tool_name)),
    }
}

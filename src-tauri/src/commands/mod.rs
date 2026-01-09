use crate::capture::CaptureManager;
use crate::model::ModelManager;
use crate::storage::{Config, StorageManager, SummaryRecord, SearchQuery, TimeRange};
use chrono::{Duration, Local, NaiveDateTime, TimeZone};
use std::collections::HashSet;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tauri_plugin_shell::ShellExt;
use tokio::sync::Mutex as TokioMutex;

pub struct AppState {
    pub capture_manager: Arc<TokioMutex<CaptureManager>>,
    pub storage_manager: Arc<StorageManager>,
}

const MIN_RECENT_DETAIL_RECORDS: usize = 20;

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

#[derive(serde::Deserialize)]
pub struct ChatHistoryMessage {
    pub role: String,
    pub content: String,
}

#[tauri::command]
pub async fn chat_with_assistant(
    message: String,
    history: Option<Vec<ChatHistoryMessage>>,
) -> Result<String, String> {
    let storage = StorageManager::new();
    let config = storage.load_config().map_err(|e| e.to_string())?;
    let model_manager = ModelManager::new();

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

    // 调用模型（传递对话历史）
    model_manager
        .chat_with_history(&config.model, &context, &message, history)
        .await
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

use crate::capture::CaptureManager;
use crate::model::ModelManager;
use crate::storage::{Config, StorageManager, SummaryRecord, SearchQuery, TimeRange};
use std::sync::Arc;
use tauri::{AppHandle, State};
use tokio::sync::Mutex as TokioMutex;

pub struct AppState {
    pub capture_manager: Arc<TokioMutex<CaptureManager>>,
    pub storage_manager: Arc<StorageManager>,
}

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
pub async fn test_model_connection() -> Result<(), String> {
    let storage = StorageManager::new();
    let config = storage.load_config().map_err(|e| e.to_string())?;
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

#[tauri::command]
pub async fn chat_with_assistant(message: String) -> Result<String, String> {
    let storage = StorageManager::new();
    let config = storage.load_config().map_err(|e| e.to_string())?;
    let model_manager = ModelManager::new();

    // 分析用户问题，提取时间范围和关键词
    let query = parse_user_query(&message);

    // 智能检索相关记录
    let search_result = storage.smart_search(&query)?;

    // 构建上下文（使用配置中的最大字符数）
    let context = search_result.build_context(config.storage.max_context_chars);

    // 调用模型
    model_manager
        .chat(&config.model, &context, &message)
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

    SearchQuery {
        time_range,
        keywords,
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

#[tauri::command]
pub async fn get_summaries(date: String) -> Result<Vec<SummaryRecord>, String> {
    let storage = StorageManager::new();
    storage.get_summaries(&date).map_err(|e| e.to_string())
}

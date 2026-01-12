use crate::capture::CaptureManager;
use crate::model::{ModelManager, ChatWithToolsResult};
use crate::storage::{Config, StorageManager, SummaryRecord, SearchQuery, TimeRange};
use crate::skills::{SkillManager, SkillMetadata, Skill};
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

#[derive(serde::Deserialize, Clone)]
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

    // 使用 API 模式时启用 Tool Use
    if config.model.provider == "api" {
        // 使用 Tool Use 进行对话
        let result = model_manager
            .chat_with_tools(&config.model, &context, &message, history.clone(), &available_skills)
            .await?;

        match result {
            ChatWithToolsResult::Text(text) => {
                return Ok(text);
            }
            ChatWithToolsResult::ToolCalls(tool_calls) => {
                // 处理工具调用
                let mut final_response = String::new();

                for tool_call in tool_calls {
                    match tool_call.function.name.as_str() {
                        "invoke_skill" => {
                            // 解析参数
                            let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)
                                .map_err(|e| format!("解析工具参数失败: {}", e))?;

                            let skill_name = args.get("skill_name")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| "缺少 skill_name 参数".to_string())?;

                            let skill_args = args.get("args")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());

                            // 执行 skill
                            let skill_result = execute_skill_internal(
                                &storage,
                                &config,
                                &model_manager,
                                &skill_manager,
                                skill_name,
                                skill_args,
                                history.clone(),
                            ).await?;

                            // 将 skill 结果作为最终响应
                            final_response = format!("🔧 已调用技能 `/{}`\n\n{}", skill_name, skill_result);
                        }
                        "manage_skill" => {
                            // 解析参数
                            let args: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)
                                .map_err(|e| format!("解析工具参数失败: {}", e))?;

                            let action = args.get("action")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| "缺少 action 参数".to_string())?;

                            let name = args.get("name")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| "缺少 name 参数".to_string())?;

                            match action {
                                "create" => {
                                    let description = args.get("description")
                                        .and_then(|v| v.as_str())
                                        .ok_or_else(|| "创建技能需要 description 参数".to_string())?;
                                    let instructions = args.get("instructions")
                                        .and_then(|v| v.as_str())
                                        .ok_or_else(|| "创建技能需要 instructions 参数".to_string())?;

                                    match skill_manager.create_skill(name, description, instructions) {
                                        Ok(_) => {
                                            final_response = format!(
                                                "✅ 技能 `{}` 创建成功！\n\n**描述**: {}\n\n你现在可以通过 `/{name}` 来调用它。",
                                                name, description
                                            );
                                        }
                                        Err(e) => {
                                            final_response = format!("❌ 创建技能失败: {}", e);
                                        }
                                    }
                                }
                                "update" => {
                                    let description = args.get("description")
                                        .and_then(|v| v.as_str())
                                        .ok_or_else(|| "更新技能需要 description 参数".to_string())?;
                                    let instructions = args.get("instructions")
                                        .and_then(|v| v.as_str())
                                        .ok_or_else(|| "更新技能需要 instructions 参数".to_string())?;

                                    match skill_manager.update_skill(name, description, instructions) {
                                        Ok(_) => {
                                            final_response = format!(
                                                "✅ 技能 `{}` 更新成功！\n\n**新描述**: {}",
                                                name, description
                                            );
                                        }
                                        Err(e) => {
                                            final_response = format!("❌ 更新技能失败: {}", e);
                                        }
                                    }
                                }
                                "delete" => {
                                    match skill_manager.delete_skill(name) {
                                        Ok(_) => {
                                            final_response = format!("✅ 技能 `{}` 已删除。", name);
                                        }
                                        Err(e) => {
                                            final_response = format!("❌ 删除技能失败: {}", e);
                                        }
                                    }
                                }
                                _ => {
                                    final_response = format!("❌ 未知操作: {}", action);
                                }
                            }
                        }
                        _ => {}
                    }
                }

                if !final_response.is_empty() {
                    return Ok(final_response);
                }
            }
        }
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
    model_manager
        .chat_with_history(&config.model, &context_with_skills, &message, history)
        .await
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
) -> Result<String, String> {
    // 加载 skill
    let skill = skill_manager.load_skill(skill_name)?;

    // 构建用户消息（包含参数）
    let user_message = if let Some(ref args_str) = args {
        format!("执行技能 /{}: {}", skill_name, args_str)
    } else {
        format!("执行技能 /{}", skill_name)
    };

    // 获取屏幕记录上下文
    let query = parse_user_query(&args.unwrap_or_default());
    let search_result = storage.smart_search(&query).unwrap_or_default();
    let screen_context = search_result.build_context(config.storage.max_context_chars, true);

    // 构建 system prompt，注入 skill 指令
    let system_prompt = format!(
        r#"你是一个屏幕监控助手。现在用户调用了技能 "{}"。

## 技能说明
{}

## 技能指令
{}

## 屏幕活动记录
{}

请根据技能指令和屏幕活动记录，完成用户的请求。"#,
        skill.metadata.name,
        skill.metadata.description,
        skill.instructions,
        screen_context
    );

    // 调用模型
    model_manager
        .chat_with_system_prompt(&config.model, &system_prompt, &user_message, history)
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
) -> Result<String, String> {
    let storage = StorageManager::new();
    let config = storage.load_config().map_err(|e| e.to_string())?;
    let model_manager = ModelManager::new();
    let skill_manager = SkillManager::new();

    // 加载 skill
    let skill = skill_manager.load_skill(&name)?;

    // 构建用户消息（包含参数）
    let user_message = if let Some(ref args_str) = args {
        format!("执行技能 /{}: {}", name, args_str)
    } else {
        format!("执行技能 /{}", name)
    };

    // 获取屏幕记录上下文
    let query = parse_user_query(&args.unwrap_or_default());
    let search_result = storage.smart_search(&query).unwrap_or_default();
    let screen_context = search_result.build_context(config.storage.max_context_chars, true);

    // 构建 system prompt，注入 skill 指令
    let system_prompt = format!(
        r#"你是一个屏幕监控助手。现在用户调用了技能 "{}"。

## 技能说明
{}

## 技能指令
{}

## 屏幕活动记录
{}

请根据技能指令和屏幕活动记录，完成用户的请求。"#,
        skill.metadata.name,
        skill.metadata.description,
        skill.instructions,
        screen_context
    );

    // 调用模型
    model_manager
        .chat_with_system_prompt(&config.model, &system_prompt, &user_message, history)
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

mod screen;
mod scheduler;

pub use screen::*;
pub use scheduler::*;

use crate::model::{build_model_error_alert, ModelManager};
use crate::storage::{Config, StorageManager, SummaryRecord};
use chrono::{DateTime, Duration, Local};
use image::DynamicImage;
use parking_lot::Mutex as ParkingMutex;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

const RECENT_CONTEXT_MINUTES: i64 = 3;

pub struct CaptureManager {
    is_running: Arc<ParkingMutex<bool>>,
    record_count: Arc<ParkingMutex<u64>>,
    skip_count: Arc<ParkingMutex<u64>>,  // 跳过的帧数
    stop_tx: Option<mpsc::Sender<()>>,
    recent_alerts: Arc<ParkingMutex<HashMap<String, DateTime<Local>>>>,
    last_issue_key: Arc<ParkingMutex<Option<String>>>,
}

impl CaptureManager {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(ParkingMutex::new(false)),
            record_count: Arc::new(ParkingMutex::new(0)),
            skip_count: Arc::new(ParkingMutex::new(0)),
            stop_tx: None,
            recent_alerts: Arc::new(ParkingMutex::new(HashMap::new())),
            last_issue_key: Arc::new(ParkingMutex::new(None)),
        }
    }

    pub fn is_running(&self) -> bool {
        *self.is_running.lock()
    }

    pub fn get_count(&self) -> u64 {
        *self.record_count.lock()
    }

    pub fn get_skip_count(&self) -> u64 {
        *self.skip_count.lock()
    }

    pub async fn start(&mut self, config: Config, app_handle: AppHandle) {
        if self.is_running() {
            return;
        }

        let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
        self.stop_tx = Some(stop_tx);

        let is_running = self.is_running.clone();
        let record_count = self.record_count.clone();
        let skip_count = self.skip_count.clone();
        let recent_alerts = self.recent_alerts.clone();
        let last_issue_key = self.last_issue_key.clone();
        let interval_ms = config.capture.interval_ms;

        *is_running.lock() = true;

        tokio::spawn(async move {
            let model_manager = ModelManager::new();
            let storage_manager = StorageManager::new();
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_millis(interval_ms)
            );

            // 上一帧的图像哈希（用于对比）
            let mut prev_image_hash: Option<u64> = None;
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if !*is_running.lock() {
                            break;
                        }

                        // 执行截屏和识别
                        match capture_and_analyze_with_diff(
                            &config,
                            &model_manager,
                            &storage_manager,
                            &recent_alerts,
                            &last_issue_key,
                            &app_handle,
                            &mut prev_image_hash,
                        ).await {
                            Ok(analyzed) => {
                                if analyzed {
                                    *record_count.lock() += 1;
                                } else {
                                    *skip_count.lock() += 1;
                                }
                            }
                            Err(e) => {
                                eprintln!("截屏分析失败: {}", e);
                            }
                        }

                    }
                    _ = stop_rx.recv() => {
                        break;
                    }
                }
            }

            *is_running.lock() = false;
        });
    }

    pub async fn stop(&mut self) {
        *self.is_running.lock() = false;
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(()).await;
        }
    }
}

/// 计算图像的简单哈希值（用于快速对比）
fn compute_image_hash(image: &DynamicImage) -> u64 {
    // 缩小图像到8x8进行快速哈希
    let small = image.resize_exact(8, 8, image::imageops::FilterType::Nearest);
    let gray = small.to_luma8();

    let pixels: Vec<u8> = gray.pixels().map(|p| p.0[0]).collect();
    let avg: u64 = pixels.iter().map(|&p| p as u64).sum::<u64>() / pixels.len() as u64;

    // 生成感知哈希
    let mut hash: u64 = 0;
    for (i, &pixel) in pixels.iter().enumerate() {
        if pixel as u64 > avg {
            hash |= 1 << i;
        }
    }
    hash
}

fn save_screenshot(
    storage_manager: &StorageManager,
    image: &DynamicImage,
    now: &DateTime<Local>,
    quality: u8,
) -> Option<String> {
    let dir = match storage_manager.screenshots_dir() {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("获取截图目录失败: {}", err);
            return None;
        }
    };

    let filename = format!("{}.jpg", now.format("%Y%m%d-%H%M%S-%.3f"));
    let path = dir.join(&filename);
    let path_str = path.to_string_lossy();

    if let Err(err) = ScreenCapture::save_to_file(image, path_str.as_ref(), quality) {
        eprintln!("保存截图失败: {}", err);
        return None;
    }

    Some(filename)
}

/// 计算两个哈希的相似度 (0.0 - 1.0)
fn hash_similarity(hash1: u64, hash2: u64) -> f32 {
    let xor = hash1 ^ hash2;
    let diff_bits = xor.count_ones();
    1.0 - (diff_bits as f32 / 64.0)
}

/// 截屏并分析，支持跳过无变化的帧
async fn capture_and_analyze_with_diff(
    config: &Config,
    model_manager: &ModelManager,
    storage_manager: &StorageManager,
    recent_alerts: &Arc<ParkingMutex<HashMap<String, DateTime<Local>>>>,
    last_issue_key: &Arc<ParkingMutex<Option<String>>>,
    app_handle: &AppHandle,
    prev_hash: &mut Option<u64>,
) -> Result<bool, String> {
    // 1. 截屏
    let image = ScreenCapture::capture_primary()?;
    let now = Local::now();

    // 2. 如果启用了跳过无变化，进行对比
    if config.capture.skip_unchanged {
        let current_hash = compute_image_hash(&image);

        if let Some(prev) = *prev_hash {
            let similarity = hash_similarity(prev, current_hash);

            // 如果相似度超过阈值，跳过这一帧
            if similarity >= config.capture.change_threshold {
                return Ok(false);  // 返回false表示跳过
            }
        }

        // 更新上一帧哈希
        *prev_hash = Some(current_hash);
    }

    // 3. 保存截图
    let screenshot_ref = save_screenshot(storage_manager, &image, &now, config.capture.compress_quality);

    // 4. 转换为 base64
    let image_base64 = ScreenCapture::image_to_base64(&image, config.capture.compress_quality)?;

    // 5. 发送给大模型识别
    let recent_context = build_recent_summary_context(
        storage_manager,
        config.capture.recent_summary_limit,
        config.capture.recent_detail_limit,
    );
    let prompt = format!(
        r#"你是屏幕截图分析器。请严格只输出一个可解析的 JSON 对象，不要输出任何解释、Markdown 或代码块。

必须包含以下字段：
{{
  "summary": "30-50字的操作概述，描述用户正在做什么、使用什么工具、处理什么内容",
  "detail": "对画面的详细描述：包含主要窗口/界面区域、可见文本、按钮、输入输出、错误提示等具体细节",
  "app": "主要应用或窗口名称，无法判断写 Unknown",
  "has_issue": true 或 false（布尔值）,
  "issue_type": "问题类型（仅在 has_issue 为 true 时填写，否则空字符串）",
  "issue_summary": "问题摘要（仅在 has_issue 为 true 时填写，否则空字符串）",
  "suggestion": "解决建议（仅在 has_issue 为 true 时填写，否则空字符串）：根据 detail 中的错误信息，指出最可能的原因，并给出具体可操作的解决步骤",
  "confidence": 对整体分析结果准确性的置信度，0.0-1.0 之间的数值
}}

示例输出：
{{
  "summary": "在 VS Code 中编辑 OpenCowork 项目的 Rust 后端代码，正在修改 capture 模块的截图分析提示词",
  "detail": "VS Code 编辑器窗口最大化显示。左侧资源管理器展开 src-tauri/src/capture 目录，当前打开文件为 mod.rs。编辑区域显示第 215-260 行的 Rust 代码，包含 format! 宏和 JSON 字符串。光标位于第 238 行。右上角显示 Git 分支为 master。底部状态栏显示 UTF-8 编码、LF 换行符、Rust 语言模式。底部终端面板已折叠。窗口标题为 'mod.rs - OpenCowork - Visual Studio Code'。",
  "app": "Visual Studio Code",
  "has_issue": false,
  "issue_type": "",
  "issue_summary": "",
  "suggestion": "",
  "confidence": 0.95
}}

判定规则：
- 只有当截图中出现明确错误/失败/阻塞提示时，has_issue 才为 true
- issue_type 用 2-6 个词概括问题（如 编译错误/网络错误/权限不足/界面卡死）
- issue_summary 必须具体指出错误内容或提示文本，不要泛泛而谈
- detail 只描述可见信息，不要猜测未显示的内容

近期记录（仅供参考，可能不完整）：
{}
"#,
        recent_context
    );

    let analysis = match model_manager
        .analyze_image(&config.model, &image_base64, &prompt)
        .await
    {
        Ok(result) => result,
        Err(err) => {
            emit_model_error_once(
                recent_alerts,
                app_handle,
                &err,
                "capture",
                now,
                config.capture.alert_cooldown_seconds,
            );
            return Err(err);
        }
    };

    // 6. 解析分析结果
    let mut parsed = parse_analysis(&analysis);
    let alert_threshold = config.capture.alert_confidence_threshold.clamp(0.0, 1.0);
    let issue_message = if parsed.issue_message.is_empty() {
        parsed.summary.clone()
    } else {
        parsed.issue_message.clone()
    };
    let mut should_emit = false;
    let mut current_issue_key: Option<String> = None;

    if parsed.has_issue && parsed.confidence >= alert_threshold && !should_suppress_alert(&parsed) {
        let alert_key = build_alert_key(&parsed, &issue_message);
        current_issue_key = Some(alert_key.clone());

        let last_key = last_issue_key.lock().clone();
        if last_key.as_deref() != Some(alert_key.as_str()) {
            should_emit = should_emit_alert(
                recent_alerts,
                &alert_key,
                now,
                config.capture.alert_cooldown_seconds,
            );
        }

        if should_emit && parsed.suggestion.trim().is_empty() {
            match generate_issue_suggestion(&model_manager, &config, &recent_context, &parsed).await {
                Ok(suggestion) => parsed.suggestion = suggestion,
                Err(err) => {
                    eprintln!("生成建议失败: {}", err);
                    parsed.suggestion = "建议生成失败，请查看详情或稍后重试。".to_string();
                }
            }
        }
    }

    *last_issue_key.lock() = current_issue_key;

    // 7. 保存摘要
    let timestamp = now.format("%Y-%m-%dT%H:%M:%S").to_string();
    let issue_summary = issue_message.clone();

    let summary = SummaryRecord {
        timestamp: timestamp.clone(),
        summary: parsed.summary.clone(),
        app: parsed.app.clone(),
        action: if parsed.has_issue { "issue".to_string() } else { "active".to_string() },
        keywords: extract_keywords_from_analysis(&parsed.summary),
        has_issue: parsed.has_issue,
        issue_type: parsed.issue_type.clone(),
        issue_summary,
        suggestion: parsed.suggestion.clone(),
        confidence: parsed.confidence,
        detail: parsed.detail.clone(),
        detail_ref: screenshot_ref.unwrap_or_default(),
    };

    storage_manager.save_summary(&summary)?;

    // 8. 如果检测到困难，主动推送提示
    if parsed.has_issue && should_emit {
        let alert_message = AssistantAlert {
            timestamp: timestamp.clone(),
            issue_type: parsed.issue_type,
            message: issue_message,
            suggestion: parsed.suggestion,
        };

        let mut alert_log = String::new();
        alert_log.push_str(&format!("time: {}\n", timestamp));
        alert_log.push_str(&format!("issue_type: {}\n", alert_message.issue_type));
        alert_log.push_str(&format!("message: {}\n", alert_message.message));
        if !alert_message.suggestion.is_empty() {
            alert_log.push_str(&format!("suggestion: {}\n", alert_message.suggestion));
        }
        alert_log.push_str(&format!(
            "confidence: {:.2}\nthreshold: {:.2}\n",
            parsed.confidence, alert_threshold
        ));
        if let Err(err) = storage_manager.write_log_snapshot("assistant-alert", &alert_log) {
            eprintln!("写入提醒日志失败: {}", err);
        }

        if let Err(err) = app_handle.emit("assistant-alert", alert_message) {
            eprintln!("发送提醒失败: {}", err);
        }
    }

    Ok(true)  // 返回true表示已分析
}

#[derive(Clone, serde::Serialize)]
pub struct AssistantAlert {
    pub timestamp: String,
    pub issue_type: String,
    pub message: String,
    pub suggestion: String,
}

fn should_suppress_alert(parsed: &AnalysisResult) -> bool {
    let app = parsed.app.to_lowercase();
    let combined = format!(
        "{} {} {} {}",
        parsed.app,
        parsed.summary,
        parsed.detail,
        parsed.issue_message
    )
    .to_lowercase();

    let markers = ["历史", "对话", "聊天", "提醒", "警告", "设置"];
    let has_marker = markers.iter().any(|marker| combined.contains(marker));

    let self_markers = ["opencowork", "open cowork"];
    let is_self_app = self_markers.iter().any(|marker| app.contains(marker));
    let is_self_combined = self_markers.iter().any(|marker| combined.contains(marker));

    if is_self_app {
        return has_marker;
    }

    if (app.is_empty() || app == "unknown") && is_self_combined {
        return has_marker;
    }

    false
}

fn build_alert_key(parsed: &AnalysisResult, issue_message: &str) -> String {
    let issue_type = normalize_key(&parsed.issue_type);
    if !issue_type.is_empty() {
        return issue_type;
    }
    normalize_issue_text(issue_message)
}

fn normalize_key(text: &str) -> String {
    text.trim().to_lowercase()
}

fn normalize_issue_text(text: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;

    for ch in text.trim().chars() {
        if ch.is_ascii_digit() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
            continue;
        }

        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
            continue;
        }

        out.push(ch.to_ascii_lowercase());
        last_space = false;
    }

    out.trim().to_string()
}

fn should_emit_alert(
    recent_alerts: &Arc<ParkingMutex<HashMap<String, DateTime<Local>>>>,
    alert_key: &str,
    now: DateTime<Local>,
    cooldown_seconds: u64,
) -> bool {
    let cooldown = Duration::seconds(cooldown_seconds.max(5) as i64);
    let mut alerts = recent_alerts.lock();
    if let Some(prev) = alerts.get(alert_key) {
        if now.signed_duration_since(*prev) < cooldown {
            return false;
        }
    }
    alerts.insert(alert_key.to_string(), now);
    true
}

fn emit_model_error_once(
    recent_alerts: &Arc<ParkingMutex<HashMap<String, DateTime<Local>>>>,
    app_handle: &AppHandle,
    detail: &str,
    source: &str,
    now: DateTime<Local>,
    cooldown_seconds: u64,
) {
    let alert = build_model_error_alert(detail, source);
    let key = format!("model:{}:{}", &alert.error_type, &alert.message);
    if should_emit_alert(recent_alerts, &key, now, cooldown_seconds) {
        let _ = app_handle.emit("model-error", alert);
    }
}

#[derive(Default)]
struct AnalysisResult {
    summary: String,
    app: String,
    detail: String,
    has_issue: bool,
    issue_type: String,
    issue_message: String,
    suggestion: String,
    confidence: f32,
}

fn parse_analysis(analysis: &str) -> AnalysisResult {
    if let Some(json) = extract_json_value(analysis) {
        let mut has_issue = json
            .get("has_issue")
            .and_then(|v| v.as_bool())
            .or_else(|| json.get("has_error").and_then(|v| v.as_bool()))
            .unwrap_or(false);
        let issue_type = json
            .get("issue_type")
            .and_then(|v| v.as_str())
            .or_else(|| json.get("error_type").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        let issue_message = json
            .get("issue_summary")
            .and_then(|v| v.as_str())
            .or_else(|| json.get("error_message").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        let detail = json
            .get("detail")
            .or_else(|| json.get("detail_description"))
            .or_else(|| json.get("image_detail"))
            .or_else(|| json.get("image_description"))
            .or_else(|| json.get("screen_detail"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let suggestion = json.get("suggestion").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let confidence = parse_confidence(&json, has_issue);

        if !has_issue && (!issue_type.is_empty() || !issue_message.is_empty() || !suggestion.is_empty()) {
            has_issue = true;
        }

        return AnalysisResult {
            summary: json.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            app: json.get("app").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string(),
            detail,
            has_issue,
            issue_type,
            issue_message,
            suggestion,
            confidence,
        };
    }

    let has_issue = analysis.to_lowercase().contains("error")
        || analysis.contains("错误")
        || analysis.contains("失败")
        || analysis.contains("异常")
        || analysis.contains("无法")
        || analysis.contains("找不到")
        || analysis.contains("未找到")
        || analysis.contains("卡住")
        || analysis.contains("无响应");

    AnalysisResult {
        summary: analysis.lines().next().unwrap_or(analysis).to_string(),
        app: extract_app_from_text(analysis),
        detail: analysis.to_string(),
        has_issue,
        issue_type: if has_issue { "detected".to_string() } else { String::new() },
        issue_message: if has_issue { analysis.to_string() } else { String::new() },
        suggestion: String::new(),
        confidence: if has_issue { 0.4 } else { 0.2 },
    }
}

fn extract_json_value(text: &str) -> Option<serde_json::Value> {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
        return Some(json);
    }

    if let Some(inner) = extract_fenced_json(text) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&inner) {
            return Some(json);
        }
    }

    if let Some(inner) = extract_braced_json(text) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&inner) {
            return Some(json);
        }
    }

    None
}

fn extract_fenced_json(text: &str) -> Option<String> {
    if let Some(start) = text.find("```json") {
        let rest = &text[start + 7..];
        return extract_fence_body(rest);
    }

    if let Some(start) = text.find("```") {
        let rest = &text[start + 3..];
        return extract_fence_body(rest);
    }

    None
}

fn extract_fence_body(text: &str) -> Option<String> {
    let end = text.find("```")?;
    let mut body = text[..end].trim().to_string();
    if let Some(stripped) = body.strip_prefix("json") {
        body = stripped.trim_start().to_string();
    }
    Some(body)
}

fn extract_braced_json(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(text[start..=end].to_string())
}

async fn generate_issue_suggestion(
    model_manager: &ModelManager,
    config: &Config,
    recent_context: &str,
    parsed: &AnalysisResult,
) -> Result<String, String> {
    let issue_summary = if parsed.issue_message.is_empty() {
        parsed.summary.as_str()
    } else {
        parsed.issue_message.as_str()
    };
    let issue_type = if parsed.issue_type.is_empty() {
        "未分类"
    } else {
        parsed.issue_type.as_str()
    };

    let context = format!(
        "当前截图分析:\n- summary: {}\n- detail: {}\n- issue_type: {}\n- issue_summary: {}\n- confidence: {:.2}\n\n近期记录:\n{}",
        parsed.summary,
        parsed.detail,
        issue_type,
        issue_summary,
        parsed.confidence,
        recent_context
    );

    let question = "基于以上信息给出 1-3 条可执行的解决建议，尽量具体，不要复述背景。";

    model_manager.chat(&config.model, &context, question).await
}

fn extract_app_from_text(text: &str) -> String {
    let apps = [
        "Visual Studio Code", "VS Code", "Chrome", "Firefox", "Edge",
        "微信", "QQ", "钉钉", "飞书", "Slack", "Discord",
        "Word", "Excel", "PowerPoint", "Notion", "Obsidian",
        "Terminal", "PowerShell", "CMD",
    ];

    for app in apps {
        if text.contains(app) {
            return app.to_string();
        }
    }

    "Unknown".to_string()
}

fn extract_keywords_from_analysis(analysis: &str) -> Vec<String> {
    let mut keywords = Vec::new();

    let extensions = [".rs", ".ts", ".js", ".py", ".vue", ".tsx", ".jsx", ".md", ".json"];
    for ext in extensions {
        if analysis.contains(ext) {
            keywords.push(ext.to_string());
        }
    }

    let actions = [
        "编辑", "浏览", "搜索", "调试", "运行", "编写", "阅读", "聊天",
        "错误", "报错", "困难", "无法", "找不到", "未找到", "卡住", "无响应",
    ];
    for action in actions {
        if analysis.contains(action) {
            keywords.push(action.to_string());
        }
    }

    keywords
}

fn build_recent_summary_context(
    storage_manager: &StorageManager,
    max_items: usize,
    detail_limit: usize,
) -> String {
    let now = Local::now();
    let date = now.format("%Y-%m-%d").to_string();
    let cutoff = (now - Duration::minutes(RECENT_CONTEXT_MINUTES))
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();

    let records = match storage_manager.get_summaries(&date) {
        Ok(data) => data,
        Err(_) => return "（无）".to_string(),
    };

    let mut recent: Vec<_> = records
        .into_iter()
        .filter(|r| r.timestamp >= cutoff)
        .collect();

    if recent.is_empty() {
        return "（无）".to_string();
    }

    let max_items = max_items.clamp(1, 100);
    let detail_limit = detail_limit.min(max_items);
    recent.reverse();
    let mut recent = recent.into_iter().take(max_items).collect::<Vec<_>>();
    recent.reverse();

    let detail_start = recent.len().saturating_sub(detail_limit);

    recent
        .into_iter()
        .enumerate()
        .map(|(idx, record)| {
            let time = record.timestamp.get(11..19).unwrap_or(&record.timestamp);
            let app = if record.app.is_empty() || record.app == "Unknown" {
                String::new()
            } else {
                format!(" [{}]", record.app)
            };
            let mut line = format!("- {}{} {}", time, app, record.summary);
            if idx >= detail_start && !record.detail.is_empty() {
                let detail = record.detail.replace('\n', " ");
                line.push_str(&format!("\n  细节: {}", detail));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_confidence(json: &serde_json::Value, has_issue: bool) -> f32 {
    let fallback = if has_issue { 0.5 } else { 0.2 };
    let value = match json.get("confidence") {
        Some(serde_json::Value::Number(num)) => num.as_f64().unwrap_or(fallback as f64) as f32,
        Some(serde_json::Value::String(text)) => match text.to_lowercase().as_str() {
            "high" => 0.9,
            "medium" => 0.6,
            "low" => 0.3,
            _ => fallback,
        },
        _ => fallback,
    };

    value.clamp(0.0, 1.0)
}

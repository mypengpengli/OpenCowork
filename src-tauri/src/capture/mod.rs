mod screen;
mod scheduler;

pub use screen::*;
pub use scheduler::*;

use crate::model::{build_model_error_alert, ModelManager};
use crate::storage::{Config, StorageManager, SummaryRecord};
use chrono::{DateTime, Duration, Local};
use image::DynamicImage;
use parking_lot::Mutex as ParkingMutex;
use std::collections::HashSet;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

const RECENT_CONTEXT_MINUTES: i64 = 3;

pub struct CaptureManager {
    is_running: Arc<ParkingMutex<bool>>,
    record_count: Arc<ParkingMutex<u64>>,
    skip_count: Arc<ParkingMutex<u64>>,  // 跳过的帧数
    stop_tx: Option<mpsc::Sender<()>>,
    recent_alerts: Arc<ParkingMutex<HashSet<String>>>,
}

impl CaptureManager {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(ParkingMutex::new(false)),
            record_count: Arc::new(ParkingMutex::new(0)),
            skip_count: Arc::new(ParkingMutex::new(0)),
            stop_tx: None,
            recent_alerts: Arc::new(ParkingMutex::new(HashSet::new())),
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
            let mut cleanup_counter = 0u32;

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

                        cleanup_counter += 1;
                        if cleanup_counter >= 60 {
                            recent_alerts.lock().clear();
                            cleanup_counter = 0;
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
    recent_alerts: &Arc<ParkingMutex<HashSet<String>>>,
    app_handle: &AppHandle,
    prev_hash: &mut Option<u64>,
) -> Result<bool, String> {
    // 1. 截屏
    let image = ScreenCapture::capture_primary()?;
    let now = Local::now();
    let screenshot_ref = save_screenshot(storage_manager, &image, &now, config.capture.compress_quality);

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

    // 3. 转换为 base64
    let image_base64 = ScreenCapture::image_to_base64(&image, config.capture.compress_quality)?;

    // 4. 发送给大模型识别
    let recent_context = build_recent_summary_context(
        storage_manager,
        config.capture.recent_summary_limit,
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
  "suggestion": "解决建议（仅在 has_issue 为 true 时填写，否则空字符串）",
  "confidence": 对整体分析结果准确性的置信度，0.0-1.0 之间的数值
}}

示例输出：
{{
  "summary": "在 VS Code 中编辑 screen-assistant 项目的 Rust 后端代码，正在修改 capture 模块的截图分析提示词",
  "detail": "VS Code 编辑器窗口最大化显示。左侧资源管理器展开 src-tauri/src/capture 目录，当前打开文件为 mod.rs。编辑区域显示第 215-260 行的 Rust 代码，包含 format! 宏和 JSON 字符串。光标位于第 238 行。右上角显示 Git 分支为 master。底部状态栏显示 UTF-8 编码、LF 换行符、Rust 语言模式。底部终端面板已折叠。窗口标题为 'mod.rs - screen-assistant - Visual Studio Code'。",
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
            emit_model_error_once(recent_alerts, app_handle, &err, "capture");
            return Err(err);
        }
    };

    // 5. 解析分析结果
    let parsed = parse_analysis(&analysis);

    // 6. 保存摘要
    let timestamp = now.format("%Y-%m-%dT%H:%M:%S").to_string();

    let summary = SummaryRecord {
        timestamp: timestamp.clone(),
        summary: parsed.summary.clone(),
        app: parsed.app.clone(),
        action: if parsed.has_issue { "issue".to_string() } else { "active".to_string() },
        keywords: extract_keywords_from_analysis(&parsed.summary),
        confidence: parsed.confidence,
        detail: parsed.detail.clone(),
        detail_ref: screenshot_ref.unwrap_or_default(),
    };

    storage_manager.save_summary(&summary)?;

    // 7. 如果检测到困难，主动推送提示
    if parsed.has_issue {
        let error_key = format!("{}:{}", parsed.issue_type, parsed.issue_message);

        let should_alert = {
            let mut alerts = recent_alerts.lock();
            if alerts.contains(&error_key) {
                false
            } else {
                alerts.insert(error_key);
                true
            }
        };

        if should_alert {
            let alert_message = AssistantAlert {
                timestamp: timestamp.clone(),
                issue_type: parsed.issue_type,
                message: parsed.issue_message,
                suggestion: parsed.suggestion,
            };

            let _ = app_handle.emit("assistant-alert", alert_message);
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

fn emit_model_error_once(
    recent_alerts: &Arc<ParkingMutex<HashSet<String>>>,
    app_handle: &AppHandle,
    detail: &str,
    source: &str,
) {
    let alert = build_model_error_alert(detail, source);
    let key = format!("model:{}:{}", &alert.error_type, &alert.message);

    let should_emit = {
        let mut alerts = recent_alerts.lock();
        if alerts.contains(&key) {
            false
        } else {
            alerts.insert(key);
            true
        }
    };

    if should_emit {
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
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(analysis) {
        let has_issue = json
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
        let confidence = parse_confidence(&json, has_issue);

        return AnalysisResult {
            summary: json.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            app: json.get("app").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string(),
            detail,
            has_issue,
            issue_type,
            issue_message,
            suggestion: json.get("suggestion").and_then(|v| v.as_str()).unwrap_or("").to_string(),
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

fn build_recent_summary_context(storage_manager: &StorageManager, max_items: usize) -> String {
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
    recent.reverse();
    let mut recent = recent.into_iter().take(max_items).collect::<Vec<_>>();
    recent.reverse();

    recent
        .into_iter()
        .map(|record| {
            let time = record.timestamp.get(11..19).unwrap_or(&record.timestamp);
            let app = if record.app.is_empty() || record.app == "Unknown" {
                String::new()
            } else {
                format!(" [{}]", record.app)
            };
            format!("- {}{} {}", time, app, record.summary)
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

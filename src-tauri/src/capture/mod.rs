mod screen;
mod scheduler;

pub use screen::*;
pub use scheduler::*;

use crate::model::ModelManager;
use crate::storage::{Config, StorageManager, SummaryRecord};
use chrono::Local;
use image::DynamicImage;
use parking_lot::Mutex as ParkingMutex;
use std::collections::HashSet;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

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
    let prompt = r#"请分析这个屏幕截图，返回JSON格式：
{
  "summary": "一句话描述当前操作",
  "app": "应用程序名称",
  "has_error": true/false,
  "error_type": "错误类型（如果有）",
  "error_message": "错误信息摘要（如果有）",
  "suggestion": "解决建议（如果有错误）"
}

重点检测：
- 编程错误（编译错误、运行时错误、语法错误）
- 命令行报错
- 弹窗错误提示
- 网页错误页面

如果没有错误，has_error设为false，error相关字段留空。"#;

    let analysis = model_manager
        .analyze_image(&config.model, &image_base64, prompt)
        .await?;

    // 5. 解析分析结果
    let parsed = parse_analysis(&analysis);

    // 6. 保存摘要
    let timestamp = Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();

    let summary = SummaryRecord {
        timestamp: timestamp.clone(),
        summary: parsed.summary.clone(),
        app: parsed.app.clone(),
        action: if parsed.has_error { "error".to_string() } else { "active".to_string() },
        keywords: extract_keywords_from_analysis(&parsed.summary),
        detail_ref: String::new(),
    };

    storage_manager.save_summary(&summary)?;

    // 7. 如果检测到错误，主动推送提示
    if parsed.has_error {
        let error_key = format!("{}:{}", parsed.error_type, parsed.error_message);

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
                error_type: parsed.error_type,
                message: parsed.error_message,
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
    pub error_type: String,
    pub message: String,
    pub suggestion: String,
}

#[derive(Default)]
struct AnalysisResult {
    summary: String,
    app: String,
    has_error: bool,
    error_type: String,
    error_message: String,
    suggestion: String,
}

fn parse_analysis(analysis: &str) -> AnalysisResult {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(analysis) {
        return AnalysisResult {
            summary: json.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            app: json.get("app").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string(),
            has_error: json.get("has_error").and_then(|v| v.as_bool()).unwrap_or(false),
            error_type: json.get("error_type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            error_message: json.get("error_message").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            suggestion: json.get("suggestion").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        };
    }

    let has_error = analysis.to_lowercase().contains("error")
        || analysis.contains("错误")
        || analysis.contains("失败")
        || analysis.contains("异常");

    AnalysisResult {
        summary: analysis.lines().next().unwrap_or(analysis).to_string(),
        app: extract_app_from_text(analysis),
        has_error,
        error_type: if has_error { "detected".to_string() } else { String::new() },
        error_message: if has_error { analysis.to_string() } else { String::new() },
        suggestion: String::new(),
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

    let actions = ["编辑", "浏览", "搜索", "调试", "运行", "编写", "阅读", "聊天", "错误", "报错"];
    for action in actions {
        if analysis.contains(action) {
            keywords.push(action.to_string());
        }
    }

    keywords
}

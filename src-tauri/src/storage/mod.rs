use chrono::{DateTime, Local, Duration, Timelike};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::collections::HashMap;

// ============ 配置结构 ============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub model: ModelConfig,
    pub capture: CaptureConfig,
    pub storage: StorageConfig,
    #[serde(default)]
    pub global_prompt: GlobalPromptConfig,
}

// ============ 全局提示词配置 ============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalPromptItem {
    pub id: String,           // 唯一标识 (UUID)
    pub name: String,         // 提示词名称（如"个人信息"、"公司信息"）
    pub content: String,      // 提示词内容
    pub enabled: bool,        // 是否启用
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalPromptConfig {
    #[serde(default)]
    pub items: Vec<GlobalPromptItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub api: ApiConfig,
    pub ollama: OllamaConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    #[serde(rename = "type")]
    pub api_type: String,
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub endpoint: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    pub enabled: bool,
    pub interval_ms: u64,
    pub compress_quality: u8,
    #[serde(default = "default_skip_unchanged")]
    pub skip_unchanged: bool,  // 跳过无变化的画面，节省token
    #[serde(default = "default_change_threshold")]
    pub change_threshold: f32,  // 变化阈值 (0.0-1.0)，越小越敏感
    #[serde(default = "default_recent_summary_limit")]
    pub recent_summary_limit: usize,  // 近期摘要条数（用于上下文参考）
    #[serde(default = "default_recent_detail_limit")]
    pub recent_detail_limit: usize,  // 近期 detail 条数（用于截图分析上下文）
    #[serde(default = "default_alert_confidence_threshold")]
    pub alert_confidence_threshold: f32,  // issue 提醒触发阈值
    #[serde(default = "default_alert_cooldown_seconds")]
    pub alert_cooldown_seconds: u64,  // issue 提醒冷却时间（秒）
}

fn default_skip_unchanged() -> bool {
    true  // 默认启用，节省token
}

fn default_change_threshold() -> f32 {
    0.95  // 相似度超过95%认为无变化
}

fn default_recent_summary_limit() -> usize {
    8
}

fn default_recent_detail_limit() -> usize {
    3
}

fn default_alert_confidence_threshold() -> f32 {
    0.7
}

fn default_alert_cooldown_seconds() -> u64 {
    120
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub retention_days: u32,
    pub max_screenshots: u32,
    #[serde(default = "default_max_context_chars")]
    pub max_context_chars: usize,  // 上下文最大字符数，用户可调整
    #[serde(default)]
    pub auto_clear_on_start: bool,  // 启动时自动清空历史
}

fn default_max_context_chars() -> usize {
    10000  // 默认10000字符
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: ModelConfig {
                provider: "api".to_string(),
                api: ApiConfig {
                    api_type: "openai".to_string(),
                    endpoint: "https://api.openai.com/v1".to_string(),
                    api_key: String::new(),
                    model: "gpt-4-vision-preview".to_string(),
                },
                ollama: OllamaConfig {
                    endpoint: "http://localhost:11434".to_string(),
                    model: "llava".to_string(),
                },
            },
            capture: CaptureConfig {
                enabled: true,
                interval_ms: 1000,
                compress_quality: 80,
                skip_unchanged: true,   // 默认启用，节省token
                change_threshold: 0.95, // 相似度阈值
                recent_summary_limit: 8,
                recent_detail_limit: 3,
                alert_confidence_threshold: 0.7,
                alert_cooldown_seconds: 120,
            },
            storage: StorageConfig {
                retention_days: 7,
                max_screenshots: 10000,
                max_context_chars: 10000,  // 默认10000字符
                auto_clear_on_start: false,
            },
            global_prompt: GlobalPromptConfig::default(),
        }
    }
}

// ============ 分层记录结构 ============

/// 原始记录（每秒级别）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryRecord {
    pub timestamp: String,
    pub summary: String,
    pub app: String,
    pub action: String,
    pub keywords: Vec<String>,
    #[serde(default)]
    pub has_issue: bool,
    #[serde(default)]
    pub issue_type: String,
    #[serde(default)]
    pub issue_summary: String,
    #[serde(default)]
    pub suggestion: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub detail: String,
    #[serde(default)]
    pub detail_ref: String,
}

/// 聚合记录（5分钟级别）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedRecord {
    pub start_time: String,
    pub end_time: String,
    pub summary: String,           // 这5分钟的概要
    pub apps: Vec<String>,         // 使用的应用列表
    pub main_activities: Vec<String>, // 主要活动
    pub keywords: Vec<String>,     // 关键词
    pub record_count: u32,         // 原始记录数量
    pub has_errors: bool,          // 是否有错误
    pub error_summary: Option<String>, // 错误概要
}

/// 日摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySummary {
    pub date: String,
    pub records: Vec<SummaryRecord>,
    #[serde(default)]
    pub aggregated: Vec<AggregatedRecord>,
    #[serde(default)]
    pub day_summary: Option<String>, // 当天总结
}

// ============ 存储管理器 ============

pub struct StorageManager {
    data_dir: PathBuf,
}

impl StorageManager {
    pub fn new() -> Self {
        let base_dir = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
        let mut data_dir = base_dir.join("opencowork").join("data");
        if !data_dir.exists() {
            let legacy_dir = base_dir.join("screen-assistant").join("data");
            if legacy_dir.exists() {
                data_dir = legacy_dir;
            }
        }

        Self { data_dir }
    }

    /// 获取数据目录路径
    pub fn get_data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    fn ensure_dirs(&self) -> Result<(), String> {
        let dirs = [
            self.data_dir.clone(),
            self.data_dir.join("summaries"),
            self.data_dir.join("aggregated"),
            self.data_dir.join("profiles"),
            self.data_dir.join("screenshots"),
            self.data_dir.join("logs"),
        ];

        for dir in dirs {
            fs::create_dir_all(&dir)
                .map_err(|e| format!("创建目录失败 {:?}: {}", dir, e))?;
        }

        Ok(())
    }

    pub fn screenshots_dir(&self) -> Result<PathBuf, String> {
        self.ensure_dirs()?;
        Ok(self.data_dir.join("screenshots"))
    }

    pub fn logs_dir(&self) -> Result<PathBuf, String> {
        self.ensure_dirs()?;
        Ok(self.data_dir.join("logs"))
    }

    pub fn write_log_snapshot(&self, prefix: &str, content: &str) -> Result<PathBuf, String> {
        let dir = self.logs_dir()?;
        let now = Local::now();
        let filename = format!(
            "{}-{:03}-{}.log",
            now.format("%Y%m%d-%H%M%S"),
            now.timestamp_subsec_millis(),
            sanitize_log_prefix(prefix),
        );
        let path = dir.join(filename);
        fs::write(&path, content).map_err(|e| format!("写入日志失败 {:?}: {}", path, e))?;
        Ok(path)
    }

    // ============ 配置管理 ============

    pub fn load_config(&self) -> Result<Config, String> {
        self.ensure_dirs()?;
        let config_path = self.data_dir.join("config.json");

        if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .map_err(|e| format!("读取配置失败: {}", e))?;
            serde_json::from_str(&content)
                .map_err(|e| format!("解析配置失败: {}", e))
        } else {
            Ok(Config::default())
        }
    }

    pub fn save_config(&self, config: &Config) -> Result<(), String> {
        self.ensure_dirs()?;
        let config_path = self.data_dir.join("config.json");
        let content = serde_json::to_string_pretty(config)
            .map_err(|e| format!("序列化配置失败: {}", e))?;
        fs::write(&config_path, content)
            .map_err(|e| format!("保存配置失败: {}", e))
    }

    // ============ 配置方案管理 ============

    pub fn list_profiles(&self) -> Result<Vec<String>, String> {
        self.ensure_dirs()?;
        let profiles_dir = self.data_dir.join("profiles");

        let mut profiles = Vec::new();
        let entries = fs::read_dir(&profiles_dir)
            .map_err(|e| format!("读取配置方案失败: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("读取配置方案失败: {}", e))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                profiles.push(name.to_string());
            }
        }

        profiles.sort();
        Ok(profiles)
    }

    pub fn save_profile(&self, name: &str, config: &Config) -> Result<(), String> {
        self.ensure_dirs()?;
        let safe_name = sanitize_profile_name(name)?;
        let path = self.profile_path(&safe_name)?;
        let content = serde_json::to_string_pretty(config)
            .map_err(|e| format!("序列化配置方案失败: {}", e))?;
        fs::write(&path, content)
            .map_err(|e| format!("保存配置方案失败: {}", e))
    }

    pub fn load_profile(&self, name: &str) -> Result<Config, String> {
        self.ensure_dirs()?;
        let safe_name = sanitize_profile_name(name)?;
        let path = self.profile_path(&safe_name)?;
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("读取配置方案失败: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("解析配置方案失败: {}", e))
    }

    pub fn delete_profile(&self, name: &str) -> Result<(), String> {
        self.ensure_dirs()?;
        let safe_name = sanitize_profile_name(name)?;
        let path = self.profile_path(&safe_name)?;
        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("删除配置方案失败: {}", e))?;
        }
        Ok(())
    }

    fn profile_path(&self, name: &str) -> Result<PathBuf, String> {
        if name.is_empty() {
            return Err("配置名不能为空".to_string());
        }
        Ok(self.data_dir.join("profiles").join(format!("{}.json", name)))
    }

    // ============ 原始记录管理 ============

    pub fn get_summaries(&self, date: &str) -> Result<Vec<SummaryRecord>, String> {
        let summary_path = self.data_dir.join("summaries").join(format!("{}.json", date));

        if !summary_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&summary_path)
            .map_err(|e| format!("读取摘要失败: {}", e))?;

        let daily: DailySummary = serde_json::from_str(&content)
            .map_err(|e| format!("解析摘要失败: {}", e))?;

        Ok(daily.records)
    }

    pub fn get_recent_records(&self, limit: usize, days: u32) -> Vec<SummaryRecord> {
        if limit == 0 {
            return Vec::new();
        }

        let max_days = days.max(1);
        let mut recent_rev = Vec::new();

        for offset in 0..max_days {
            let date = (Local::now() - Duration::days(offset as i64))
                .format("%Y-%m-%d")
                .to_string();
            let records = match self.get_summaries(&date) {
                Ok(data) => data,
                Err(_) => continue,
            };

            for record in records.into_iter().rev() {
                recent_rev.push(record);
                if recent_rev.len() >= limit {
                    break;
                }
            }

            if recent_rev.len() >= limit {
                break;
            }
        }

        recent_rev.reverse();
        recent_rev
    }

    pub fn save_summary(&self, record: &SummaryRecord) -> Result<(), String> {
        self.ensure_dirs()?;

        let date = &record.timestamp[..10];
        let summary_path = self.data_dir.join("summaries").join(format!("{}.json", date));

        let mut daily = if summary_path.exists() {
            let content = fs::read_to_string(&summary_path)
                .map_err(|e| format!("读取摘要失败: {}", e))?;
            serde_json::from_str(&content).unwrap_or(DailySummary {
                date: date.to_string(),
                records: Vec::new(),
                aggregated: Vec::new(),
                day_summary: None,
            })
        } else {
            DailySummary {
                date: date.to_string(),
                records: Vec::new(),
                aggregated: Vec::new(),
                day_summary: None,
            }
        };

        daily.records.push(record.clone());

        // 检查是否需要聚合（每300条触发一次，约5分钟）
        if daily.records.len() % 300 == 0 {
            self.trigger_aggregation(&mut daily)?;
        }

        let content = serde_json::to_string_pretty(&daily)
            .map_err(|e| format!("序列化摘要失败: {}", e))?;

        fs::write(&summary_path, content)
            .map_err(|e| format!("保存摘要失败: {}", e))
    }

    pub fn delete_summaries_for_date(&self, date: &str) -> Result<usize, String> {
        self.ensure_dirs()?;
        let summary_path = self.data_dir.join("summaries").join(format!("{}.json", date));
        if !summary_path.exists() {
            return Ok(0);
        }

        let content = fs::read_to_string(&summary_path)
            .map_err(|e| format!("读取摘要失败: {}", e))?;
        let daily: DailySummary = serde_json::from_str(&content)
            .map_err(|e| format!("解析摘要失败: {}", e))?;

        let mut removed = 0usize;
        for record in daily.records {
            removed += 1;
            if !record.detail_ref.is_empty() {
                if let Ok(dir) = self.screenshots_dir() {
                    let path = dir.join(&record.detail_ref);
                    let _ = fs::remove_file(&path);
                }
            }
        }

        fs::remove_file(&summary_path)
            .map_err(|e| format!("删除摘要失败: {}", e))?;

        Ok(removed)
    }

    pub fn delete_all_summaries(&self) -> Result<usize, String> {
        self.ensure_dirs()?;
        let summaries_dir = self.data_dir.join("summaries");
        if !summaries_dir.exists() {
            return Ok(0);
        }

        let mut total_removed = 0usize;
        let entries = fs::read_dir(&summaries_dir)
            .map_err(|e| format!("读取摘要目录失败: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("读取摘要目录失败: {}", e))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }

            let content = match fs::read_to_string(&path) {
                Ok(value) => value,
                Err(_) => {
                    let _ = fs::remove_file(&path);
                    continue;
                }
            };

            if let Ok(daily) = serde_json::from_str::<DailySummary>(&content) {
                for record in daily.records {
                    total_removed += 1;
                    if !record.detail_ref.is_empty() {
                        if let Ok(dir) = self.screenshots_dir() {
                            let shot = dir.join(&record.detail_ref);
                            let _ = fs::remove_file(&shot);
                        }
                    }
                }
            }

            let _ = fs::remove_file(&path);
        }

        Ok(total_removed)
    }

    // ============ 聚合管理 ============

    fn trigger_aggregation(&self, daily: &mut DailySummary) -> Result<(), String> {
        // 获取最后300条记录进行聚合
        let records_to_aggregate: Vec<_> = daily.records.iter()
            .rev()
            .take(300)
            .cloned()
            .collect();

        if records_to_aggregate.is_empty() {
            return Ok(());
        }

        let aggregated = self.aggregate_records(&records_to_aggregate);
        daily.aggregated.push(aggregated);

        Ok(())
    }

    fn aggregate_records(&self, records: &[SummaryRecord]) -> AggregatedRecord {
        let start_time = records.last().map(|r| r.timestamp.clone()).unwrap_or_default();
        let end_time = records.first().map(|r| r.timestamp.clone()).unwrap_or_default();

        // 统计应用使用
        let mut app_counts: HashMap<String, u32> = HashMap::new();
        let mut all_keywords: HashMap<String, u32> = HashMap::new();
        let mut activities: Vec<String> = Vec::new();
        let mut has_errors = false;
        let mut error_messages: Vec<String> = Vec::new();

        for record in records {
            *app_counts.entry(record.app.clone()).or_insert(0) += 1;

            for kw in &record.keywords {
                *all_keywords.entry(kw.clone()).or_insert(0) += 1;
            }

            if record.action == "error" || record.action == "issue" {
                has_errors = true;
                error_messages.push(record.summary.clone());
            }

            // 提取主要活动（去重）
            if !activities.contains(&record.summary) && activities.len() < 5 {
                activities.push(record.summary.clone());
            }
        }

        // 排序获取最常用的应用
        let mut apps: Vec<_> = app_counts.into_iter().collect();
        apps.sort_by(|a, b| b.1.cmp(&a.1));
        let top_apps: Vec<String> = apps.into_iter().take(3).map(|(k, _)| k).collect();

        // 排序获取最常见的关键词
        let mut keywords: Vec<_> = all_keywords.into_iter().collect();
        keywords.sort_by(|a, b| b.1.cmp(&a.1));
        let top_keywords: Vec<String> = keywords.into_iter().take(10).map(|(k, _)| k).collect();

        // 生成概要
        let summary = format!(
            "使用 {} 进行了 {} 等操作",
            top_apps.join("、"),
            activities.first().unwrap_or(&"未知".to_string())
        );

        AggregatedRecord {
            start_time,
            end_time,
            summary,
            apps: top_apps,
            main_activities: activities,
            keywords: top_keywords,
            record_count: records.len() as u32,
            has_errors,
            error_summary: if has_errors {
                Some(error_messages.join("; "))
            } else {
                None
            },
        }
    }

    // ============ 智能检索 ============

    /// 根据时间范围和关键词智能检索记录
    pub fn smart_search(&self, query: &SearchQuery) -> Result<SearchResult, String> {
        let today = Local::now().format("%Y-%m-%d").to_string();

        match query.time_range {
            TimeRange::Recent(minutes) => {
                // 最近N分钟：使用原始记录
                let records = self.get_summaries(&today)?;
                let cutoff = Local::now() - Duration::minutes(minutes as i64);
                let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%S").to_string();

                let filtered: Vec<_> = records.into_iter()
                    .filter(|r| r.timestamp >= cutoff_str)
                    .filter(|r| query.matches_keywords(r))
                    .collect();

                Ok(SearchResult {
                    records: filtered,
                    aggregated: Vec::new(),
                    source: "原始记录".to_string(),
                })
            }
            TimeRange::Today => {
                // 今天：优先使用聚合记录
                let daily = self.load_daily(&today)?;

                if !query.keywords.is_empty() {
                    // 有关键词：搜索原始记录
                    let filtered: Vec<_> = daily.records.into_iter()
                        .filter(|r| query.matches_keywords(r))
                        .collect();
                    Ok(SearchResult {
                        records: filtered,
                        aggregated: Vec::new(),
                        source: "关键词搜索".to_string(),
                    })
                } else {
                    // 无关键词：返回聚合记录 + 最近的原始记录
                    let recent: Vec<_> = daily.records.into_iter().rev().take(20).collect();
                    Ok(SearchResult {
                        records: recent,
                        aggregated: daily.aggregated,
                        source: "聚合记录".to_string(),
                    })
                }
            }
            TimeRange::Days(days) => {
                // 多天：只使用聚合记录
                let mut all_aggregated = Vec::new();

                for i in 0..days {
                    let date = (Local::now() - Duration::days(i as i64))
                        .format("%Y-%m-%d").to_string();
                    if let Ok(daily) = self.load_daily(&date) {
                        all_aggregated.extend(daily.aggregated);
                    }
                }

                Ok(SearchResult {
                    records: Vec::new(),
                    aggregated: all_aggregated,
                    source: "历史聚合".to_string(),
                })
            }
        }
    }

    fn load_daily(&self, date: &str) -> Result<DailySummary, String> {
        let path = self.data_dir.join("summaries").join(format!("{}.json", date));

        if !path.exists() {
            return Ok(DailySummary {
                date: date.to_string(),
                records: Vec::new(),
                aggregated: Vec::new(),
                day_summary: None,
            });
        }

        let content = fs::read_to_string(&path)
            .map_err(|e| format!("读取失败: {}", e))?;

        serde_json::from_str(&content)
            .map_err(|e| format!("解析失败: {}", e))
    }
}

fn sanitize_profile_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    let base = trimmed.strip_suffix(".json").unwrap_or(trimmed).trim();

    if base.is_empty() {
        return Err("配置名不能为空".to_string());
    }
    if base.len() > 64 {
        return Err("配置名过长".to_string());
    }
    if base == "." || base == ".." {
        return Err("配置名不可用".to_string());
    }
    if base.ends_with(' ') || base.ends_with('.') {
        return Err("配置名不能以空格或句点结尾".to_string());
    }

    let invalid_chars = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
    if base.chars().any(|c| c.is_control() || invalid_chars.contains(&c)) {
        return Err("配置名包含非法字符".to_string());
    }

    let upper = base.to_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL",
        "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved.contains(&upper.as_str()) {
        return Err("配置名不可用".to_string());
    }

    Ok(base.to_string())
}

fn sanitize_log_prefix(prefix: &str) -> String {
    let mut clean = String::new();
    for ch in prefix.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            clean.push(ch);
        }
    }
    if clean.is_empty() {
        "log".to_string()
    } else {
        clean
    }
}

// ============ 搜索相关结构 ============

#[derive(Debug, Clone)]
pub enum TimeRange {
    Recent(u32),  // 最近N分钟
    Today,        // 今天
    Days(u32),    // 最近N天
}

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub time_range: TimeRange,
    pub keywords: Vec<String>,
    pub include_detail: bool,
}

impl SearchQuery {
    pub fn matches_keywords(&self, record: &SummaryRecord) -> bool {
        if self.keywords.is_empty() {
            return true;
        }

        let text = format!("{} {} {}",
            record.summary,
            record.app,
            format!("{} {}", record.detail, record.keywords.join(" "))
        ).to_lowercase();

        self.keywords.iter().any(|kw| text.contains(&kw.to_lowercase()))
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub records: Vec<SummaryRecord>,
    pub aggregated: Vec<AggregatedRecord>,
    pub source: String,
}

impl Default for SearchResult {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            aggregated: Vec::new(),
            source: String::new(),
        }
    }
}

impl SearchResult {
    /// 构建上下文字符串，控制在指定token数内
    pub fn build_context(&self, max_chars: usize, include_detail: bool) -> String {
        let mut context = String::new();
        let mut current_len = 0;

        // 先添加聚合记录（概要）
        if !self.aggregated.is_empty() {
            context.push_str("## 操作概要\n\n");
            for agg in &self.aggregated {
                let line = format!(
                    "- [{} ~ {}] {}\n",
                    &agg.start_time[11..16],
                    &agg.end_time[11..16],
                    agg.summary
                );
                if current_len + line.len() > max_chars {
                    break;
                }
                context.push_str(&line);
                current_len += line.len();

                // 如果有错误，添加错误信息
                if let Some(ref err) = agg.error_summary {
                    let err_line = format!("  ⚠️ 错误: {}\n", err);
                    if current_len + err_line.len() <= max_chars {
                        context.push_str(&err_line);
                        current_len += err_line.len();
                    }
                }
            }
            context.push('\n');
        }

        // 再添加详细记录
        if !self.records.is_empty() {
            context.push_str("## 详细记录\n\n");
            let mut entries: Vec<String> = Vec::new();
            let mut truncated = false;

            for record in self.records.iter().rev() {
                let line = format!(
                    "- [{}] {}\n",
                    &record.timestamp[11..19],
                    record.summary
                );
                if current_len + line.len() > max_chars {
                    truncated = true;
                    break;
                }

                let mut entry = String::new();
                entry.push_str(&line);
                current_len += line.len();

                if include_detail && !record.detail.is_empty() {
                    let detail_text = record.detail.replace('\n', " ");
                    let detail_line = format!("  细节: {}\n", detail_text);
                    if current_len + detail_line.len() > max_chars {
                        entry.push_str("  ...(细节已省略)\n");
                        current_len += "  ...(细节已省略)\n".len();
                        truncated = true;
                        entries.push(entry);
                        break;
                    }
                    entry.push_str(&detail_line);
                    current_len += detail_line.len();
                }

                entries.push(entry);
            }

            entries.reverse();
            for entry in entries {
                context.push_str(&entry);
            }
            if truncated {
                context.push_str("...(更多记录已省略)\n");
            }
        }

        if context.is_empty() {
            context = "目前没有相关的操作记录。".to_string();
        }

        context
    }
}

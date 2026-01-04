use crate::storage::{SummaryRecord, StorageManager};

pub struct ContextBuilder {
    storage: StorageManager,
}

impl ContextBuilder {
    pub fn new() -> Self {
        Self {
            storage: StorageManager::new(),
        }
    }

    /// 构建对话上下文
    pub fn build_context(&self, max_records: usize) -> String {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let summaries = self.storage.get_summaries(&today).unwrap_or_default();

        if summaries.is_empty() {
            return String::from("目前没有记录的屏幕操作历史。");
        }

        let mut context = String::from("以下是用户最近的屏幕操作记录：\n\n");

        for record in summaries.iter().rev().take(max_records) {
            context.push_str(&format!(
                "- [{}] {} (应用: {}, 操作: {})\n",
                &record.timestamp[11..19],
                record.summary,
                record.app,
                record.action
            ));
        }

        context
    }

    /// 根据关键词搜索相关记录
    pub fn search_by_keywords(&self, keywords: &[String], days: u32) -> Vec<SummaryRecord> {
        let mut results = Vec::new();

        for i in 0..days {
            let date = chrono::Local::now()
                .checked_sub_signed(chrono::Duration::days(i as i64))
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default();

            if let Ok(summaries) = self.storage.get_summaries(&date) {
                for record in summaries {
                    let matches = keywords.iter().any(|kw| {
                        record.summary.to_lowercase().contains(&kw.to_lowercase())
                            || record.keywords.iter().any(|k| k.to_lowercase().contains(&kw.to_lowercase()))
                    });

                    if matches {
                        results.push(record);
                    }
                }
            }
        }

        results
    }

    /// 获取特定时间范围的记录
    pub fn get_records_in_range(&self, start: &str, end: &str) -> Vec<SummaryRecord> {
        let mut results = Vec::new();

        // 简化实现：只获取当天的记录并过滤
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        if let Ok(summaries) = self.storage.get_summaries(&today) {
            for record in summaries {
                let time = &record.timestamp[11..19];
                if time >= start && time <= end {
                    results.push(record);
                }
            }
        }

        results
    }
}

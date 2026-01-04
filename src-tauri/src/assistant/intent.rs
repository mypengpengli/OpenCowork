/// 用户意图识别
pub struct IntentRecognizer;

#[derive(Debug, Clone)]
pub enum UserIntent {
    /// 查询最近操作
    QueryRecent { count: Option<usize> },
    /// 查询特定时间范围
    QueryTimeRange { start: String, end: String },
    /// 搜索特定内容
    Search { keywords: Vec<String> },
    /// 请求帮助/指导
    RequestHelp { topic: String },
    /// 一般对话
    General,
}

impl IntentRecognizer {
    /// 从用户消息中识别意图
    pub fn recognize(message: &str) -> UserIntent {
        let msg_lower = message.to_lowercase();

        // 查询最近操作
        if msg_lower.contains("刚才") || msg_lower.contains("刚刚") || msg_lower.contains("最近") {
            let count = Self::extract_count(&msg_lower);
            return UserIntent::QueryRecent { count };
        }

        // 时间范围查询
        if msg_lower.contains("从") && msg_lower.contains("到") {
            if let Some((start, end)) = Self::extract_time_range(&msg_lower) {
                return UserIntent::QueryTimeRange { start, end };
            }
        }

        // 过去 N 分钟
        if msg_lower.contains("过去") && msg_lower.contains("分钟") {
            let count = Self::extract_count(&msg_lower).unwrap_or(10);
            return UserIntent::QueryRecent { count: Some(count * 60) }; // 转换为秒数
        }

        // 搜索特定内容
        if msg_lower.contains("搜索") || msg_lower.contains("查找") || msg_lower.contains("找") {
            let keywords = Self::extract_keywords(message);
            if !keywords.is_empty() {
                return UserIntent::Search { keywords };
            }
        }

        // 请求帮助
        if msg_lower.contains("怎么") || msg_lower.contains("如何") || msg_lower.contains("帮我") {
            return UserIntent::RequestHelp {
                topic: message.to_string(),
            };
        }

        UserIntent::General
    }

    fn extract_count(text: &str) -> Option<usize> {
        // 提取数字
        let numbers: Vec<usize> = text
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .ok()
            .into_iter()
            .collect();

        numbers.first().copied()
    }

    fn extract_time_range(text: &str) -> Option<(String, String)> {
        // 简单的时间提取，格式如 "从10:00到11:00"
        let time_pattern = regex::Regex::new(r"(\d{1,2}:\d{2})").ok()?;
        let times: Vec<String> = time_pattern
            .find_iter(text)
            .map(|m| m.as_str().to_string())
            .collect();

        if times.len() >= 2 {
            Some((times[0].clone(), times[1].clone()))
        } else {
            None
        }
    }

    fn extract_keywords(text: &str) -> Vec<String> {
        // 提取引号中的内容或关键词
        let mut keywords = Vec::new();

        // 提取引号中的内容
        let quote_pattern = regex::Regex::new(r#"["""]([^"""]+)["""]"#).ok();
        if let Some(pattern) = quote_pattern {
            for cap in pattern.captures_iter(text) {
                if let Some(m) = cap.get(1) {
                    keywords.push(m.as_str().to_string());
                }
            }
        }

        // 如果没有引号内容，提取名词性词汇
        if keywords.is_empty() {
            let words: Vec<&str> = text.split_whitespace().collect();
            for word in words {
                if word.len() > 2 && !Self::is_stop_word(word) {
                    keywords.push(word.to_string());
                }
            }
        }

        keywords
    }

    fn is_stop_word(word: &str) -> bool {
        let stop_words = [
            "的", "了", "是", "在", "我", "有", "和", "就", "不", "人",
            "都", "一", "一个", "上", "也", "很", "到", "说", "要", "去",
            "你", "会", "着", "没有", "看", "好", "自己", "这",
        ];
        stop_words.contains(&word)
    }
}

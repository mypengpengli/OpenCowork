/// 从屏幕内容中提取关键信息
pub struct InfoExtractor;

impl InfoExtractor {
    /// 从窗口标题提取应用名称
    pub fn extract_app_from_title(title: &str) -> String {
        // 常见的应用标题模式
        let patterns = [
            (" - Visual Studio Code", "Visual Studio Code"),
            (" - Google Chrome", "Google Chrome"),
            (" - Mozilla Firefox", "Mozilla Firefox"),
            (" - Microsoft Edge", "Microsoft Edge"),
            (" - Notepad++", "Notepad++"),
            (" | Microsoft Teams", "Microsoft Teams"),
            (" - Slack", "Slack"),
            (" - Discord", "Discord"),
        ];

        for (pattern, app_name) in patterns {
            if title.contains(pattern) {
                return app_name.to_string();
            }
        }

        // 尝试从标题末尾提取应用名
        if let Some(last_part) = title.split(" - ").last() {
            return last_part.trim().to_string();
        }

        "Unknown".to_string()
    }

    /// 从窗口标题提取文件名
    pub fn extract_file_from_title(title: &str) -> Option<String> {
        // VS Code 格式: "filename.ext - folder - Visual Studio Code"
        if title.contains("Visual Studio Code") {
            if let Some(first_part) = title.split(" - ").next() {
                let trimmed = first_part.trim();
                if trimmed.contains('.') {
                    return Some(trimmed.to_string());
                }
            }
        }

        // 通用格式: 尝试找到带扩展名的部分
        for part in title.split(&[' ', '-', '|'][..]) {
            let trimmed = part.trim();
            if trimmed.contains('.') && trimmed.len() < 100 {
                // 检查是否像文件名
                let ext_patterns = [".rs", ".ts", ".js", ".py", ".vue", ".tsx", ".jsx", ".md", ".json", ".html", ".css"];
                for ext in ext_patterns {
                    if trimmed.ends_with(ext) {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }

        None
    }

    /// 推断活动类型
    pub fn infer_activity_type(app: &str, title: &str, content: &str) -> String {
        let app_lower = app.to_lowercase();
        let title_lower = title.to_lowercase();
        let content_lower = content.to_lowercase();

        // 编程活动
        if app_lower.contains("code") || app_lower.contains("studio") || app_lower.contains("idea") {
            return "coding".to_string();
        }

        // 浏览器活动
        if app_lower.contains("chrome") || app_lower.contains("firefox") || app_lower.contains("edge") {
            if content_lower.contains("github") || content_lower.contains("stackoverflow") {
                return "researching".to_string();
            }
            return "browsing".to_string();
        }

        // 通讯活动
        if app_lower.contains("teams") || app_lower.contains("slack") || app_lower.contains("discord") {
            return "communicating".to_string();
        }

        // 文档活动
        if app_lower.contains("word") || app_lower.contains("docs") || app_lower.contains("notion") {
            return "documenting".to_string();
        }

        // 终端活动
        if app_lower.contains("terminal") || app_lower.contains("cmd") || app_lower.contains("powershell") {
            return "terminal".to_string();
        }

        "general".to_string()
    }

    /// 提取关键词
    pub fn extract_keywords(content: &str, max_keywords: usize) -> Vec<String> {
        let stop_words = ["the", "a", "an", "is", "are", "was", "were", "be", "been",
            "being", "have", "has", "had", "do", "does", "did", "will", "would",
            "could", "should", "may", "might", "must", "shall", "can", "need",
            "to", "of", "in", "for", "on", "with", "at", "by", "from", "as",
            "into", "through", "during", "before", "after", "above", "below",
            "between", "under", "again", "further", "then", "once", "here",
            "there", "when", "where", "why", "how", "all", "each", "few",
            "more", "most", "other", "some", "such", "no", "nor", "not",
            "only", "own", "same", "so", "than", "too", "very", "just"];

        let mut word_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        for word in content.split_whitespace() {
            let cleaned: String = word
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();

            if cleaned.len() > 2 && !stop_words.contains(&cleaned.as_str()) {
                *word_count.entry(cleaned).or_insert(0) += 1;
            }
        }

        let mut words: Vec<_> = word_count.into_iter().collect();
        words.sort_by(|a, b| b.1.cmp(&a.1));

        words.into_iter()
            .take(max_keywords)
            .map(|(word, _)| word)
            .collect()
    }
}

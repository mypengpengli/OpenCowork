pub struct DiffAnalyzer;

impl DiffAnalyzer {
    /// 比较两个文本的相似度 (0.0 - 1.0)
    pub fn text_similarity(text1: &str, text2: &str) -> f64 {
        if text1.is_empty() && text2.is_empty() {
            return 1.0;
        }
        if text1.is_empty() || text2.is_empty() {
            return 0.0;
        }

        let words1: std::collections::HashSet<&str> = text1.split_whitespace().collect();
        let words2: std::collections::HashSet<&str> = text2.split_whitespace().collect();

        let intersection = words1.intersection(&words2).count();
        let union = words1.union(&words2).count();

        if union == 0 {
            return 1.0;
        }

        intersection as f64 / union as f64
    }

    /// 判断内容是否有显著变化
    pub fn has_significant_change(prev_text: &str, curr_text: &str, threshold: f64) -> bool {
        let similarity = Self::text_similarity(prev_text, curr_text);
        similarity < threshold
    }

    /// 提取变化的关键词
    pub fn extract_changed_keywords(prev_text: &str, curr_text: &str) -> Vec<String> {
        let prev_words: std::collections::HashSet<&str> = prev_text.split_whitespace().collect();
        let curr_words: std::collections::HashSet<&str> = curr_text.split_whitespace().collect();

        // 新增的词
        let added: Vec<String> = curr_words
            .difference(&prev_words)
            .take(10)
            .map(|s| s.to_string())
            .collect();

        added
    }
}

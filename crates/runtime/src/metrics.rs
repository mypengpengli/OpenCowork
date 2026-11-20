use crate::TokenUsage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RequestMetrics {
    pub id: String,
    pub model: String,
    pub started_at: u64,
    pub first_event_ms: Option<u64>,
    pub first_text_ms: Option<u64>,
    pub elapsed_ms: u64,
    pub status: String,
    pub usage: TokenUsage,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CompactionMetrics {
    pub elapsed_ms: u64,
    pub before_tokens: usize,
    pub after_tokens: usize,
    pub usage: TokenUsage,
}

/// Conservative multilingual estimate until an actual provider measurement exists.
pub fn estimate_text_tokens(text: &str) -> usize {
    let mut ascii = 0usize;
    let mut other = 0usize;
    for c in text.chars() {
        if c.is_ascii() {
            ascii += 1
        } else {
            other += if c.len_utf8() == 4 { 3 } else { 2 }
        }
    }
    ascii.div_ceil(3) + other
}

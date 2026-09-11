use serde::{Deserialize, Serialize};

use crate::session::Session;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Uncached input; cache buckets are disjoint, without overflowing aggregate counters.
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_creation_input_tokens: u32,
    pub cache_read_input_tokens: u32,
}

impl TokenUsage {
    #[must_use]
    pub fn total_tokens(self) -> u32 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cache_creation_input_tokens)
            .saturating_add(self.cache_read_input_tokens)
    }

    pub fn accumulate(&mut self, other: Self) {
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cache_creation_input_tokens = self
            .cache_creation_input_tokens
            .saturating_add(other.cache_creation_input_tokens);
        self.cache_read_input_tokens = self
            .cache_read_input_tokens
            .saturating_add(other.cache_read_input_tokens);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsageTracker {
    turns: usize,
    cumulative: TokenUsage,
}

impl UsageTracker {
    #[must_use]
    pub fn from_session(session: &Session) -> Self {
        let mut tracker = Self::default();
        for message in &session.messages {
            if let Some(usage) = message.usage {
                tracker.record(usage);
            }
        }
        tracker
    }

    pub fn record(&mut self, usage: TokenUsage) {
        self.turns += 1;
        self.cumulative.accumulate(usage);
    }

    #[must_use]
    pub fn turns(&self) -> usize {
        self.turns
    }

    #[must_use]
    pub fn cumulative(&self) -> TokenUsage {
        self.cumulative
    }
}

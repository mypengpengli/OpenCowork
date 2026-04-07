const MODEL_CONTEXT_WINDOW_DEFAULT: usize = 200_000;
const MODEL_CONTEXT_WINDOW_1M: usize = 1_000_000;
const DEFAULT_AUTO_TOOL_SEARCH_PERCENTAGE: usize = 10;
const AUTO_TOOL_SEARCH_CHARS_PER_TOKEN_NUMERATOR: usize = 5;
const AUTO_TOOL_SEARCH_CHARS_PER_TOKEN_DENOMINATOR: usize = 2;

pub const COMPACT_MAX_OUTPUT_TOKENS: usize = 20_000;
pub const MAX_OUTPUT_TOKENS_DEFAULT: usize = 32_000;
pub const AUTOCOMPACT_BUFFER_TOKENS: usize = 13_000;
pub const WARNING_THRESHOLD_BUFFER_TOKENS: usize = 20_000;
pub const ERROR_THRESHOLD_BUFFER_TOKENS: usize = 20_000;
pub const MANUAL_COMPACT_BUFFER_TOKENS: usize = 3_000;
pub const SKILL_BUDGET_CONTEXT_PERCENT: usize = 1;
pub const SKILL_BUDGET_CHARS_PER_TOKEN: usize = 4;
pub const DEFAULT_SKILL_CHAR_BUDGET: usize = 8_000;
pub const MAX_SKILL_LISTING_DESC_CHARS: usize = 250;
pub const DEFAULT_MAX_TOOL_RESULT_CHARS: usize = 50_000;
pub const DEFAULT_MAX_TOOL_RESULTS_PER_MESSAGE_CHARS: usize = 200_000;
pub const DEFAULT_TOOL_RESULT_PREVIEW_CHARS: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextThresholds {
    pub context_window: usize,
    pub effective_context_window: usize,
    pub auto_compact_threshold: usize,
    pub warning_threshold: usize,
    pub error_threshold: usize,
    pub blocking_limit: usize,
}

#[must_use]
pub fn get_context_window_for_model(model: &str, override_window: Option<usize>) -> usize {
    if let Some(override_window) = override_window.filter(|value| *value > 0) {
        return override_window;
    }
    if has_explicit_1m_context(model) {
        return MODEL_CONTEXT_WINDOW_1M;
    }
    MODEL_CONTEXT_WINDOW_DEFAULT
}

#[must_use]
pub fn get_effective_context_window_size(
    model: &str,
    override_window: Option<usize>,
    max_output_tokens: usize,
) -> usize {
    let context_window =
        apply_auto_compact_window_override(get_context_window_for_model(model, override_window));
    let reserved_tokens = max_output_tokens.min(COMPACT_MAX_OUTPUT_TOKENS);
    context_window.saturating_sub(reserved_tokens)
}

#[must_use]
pub fn get_auto_compact_threshold(
    model: &str,
    override_window: Option<usize>,
    max_output_tokens: usize,
    auto_compact_buffer_tokens: usize,
) -> usize {
    let effective_window =
        get_effective_context_window_size(model, override_window, max_output_tokens);
    let default_threshold = effective_window.saturating_sub(auto_compact_buffer_tokens);
    apply_auto_compact_pct_override(effective_window, default_threshold)
}

#[must_use]
pub fn calculate_context_thresholds(
    model: &str,
    override_window: Option<usize>,
    max_output_tokens: usize,
    auto_compact_buffer_tokens: usize,
    warning_buffer_tokens: usize,
    error_buffer_tokens: usize,
    manual_compact_buffer_tokens: usize,
) -> ContextThresholds {
    let context_window =
        apply_auto_compact_window_override(get_context_window_for_model(model, override_window));
    let effective_context_window =
        get_effective_context_window_size(model, override_window, max_output_tokens);
    let auto_compact_threshold = apply_auto_compact_pct_override(
        effective_context_window,
        effective_context_window.saturating_sub(auto_compact_buffer_tokens),
    );

    ContextThresholds {
        context_window,
        effective_context_window,
        auto_compact_threshold,
        warning_threshold: auto_compact_threshold.saturating_sub(warning_buffer_tokens),
        error_threshold: auto_compact_threshold.saturating_sub(error_buffer_tokens),
        blocking_limit: effective_context_window.saturating_sub(manual_compact_buffer_tokens),
    }
}

#[must_use]
pub fn get_skill_char_budget(context_window_tokens: Option<usize>) -> usize {
    context_window_tokens.map_or(DEFAULT_SKILL_CHAR_BUDGET, |tokens| {
        tokens
            .saturating_mul(SKILL_BUDGET_CHARS_PER_TOKEN)
            .saturating_mul(SKILL_BUDGET_CONTEXT_PERCENT)
            / 100
    })
}

#[must_use]
pub fn get_auto_tool_search_percentage() -> usize {
    let Some(value) = std::env::var("ENABLE_TOOL_SEARCH").ok() else {
        return DEFAULT_AUTO_TOOL_SEARCH_PERCENTAGE;
    };
    if value.eq_ignore_ascii_case("auto") {
        return DEFAULT_AUTO_TOOL_SEARCH_PERCENTAGE;
    }
    parse_auto_tool_search_percentage(&value).unwrap_or(DEFAULT_AUTO_TOOL_SEARCH_PERCENTAGE)
}

#[must_use]
pub fn get_auto_tool_search_char_threshold(model: &str, override_window: Option<usize>) -> usize {
    get_context_window_for_model(model, override_window)
        .saturating_mul(get_auto_tool_search_percentage())
        .saturating_mul(AUTO_TOOL_SEARCH_CHARS_PER_TOKEN_NUMERATOR)
        / (100 * AUTO_TOOL_SEARCH_CHARS_PER_TOKEN_DENOMINATOR)
}

#[must_use]
fn has_explicit_1m_context(model: &str) -> bool {
    model.to_ascii_lowercase().contains("[1m]")
}

fn apply_auto_compact_window_override(context_window: usize) -> usize {
    let Some(override_window) = std::env::var("CLAUDE_CODE_AUTO_COMPACT_WINDOW")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
    else {
        return context_window;
    };
    context_window.min(override_window)
}

fn apply_auto_compact_pct_override(
    effective_context_window: usize,
    default_threshold: usize,
) -> usize {
    let Some(override_percent) = std::env::var("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE")
        .ok()
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|value| *value > 0.0 && *value <= 100.0)
    else {
        return default_threshold;
    };

    let percentage_threshold = ((effective_context_window as f64) * (override_percent / 100.0))
        .floor()
        .max(0.0) as usize;
    percentage_threshold.min(default_threshold)
}

fn parse_auto_tool_search_percentage(value: &str) -> Option<usize> {
    let percent = value.trim().strip_prefix("auto:")?.parse::<usize>().ok()?;
    Some(percent.clamp(0, 100))
}

#[cfg(test)]
mod tests {
    use super::{
        apply_auto_compact_pct_override, calculate_context_thresholds, get_auto_compact_threshold,
        get_auto_tool_search_char_threshold, get_effective_context_window_size,
        get_skill_char_budget, parse_auto_tool_search_percentage, AUTOCOMPACT_BUFFER_TOKENS,
        ERROR_THRESHOLD_BUFFER_TOKENS, MANUAL_COMPACT_BUFFER_TOKENS, MAX_OUTPUT_TOKENS_DEFAULT,
        WARNING_THRESHOLD_BUFFER_TOKENS,
    };

    #[test]
    fn computes_reference_like_thresholds_for_200k_models() {
        assert_eq!(
            get_effective_context_window_size("gpt-5.4-mini", None, MAX_OUTPUT_TOKENS_DEFAULT),
            180_000
        );
        assert_eq!(
            get_auto_compact_threshold(
                "gpt-5.4-mini",
                None,
                MAX_OUTPUT_TOKENS_DEFAULT,
                AUTOCOMPACT_BUFFER_TOKENS
            ),
            167_000
        );
    }

    #[test]
    fn supports_explicit_1m_models() {
        assert_eq!(
            get_effective_context_window_size("gpt-5.4[1m]", None, MAX_OUTPUT_TOKENS_DEFAULT),
            980_000
        );
    }

    #[test]
    fn derives_warning_and_blocking_thresholds() {
        let thresholds = calculate_context_thresholds(
            "gpt-5.4-mini",
            None,
            MAX_OUTPUT_TOKENS_DEFAULT,
            AUTOCOMPACT_BUFFER_TOKENS,
            WARNING_THRESHOLD_BUFFER_TOKENS,
            ERROR_THRESHOLD_BUFFER_TOKENS,
            MANUAL_COMPACT_BUFFER_TOKENS,
        );
        assert_eq!(thresholds.context_window, 200_000);
        assert_eq!(thresholds.effective_context_window, 180_000);
        assert_eq!(thresholds.auto_compact_threshold, 167_000);
        assert_eq!(thresholds.warning_threshold, 147_000);
        assert_eq!(thresholds.error_threshold, 147_000);
        assert_eq!(thresholds.blocking_limit, 177_000);
    }

    #[test]
    fn uses_reference_skill_budget_defaults() {
        assert_eq!(get_skill_char_budget(None), 8_000);
        assert_eq!(get_skill_char_budget(Some(200_000)), 8_000);
        assert_eq!(get_skill_char_budget(Some(1_000_000)), 40_000);
    }

    #[test]
    fn parses_auto_tool_search_percentages() {
        assert_eq!(parse_auto_tool_search_percentage("auto:0"), Some(0));
        assert_eq!(parse_auto_tool_search_percentage("auto:10"), Some(10));
        assert_eq!(parse_auto_tool_search_percentage("auto:150"), Some(100));
        assert_eq!(parse_auto_tool_search_percentage("true"), None);
    }

    #[test]
    fn computes_reference_like_auto_tool_search_threshold() {
        assert_eq!(
            get_auto_tool_search_char_threshold("gpt-5.4-mini", Some(200_000)),
            50_000
        );
    }

    #[test]
    fn allows_percentage_override_to_lower_auto_compact_threshold() {
        assert_eq!(apply_auto_compact_pct_override(180_000, 167_000), 167_000);
    }
}

use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    estimate_session_tokens, hydrate_current_session_memory, refresh_current_session_memory,
    ContentBlock, Session,
};

const EXTRACTION_WAIT_TIMEOUT_MS: u128 = 15_000;
const EXTRACTION_STALE_THRESHOLD_MS: u128 = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMemoryConfig {
    pub minimum_message_tokens_to_init: usize,
    pub minimum_tokens_between_update: usize,
    pub tool_calls_between_updates: usize,
}

impl Default for SessionMemoryConfig {
    fn default() -> Self {
        Self {
            minimum_message_tokens_to_init: 10_000,
            minimum_tokens_between_update: 5_000,
            tool_calls_between_updates: 3,
        }
    }
}

pub fn maybe_refresh_session_memory(
    session: &mut Session,
    cwd: &Path,
    config_home: &Path,
) -> Result<Option<PathBuf>, std::io::Error> {
    maybe_refresh_session_memory_with_config(
        session,
        cwd,
        config_home,
        &SessionMemoryConfig::default(),
    )
}

pub fn maybe_schedule_session_memory_refresh(
    session: &mut Session,
    cwd: &Path,
    config_home: &Path,
) -> Result<bool, std::io::Error> {
    maybe_schedule_session_memory_refresh_with_task_and_config(
        session,
        cwd,
        config_home,
        &SessionMemoryConfig::default(),
        |mut background_session, cwd, config_home| {
            let _ = refresh_current_session_memory(&mut background_session, &cwd, &config_home);
        },
    )
}

pub fn maybe_schedule_session_memory_refresh_with_task<F>(
    session: &mut Session,
    cwd: &Path,
    config_home: &Path,
    task: F,
) -> Result<bool, std::io::Error>
where
    F: FnOnce(Session, PathBuf, PathBuf) + Send + 'static,
{
    maybe_schedule_session_memory_refresh_with_task_and_config(
        session,
        cwd,
        config_home,
        &SessionMemoryConfig::default(),
        task,
    )
}

pub fn maybe_schedule_session_memory_refresh_with_task_and_config<F>(
    session: &mut Session,
    cwd: &Path,
    config_home: &Path,
    config: &SessionMemoryConfig,
    task: F,
) -> Result<bool, std::io::Error>
where
    F: FnOnce(Session, PathBuf, PathBuf) + Send + 'static,
{
    let current_token_count = estimate_session_tokens(session);
    if !should_refresh_session_memory(session, current_token_count, config) {
        return Ok(false);
    }

    let background_session = session.clone();
    let cwd = cwd.to_path_buf();
    let config_home = config_home.to_path_buf();
    std::thread::Builder::new()
        .name("opencowork-session-memory".to_string())
        .spawn(move || task(background_session, cwd, config_home))
        .map_err(|error| {
            clear_session_memory_refresh_flag(session);
            std::io::Error::other(error)
        })?;
    Ok(true)
}

pub fn maybe_refresh_session_memory_with_config(
    session: &mut Session,
    cwd: &Path,
    config_home: &Path,
    config: &SessionMemoryConfig,
) -> Result<Option<PathBuf>, std::io::Error> {
    let current_token_count = estimate_session_tokens(session);
    if !should_refresh_session_memory(session, current_token_count, config) {
        return Ok(None);
    }

    let result = refresh_current_session_memory(session, cwd, config_home);
    match result {
        Ok(path) => {
            mark_session_memory_refresh_completed(session, current_token_count);
            Ok(path)
        }
        Err(error) => {
            clear_session_memory_refresh_flag(session);
            Err(error)
        }
    }
}

pub fn wait_for_session_memory_refresh(
    session: &mut Session,
    cwd: &Path,
    config_home: &Path,
) -> Result<(), std::io::Error> {
    let wait_started_at = now_ms();
    while session
        .session_memory_state
        .extraction_started_at_unix_ms
        .is_some()
    {
        let refreshed = hydrate_current_session_memory(session, cwd, config_home)?;
        *session = refreshed;
        let Some(extraction_started_at) =
            session.session_memory_state.extraction_started_at_unix_ms
        else {
            return Ok(());
        };

        if now_ms().saturating_sub(extraction_started_at) > EXTRACTION_STALE_THRESHOLD_MS {
            session.session_memory_state.extraction_started_at_unix_ms = None;
            return Ok(());
        }
        if now_ms().saturating_sub(wait_started_at) > EXTRACTION_WAIT_TIMEOUT_MS {
            return Ok(());
        }

        thread::sleep(Duration::from_millis(1_000));
    }

    Ok(())
}

pub fn should_refresh_session_memory(
    session: &mut Session,
    current_token_count: usize,
    config: &SessionMemoryConfig,
) -> bool {
    let extraction_started_at = session.session_memory_state.extraction_started_at_unix_ms;
    if extraction_started_at
        .is_some_and(|started| now_ms().saturating_sub(started) < EXTRACTION_STALE_THRESHOLD_MS)
    {
        return false;
    }
    if extraction_started_at.is_some() {
        session.session_memory_state.extraction_started_at_unix_ms = None;
    }

    if !session.session_memory_state.initialized {
        if current_token_count < config.minimum_message_tokens_to_init {
            return false;
        }
        session.session_memory_state.initialized = true;
    }

    let tokens_since_last_extraction =
        current_token_count.saturating_sub(session.session_memory_state.tokens_at_last_extraction);
    let has_met_token_threshold =
        tokens_since_last_extraction >= config.minimum_tokens_between_update;
    if !has_met_token_threshold {
        return false;
    }

    let last_triggered_message_count = session.session_memory_state.last_triggered_message_count;
    let tool_calls_since_last_update =
        count_tool_calls_since(session, last_triggered_message_count);
    let has_met_tool_call_threshold =
        tool_calls_since_last_update >= config.tool_calls_between_updates;
    let should_refresh =
        has_met_tool_call_threshold || !has_tool_calls_in_last_assistant_turn(session);
    if should_refresh {
        let state = &mut session.session_memory_state;
        state.extraction_started_at_unix_ms = Some(now_ms());
        state.last_triggered_message_count = session.messages.len();
    }
    should_refresh
}

fn mark_session_memory_refresh_completed(session: &mut Session, current_token_count: usize) {
    let should_advance_summary_boundary = !has_tool_calls_in_last_assistant_turn(session);
    let summarized_message_count = session.messages.len();
    let state = &mut session.session_memory_state;
    state.initialized = true;
    state.tokens_at_last_extraction = current_token_count;
    if should_advance_summary_boundary {
        state.last_summarized_message_count = summarized_message_count;
    }
    state.extraction_started_at_unix_ms = None;
}

fn clear_session_memory_refresh_flag(session: &mut Session) {
    session.session_memory_state.extraction_started_at_unix_ms = None;
}

fn count_tool_calls_since(session: &Session, start_message_count: usize) -> usize {
    session
        .messages
        .iter()
        .skip(start_message_count)
        .map(|message| {
            message
                .blocks
                .iter()
                .filter(|block| matches!(block, ContentBlock::ToolUse { .. }))
                .count()
        })
        .sum()
}

fn has_tool_calls_in_last_assistant_turn(session: &Session) -> bool {
    session
        .messages
        .iter()
        .rev()
        .find(|message| matches!(message.role, crate::MessageRole::Assistant))
        .is_some_and(|message| {
            message
                .blocks
                .iter()
                .any(|block| matches!(block, ContentBlock::ToolUse { .. }))
        })
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::{
        maybe_refresh_session_memory_with_config,
        maybe_schedule_session_memory_refresh_with_task_and_config, should_refresh_session_memory,
        SessionMemoryConfig,
    };
    use crate::{ContentBlock, ConversationMessage, Session};
    use std::fs;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("opencowork-{prefix}-{stamp}"))
    }

    #[test]
    fn does_not_refresh_before_thresholds() {
        let mut session = Session::from_messages(vec![ConversationMessage::user("short request")]);
        let should =
            should_refresh_session_memory(&mut session, 100, &SessionMemoryConfig::default());
        assert!(!should);
    }

    #[test]
    fn refreshes_when_thresholds_are_met() {
        let cwd = temp_dir("session-memory-cwd");
        let config_home = temp_dir("session-memory-config");
        let mut session = Session::from_messages(vec![
            ConversationMessage::user("A".repeat(12_000)),
            ConversationMessage::assistant(
                vec![ContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "read_file".to_string(),
                    input: r#"{"path":"src/main.ts"}"#.to_string(),
                }],
                None,
            ),
            ConversationMessage::assistant(
                vec![ContentBlock::ToolUse {
                    id: "tool-2".to_string(),
                    name: "read_file".to_string(),
                    input: r#"{"path":"src/app.ts"}"#.to_string(),
                }],
                None,
            ),
            ConversationMessage::assistant(
                vec![ContentBlock::ToolUse {
                    id: "tool-3".to_string(),
                    name: "read_file".to_string(),
                    input: r#"{"path":"src/lib.ts"}"#.to_string(),
                }],
                None,
            ),
        ]);

        let path = maybe_refresh_session_memory_with_config(
            &mut session,
            &cwd,
            &config_home,
            &SessionMemoryConfig {
                minimum_message_tokens_to_init: 100,
                minimum_tokens_between_update: 50,
                tool_calls_between_updates: 3,
            },
        )
        .expect("refresh result");

        assert!(path.is_some());
        assert!(session.current_session_memory.is_some());
        assert_eq!(
            session.session_memory_state.last_summarized_message_count,
            0
        );
        assert!(session.session_memory_state.tokens_at_last_extraction > 0);

        let _ = fs::remove_dir_all(cwd);
        let _ = fs::remove_dir_all(config_home);
    }

    #[test]
    fn schedules_background_refresh_when_thresholds_are_met() {
        let cwd = temp_dir("session-memory-cwd");
        let config_home = temp_dir("session-memory-config");
        let mut session = Session::from_messages(vec![
            ConversationMessage::user("A".repeat(12_000)),
            ConversationMessage::assistant(
                vec![ContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "read_file".to_string(),
                    input: r#"{"path":"src/main.ts"}"#.to_string(),
                }],
                None,
            ),
            ConversationMessage::assistant(
                vec![ContentBlock::Text {
                    text: "Now continue without another tool.".to_string(),
                }],
                None,
            ),
        ]);

        let scheduled = maybe_schedule_session_memory_refresh_with_task_and_config(
            &mut session,
            &cwd,
            &config_home,
            &SessionMemoryConfig {
                minimum_message_tokens_to_init: 100,
                minimum_tokens_between_update: 50,
                tool_calls_between_updates: 1,
            },
            |mut background_session, cwd, config_home| {
                let _ = crate::refresh_current_session_memory(
                    &mut background_session,
                    &cwd,
                    &config_home,
                );
            },
        )
        .expect("schedule result");

        assert!(scheduled);
        assert!(session
            .session_memory_state
            .extraction_started_at_unix_ms
            .is_some());

        let path = crate::project_session_memory_path(&config_home, &cwd, &session);
        for _ in 0..20 {
            if path.exists() {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(path.exists());

        let _ = fs::remove_dir_all(cwd);
        let _ = fs::remove_dir_all(config_home);
    }
}

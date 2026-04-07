use crate::config::{RuntimeHookCommand, RuntimeHookConfig, RuntimeHookEvent};
use std::collections::BTreeMap;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookOutcome {
    denied: bool,
    messages: Vec<String>,
}

impl HookOutcome {
    #[must_use]
    pub fn allowed(messages: Vec<String>) -> Self {
        Self {
            denied: false,
            messages,
        }
    }

    #[must_use]
    pub fn denied(messages: Vec<String>) -> Self {
        Self {
            denied: true,
            messages,
        }
    }

    #[must_use]
    pub fn is_denied(&self) -> bool {
        self.denied
    }

    #[must_use]
    pub fn messages(&self) -> &[String] {
        &self.messages
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HookRunner {
    hooks: BTreeMap<RuntimeHookEvent, Vec<RuntimeHookCommand>>,
}

#[derive(Debug, Clone, Default)]
struct HookInvocation {
    tool_name: Option<String>,
    tool_input: Option<String>,
    tool_output: Option<String>,
    tool_is_error: Option<bool>,
    user_input: Option<String>,
    message: Option<String>,
    permission_reason: Option<String>,
    file_paths: Vec<String>,
}

impl HookRunner {
    #[must_use]
    pub fn from_hook_config(config: &RuntimeHookConfig) -> Self {
        Self {
            hooks: config.entries().clone(),
        }
    }

    #[must_use]
    pub fn run_session_start(&mut self) -> HookOutcome {
        self.run_event(RuntimeHookEvent::SessionStart, HookInvocation::default())
    }

    #[must_use]
    pub fn run_session_end(&mut self, message: Option<&str>) -> HookOutcome {
        self.run_event(
            RuntimeHookEvent::SessionEnd,
            HookInvocation {
                message: message.map(ToOwned::to_owned),
                ..HookInvocation::default()
            },
        )
    }

    #[must_use]
    pub fn run_user_prompt_submit(&mut self, user_input: &str) -> HookOutcome {
        self.run_event(
            RuntimeHookEvent::UserPromptSubmit,
            HookInvocation {
                user_input: Some(user_input.to_string()),
                ..HookInvocation::default()
            },
        )
    }

    #[must_use]
    pub fn run_instructions_loaded(&mut self, file_paths: &[String]) -> HookOutcome {
        self.run_event(
            RuntimeHookEvent::InstructionsLoaded,
            HookInvocation {
                file_paths: file_paths.to_vec(),
                ..HookInvocation::default()
            },
        )
    }

    #[must_use]
    pub fn run_file_changed(&mut self, file_paths: &[String]) -> HookOutcome {
        self.run_event(
            RuntimeHookEvent::FileChanged,
            HookInvocation {
                file_paths: file_paths.to_vec(),
                ..HookInvocation::default()
            },
        )
    }

    #[must_use]
    pub fn run_permission_denied(
        &mut self,
        tool_name: &str,
        input: &str,
        reason: &str,
    ) -> HookOutcome {
        self.run_event(
            RuntimeHookEvent::PermissionDenied,
            HookInvocation {
                tool_name: Some(tool_name.to_string()),
                tool_input: Some(input.to_string()),
                permission_reason: Some(reason.to_string()),
                ..HookInvocation::default()
            },
        )
    }

    #[must_use]
    pub fn run_pre_tool_use(&mut self, tool_name: &str, input: &str) -> HookOutcome {
        self.run_event(
            RuntimeHookEvent::PreToolUse,
            HookInvocation {
                tool_name: Some(tool_name.to_string()),
                tool_input: Some(input.to_string()),
                ..HookInvocation::default()
            },
        )
    }

    #[must_use]
    pub fn run_post_tool_use(
        &mut self,
        tool_name: &str,
        input: &str,
        output: &str,
        is_error: bool,
    ) -> HookOutcome {
        let event = if is_error {
            RuntimeHookEvent::PostToolUseFailure
        } else {
            RuntimeHookEvent::PostToolUse
        };
        self.run_event(
            event,
            HookInvocation {
                tool_name: Some(tool_name.to_string()),
                tool_input: Some(input.to_string()),
                tool_output: Some(output.to_string()),
                tool_is_error: Some(is_error),
                ..HookInvocation::default()
            },
        )
    }

    fn run_event(&mut self, event: RuntimeHookEvent, invocation: HookInvocation) -> HookOutcome {
        let Some(entries) = self.hooks.get_mut(&event) else {
            return HookOutcome::allowed(Vec::new());
        };

        let mut messages = Vec::new();
        let mut kept = Vec::new();

        for entry in std::mem::take(entries) {
            if !matches_hook(&entry, event, &invocation) {
                kept.push(entry);
                continue;
            }

            let result = run_hook_command(event, &entry, &invocation);
            if !entry.once() {
                kept.push(entry);
            }

            match result {
                Ok(stdout) => {
                    if !stdout.is_empty() {
                        messages.push(stdout);
                    }
                }
                Err(message) => {
                    messages.push(message);
                    *entries = kept;
                    return HookOutcome::denied(messages);
                }
            }
        }

        *entries = kept;
        HookOutcome::allowed(messages)
    }
}

fn run_hook_command(
    event: RuntimeHookEvent,
    entry: &RuntimeHookCommand,
    invocation: &HookInvocation,
) -> Result<String, String> {
    let mut command = shell_command(entry.command());
    command.env("OPENCOWORK_HOOK_EVENT", event.label());

    if let Some(tool_name) = &invocation.tool_name {
        command.env("OPENCOWORK_TOOL_NAME", tool_name);
    }
    if let Some(tool_input) = &invocation.tool_input {
        command.env("OPENCOWORK_TOOL_INPUT", tool_input);
    }
    if let Some(tool_output) = &invocation.tool_output {
        command.env("OPENCOWORK_TOOL_OUTPUT", tool_output);
    }
    if let Some(is_error) = invocation.tool_is_error {
        command.env(
            "OPENCOWORK_TOOL_IS_ERROR",
            if is_error { "true" } else { "false" },
        );
    }
    if let Some(user_input) = &invocation.user_input {
        command.env("OPENCOWORK_USER_INPUT", user_input);
    }
    if let Some(message) = &invocation.message {
        command.env("OPENCOWORK_HOOK_MESSAGE", message);
    }
    if let Some(reason) = &invocation.permission_reason {
        command.env("OPENCOWORK_PERMISSION_REASON", reason);
    }
    if !invocation.file_paths.is_empty() {
        command
            .env("OPENCOWORK_FILE_PATHS", invocation.file_paths.join(";"))
            .env(
                "OPENCOWORK_FILE_PATHS_JSON",
                serde_json::to_string(&invocation.file_paths).unwrap_or_default(),
            );
    }

    match command.output() {
        Ok(result) if result.status.success() => {
            Ok(String::from_utf8_lossy(&result.stdout).trim().to_string())
        }
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr).trim().to_string();
            Err(if stderr.is_empty() {
                format!("hook failed with status {}", result.status)
            } else {
                stderr
            })
        }
        Err(error) => Err(format!("hook spawn failed: {error}")),
    }
}

fn matches_hook(
    entry: &RuntimeHookCommand,
    event: RuntimeHookEvent,
    invocation: &HookInvocation,
) -> bool {
    let Some(matcher) = entry.matcher() else {
        return true;
    };

    let normalized_matcher = matcher.trim().to_ascii_lowercase();
    if normalized_matcher.is_empty() {
        return true;
    }

    hook_candidates(event, invocation)
        .into_iter()
        .any(|candidate| {
            wildcard_match(
                &normalized_matcher,
                &candidate.trim().replace('\\', "/").to_ascii_lowercase(),
            )
        })
}

fn hook_candidates(event: RuntimeHookEvent, invocation: &HookInvocation) -> Vec<String> {
    let mut candidates = vec![event.label().to_string()];
    if let Some(tool_name) = &invocation.tool_name {
        candidates.push(tool_name.clone());
    }
    if let Some(user_input) = &invocation.user_input {
        candidates.push(user_input.clone());
    }
    if let Some(message) = &invocation.message {
        candidates.push(message.clone());
    }
    candidates.extend(invocation.file_paths.iter().cloned());
    candidates
}

fn wildcard_match(pattern: &str, candidate: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let parts = pattern.split('*').collect::<Vec<_>>();
    if parts.len() == 1 {
        return candidate == pattern;
    }

    let mut cursor = 0usize;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if index == 0 && !candidate[cursor..].starts_with(part) {
            return false;
        }

        let Some(found) = candidate[cursor..].find(part) else {
            return false;
        };
        cursor += found + part.len();
    }

    pattern.ends_with('*') || parts.last().is_none_or(|part| candidate.ends_with(part))
}

#[cfg(windows)]
fn shell_command(script: &str) -> Command {
    let mut command = Command::new("powershell");
    command.args(["-NoLogo", "-NoProfile", "-Command", script]);
    command
}

#[cfg(not(windows))]
fn shell_command(script: &str) -> Command {
    let mut command = Command::new("sh");
    command.args(["-lc", script]);
    command
}

#[cfg(test)]
mod tests {
    use super::HookRunner;
    use crate::config::{RuntimeHookCommand, RuntimeHookConfig, RuntimeHookEvent};
    use std::collections::BTreeMap;

    #[test]
    fn once_hooks_are_removed_after_execution() {
        let mut runner =
            HookRunner::from_hook_config(&RuntimeHookConfig::with_commands(BTreeMap::from([(
                RuntimeHookEvent::SessionStart,
                vec![RuntimeHookCommand::new("Write-Output hello", None, true)],
            )])));

        let first = runner.run_session_start();
        let second = runner.run_session_start();
        assert_eq!(first.messages(), &["hello".to_string()]);
        assert!(second.messages().is_empty());
    }

    #[test]
    fn matcher_filters_non_matching_hooks() {
        let mut runner =
            HookRunner::from_hook_config(&RuntimeHookConfig::with_commands(BTreeMap::from([(
                RuntimeHookEvent::PreToolUse,
                vec![RuntimeHookCommand::new(
                    "Write-Output matched",
                    Some("read_*".to_string()),
                    false,
                )],
            )])));

        assert!(runner
            .run_pre_tool_use("write_file", "{}")
            .messages()
            .is_empty());
        assert_eq!(
            runner.run_pre_tool_use("read_file", "{}").messages(),
            &["matched".to_string()]
        );
    }
}

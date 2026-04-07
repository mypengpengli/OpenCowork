use opencowork_runtime::{compact_session, CompactConfig, PermissionMode, Session};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlashCommandSpec {
    pub name: &'static str,
    pub summary: &'static str,
    pub argument_hint: Option<&'static str>,
}

const SPECS: &[SlashCommandSpec] = &[
    SlashCommandSpec {
        name: "help",
        summary: "Show available slash commands",
        argument_hint: None,
    },
    SlashCommandSpec {
        name: "status",
        summary: "Show current session message count",
        argument_hint: None,
    },
    SlashCommandSpec {
        name: "compact",
        summary: "Compact older session messages into a summary prefix",
        argument_hint: None,
    },
    SlashCommandSpec {
        name: "permissions",
        summary: "Show or set the active permission mode",
        argument_hint: Some("[read-only|workspace-write|danger-full-access]"),
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    Help,
    Status,
    Compact,
    Permissions(Option<String>),
    Unknown(String),
}

impl SlashCommand {
    #[must_use]
    pub fn parse(input: &str) -> Option<Self> {
        if !input.trim_start().starts_with('/') {
            return None;
        }
        let trimmed = input.trim();
        let mut parts = trimmed.trim_start_matches('/').split_whitespace();
        let name = parts.next().unwrap_or_default();
        Some(match name {
            "help" => Self::Help,
            "status" => Self::Status,
            "compact" => Self::Compact,
            "permissions" => Self::Permissions(parts.next().map(ToOwned::to_owned)),
            other => Self::Unknown(other.to_string()),
        })
    }
}

#[must_use]
pub fn specs() -> &'static [SlashCommandSpec] {
    SPECS
}

#[must_use]
pub fn render_help() -> String {
    let mut lines = vec!["Slash commands".to_string()];
    for spec in SPECS {
        let name = match spec.argument_hint {
            Some(hint) => format!("/{} {}", spec.name, hint),
            None => format!("/{}", spec.name),
        };
        lines.push(format!("  {name:<48} {}", spec.summary));
    }
    lines.join("\n")
}

pub fn handle_command(
    command: &SlashCommand,
    session: &Session,
    permission_mode: PermissionMode,
) -> Option<String> {
    match command {
        SlashCommand::Help => Some(render_help()),
        SlashCommand::Status => Some(format!(
            "Messages          {}\nPermission mode   {:?}",
            session.messages.len(),
            permission_mode
        )),
        SlashCommand::Compact => {
            let result = compact_session(session, CompactConfig::default());
            Some(format!(
                "Compacted         {}\nSummary lines     {}",
                result.removed_message_count,
                result.summary.lines().count()
            ))
        }
        SlashCommand::Permissions(value) => Some(match value.as_deref() {
            None => format!("Permission mode   {:?}", permission_mode),
            Some(raw) => {
                let normalized = match raw {
                    "read-only" => Some(PermissionMode::ReadOnly),
                    "workspace-write" => Some(PermissionMode::WorkspaceWrite),
                    "danger-full-access" => Some(PermissionMode::DangerFullAccess),
                    _ => None,
                };
                match normalized {
                    Some(mode) => format!("Requested mode    {:?}", mode),
                    None => format!("Unsupported permission mode `{raw}`"),
                }
            }
        }),
        SlashCommand::Unknown(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{handle_command, render_help, SlashCommand};
    use opencowork_runtime::{PermissionMode, Session};

    #[test]
    fn parses_commands() {
        assert_eq!(SlashCommand::parse("/help"), Some(SlashCommand::Help));
        assert_eq!(
            SlashCommand::parse("/permissions read-only"),
            Some(SlashCommand::Permissions(Some("read-only".to_string())))
        );
    }

    #[test]
    fn renders_help_and_status() {
        assert!(render_help().contains("/compact"));
        let status = handle_command(
            &SlashCommand::Status,
            &Session::new(),
            PermissionMode::WorkspaceWrite,
        )
        .expect("status response");
        assert!(status.contains("Permission mode"));
    }
}

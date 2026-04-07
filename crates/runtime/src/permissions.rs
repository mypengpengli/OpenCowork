#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PermissionMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRequest {
    pub tool_name: String,
    pub input: String,
    pub required: PermissionMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    Deny { reason: String },
}

pub trait PermissionPrompter {
    fn decide(&mut self, request: &PermissionRequest) -> PermissionDecision;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionPolicy {
    active_mode: PermissionMode,
}

impl PermissionPolicy {
    #[must_use]
    pub fn new(active_mode: PermissionMode) -> Self {
        Self { active_mode }
    }

    #[must_use]
    pub fn active_mode(&self) -> PermissionMode {
        self.active_mode
    }

    pub fn authorize(
        &self,
        tool_name: &str,
        input: &str,
        required: PermissionMode,
        prompter: Option<&mut dyn PermissionPrompter>,
    ) -> PermissionDecision {
        if self.active_mode >= required {
            return PermissionDecision::Allow;
        }
        match prompter {
            Some(prompter) => prompter.decide(&PermissionRequest {
                tool_name: tool_name.to_string(),
                input: input.to_string(),
                required,
            }),
            None => PermissionDecision::Deny {
                reason: format!(
                    "tool `{tool_name}` requires {:?}, current mode is {:?}",
                    required, self.active_mode
                ),
            },
        }
    }
}

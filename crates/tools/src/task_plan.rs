use super::{ToolExposure, ToolSpec};
use opencowork_runtime::{PermissionMode, ToolError};
use serde_json::{json, Value};

pub(super) fn spec() -> ToolSpec {
    ToolSpec { name: "UpdatePlan", aliases: &[],
        description: "Persist visible steps for a multi-step task. Update progress as work proceeds. Completed steps require concrete verification evidence. After interruption read the supplied saved plan and resume unfinished steps.",
        input_schema: json!({"type":"object","properties":{"goal":{"type":"string"},"steps":{"type":"array","minItems":1,"maxItems":30,"items":{"type":"object","properties":{"title":{"type":"string"},"status":{"type":"string","enum":["pending","in_progress","completed","interrupted"]},"check":{"type":"string","description":"Completion criterion"},"evidence":{"type":"string","description":"Observed verification result; required when completed"}},"required":["title","status","check"]}}},"required":["goal","steps"]}),
        required_permission: PermissionMode::WorkspaceWrite, exposure: ToolExposure::Core }
}

pub(super) fn execute(input: &Value) -> Result<String, ToolError> {
    let id = std::env::var("OPENCOWORK_SESSION_ID")
        .map_err(|_| ToolError::new("A desktop session is required to persist a task plan"))?;
    if id.is_empty()
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(ToolError::new("Invalid session ID"));
    }
    validate(input)?;
    let root = opencowork_runtime::default_config_home().join("task-plans");
    std::fs::create_dir_all(&root).map_err(|e| ToolError::new(e.to_string()))?;
    let temporary = root.join(format!("{id}.{}.pending", std::process::id()));
    std::fs::write(&temporary, input.to_string())
        .and_then(|_| std::fs::rename(&temporary, root.join(format!("{id}.json"))))
        .map_err(|e| ToolError::new(e.to_string()))?;
    Ok(input.to_string())
}

fn validate(input: &Value) -> Result<(), ToolError> {
    let error = || {
        ToolError::new("Plan needs a goal, 1-30 steps with title/check/status, at most one active step, and evidence for completed steps (maximum 32 KB).")
    };
    if input.to_string().len() > 32_000 || input["goal"].as_str().unwrap_or("").trim().is_empty() {
        return Err(error());
    }
    let steps = input["steps"].as_array().ok_or_else(error)?;
    if steps.is_empty()
        || steps.len() > 30
        || steps
            .iter()
            .filter(|s| s["status"] == "in_progress")
            .count()
            > 1
    {
        return Err(error());
    }
    for step in steps {
        if step["title"].as_str().unwrap_or("").trim().is_empty()
            || step["check"].as_str().unwrap_or("").trim().is_empty()
            || !["pending", "in_progress", "completed", "interrupted"]
                .contains(&step["status"].as_str().unwrap_or(""))
            || (step["status"] == "completed"
                && step["evidence"].as_str().unwrap_or("").trim().is_empty())
        {
            return Err(error());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_requires_evidence() {
        let mut plan = json!({"goal":"ship","steps":[{"title":"test","status":"completed","check":"tests pass"}]});
        assert!(validate(&plan).is_err());
        plan["steps"][0]["evidence"] = json!("12 tests passed");
        assert!(validate(&plan).is_ok());
    }
}

use super::*;
use opencowork_runtime::process_sessions::ProcessStore;

pub(super) fn spec() -> ToolSpec {
    ToolSpec {
    name:"Process",aliases:&[],exposure:ToolExposure::Core,required_permission:PermissionMode::DangerFullAccess,
    description:"Manage persistent background commands and interactive terminals in desktop/web turns. start returns an ID immediately; use read with its byte cursor to await output (readTimedOut does not mean the command exited), inspect/list for state, input for terminal text, interrupt for Ctrl-C, resize, cancel, and remove for finished records. pty=true uses native Windows ConPTY; command runs in an interactive shell. Processes belong to this root turn/session; stopping the turn or host terminates their descendants. Logs are bounded at 16 MiB and reads at 64 KiB. Inspect unknown outcomes before restarting.",
    input_schema:json!({"type":"object","additionalProperties":false,"properties":{"action":{"enum":["start","list","inspect","read","input","interrupt","resize","cancel","remove"]},"id":{"type":"string"},"command":{"type":"string"},"pty":{"type":"boolean"},"text":{"type":"string"},"cursor":{"type":"integer","minimum":0},"maxBytes":{"type":"integer","minimum":16,"maximum":65536},"untilText":{"type":"string"},"waitMs":{"type":"integer","minimum":0,"maximum":30000},"timeoutMs":{"type":"integer","minimum":1000,"maximum":86400000},"rows":{"type":"integer"},"cols":{"type":"integer"}},"required":["action"]})
}
}
pub(super) fn execute(input: &Value) -> Result<String, ToolError> {
    run(input).map(|v| v.to_string()).map_err(ToolError::new)
}
fn run(input: &Value) -> Result<Value, String> {
    let home = opencowork_runtime::default_config_home();
    let store = ProcessStore::new(&home);
    let turn = std::env::var("OPENCOWORK_ROOT_TURN_ID")
        .map_err(|_| "Process sessions require an active desktop/web turn")?;
    let caller = std::env::var("OPENCOWORK_SESSION_ID").map_err(|_| "Missing session owner")?;
    let group = opencowork_agents::jobs::TaskStore::new(&home).group(&turn)?;
    let action = input["action"].as_str().ok_or("Missing action")?;
    if action == "start" {
        if group.cancelled {
            return Err("Parent task has stopped".into());
        }
        let record = store.create(
            &caller,
            &turn,
            &group.cwd,
            group.host,
            input["command"].as_str().ok_or("Missing command")?,
            input["pty"].as_bool().unwrap_or(false),
            input["timeoutMs"].as_u64().unwrap_or(900000),
        )?;
        return Ok(json!(record));
    }
    if action == "list" {
        return Ok(json!(store
            .list()
            .into_iter()
            .filter(|p| p.cwd == group.cwd)
            .collect::<Vec<_>>()));
    }
    let id = input["id"].as_str().ok_or("Missing process ID")?;
    let mut process = store.get(id)?;
    if process.cwd != group.cwd {
        return Err("Process belongs to a different workspace".into());
    }
    if matches!(action, "inspect" | "read") {
        if action == "inspect" {
            let deadline = opencowork_runtime::process_sessions::now_ms()
                + input["waitMs"].as_u64().unwrap_or(0).min(30000);
            while matches!(process.status.as_str(), "queued" | "running")
                && opencowork_runtime::process_sessions::now_ms() < deadline
            {
                std::thread::sleep(std::time::Duration::from_millis(40));
                process = store.get(id)?;
            }
        }
        if action == "read" {
            if let Some(expected) = input["untilText"].as_str() {
                if expected.len() > 512 {
                    return Err("Readiness marker exceeds 512 bytes".into());
                }
                let deadline = opencowork_runtime::process_sessions::now_ms()
                    + input["waitMs"].as_u64().unwrap_or(10000).min(30000);
                let mut cursor = input["cursor"].as_u64().unwrap_or(0);
                let mut output = String::new();
                loop {
                    let mut next = store.read(id, cursor, 16384, 100)?;
                    cursor = next["cursor"].as_u64().unwrap_or(cursor);
                    output.push_str(next["output"].as_str().unwrap_or(""));
                    let matched =
                        opencowork_runtime::process_sessions::output_contains(&output, expected);
                    if matched
                        || next["exited"] == true
                        || output.len() >= 65536
                        || opencowork_runtime::process_sessions::now_ms() >= deadline
                    {
                        next["output"] = output.into();
                        next["ready"] = matched.into();
                        return Ok(next);
                    }
                }
            }
        }
        return if action == "inspect" {
            Ok(json!(process))
        } else {
            store.read(
                id,
                input["cursor"].as_u64().unwrap_or(0),
                input["maxBytes"].as_u64().unwrap_or(16384) as usize,
                input["waitMs"].as_u64().unwrap_or(0),
            )
        };
    }
    if caller != process.session && caller != group.session {
        return Err("Only the owning session or root can control this process".into());
    }
    if action == "remove" {
        if matches!(process.status.as_str(), "queued" | "running") {
            return Err("Cancel the process before removing its record".into());
        }
        let root = home.join("process-sessions");
        for path in [
            root.join("records").join(format!("{id}.json")),
            root.join("logs").join(format!("{id}.log")),
        ] {
            if path.exists() {
                std::fs::remove_file(path).map_err(|e| e.to_string())?;
            }
        }
        return Ok(json!({"removed":true}));
    }
    if !matches!(action, "input" | "interrupt" | "resize" | "cancel") {
        return Err("Unknown process action".into());
    }
    if input["text"].as_str().is_some_and(|s| s.len() > 64000) {
        return Err("Input exceeds 64000 bytes".into());
    }
    if action == "interrupt" && !process.pty {
        return Err("Ctrl-C requires a PTY; use cancel to terminate a piped command".into());
    }
    store.submit(input)?;
    Ok(json!({"accepted":true,"id":id}))
}

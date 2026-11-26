use super::{cases, Observation};
use opencowork_tools::GlobalToolRegistry;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Step {
    pub action: Action,
    pub control: String,
    pub value: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    SetValue,
    Invoke,
    Select,
    Click,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Proposal {
    pub(super) choice_id: String,
    before: String,
    verify: String,
    #[serde(default)]
    site: Option<String>,
    steps: Vec<Step>,
    #[serde(default)]
    reply: Option<Reply>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reply {
    pub recipient: String,
    pub message: String,
}

impl Proposal {
    pub(super) fn is_reply(&self) -> bool {
        self.reply.is_some()
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub id: Uuid,
    pub version: u64,
    pub observation_id: Uuid,
    pub hwnd: u64,
    pub pid: u32,
    pub title: String,
    pub app: String,
    pub site: Option<String>,
    pub before: String,
    pub verify: String,
    pub steps: Vec<Step>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<Reply>,
}

impl Plan {
    pub(super) fn from_proposal(
        version: u64,
        observation: &Observation,
        proposal: Proposal,
    ) -> Option<Self> {
        let before = cases::limit(&proposal.before, 220);
        let verify = cases::limit(&proposal.verify, 220);
        let before_key = cases::normalized(&before);
        let valid_reply = proposal.reply.as_ref().is_some_and(|reply| {
            observation.app.eq_ignore_ascii_case("wxworklocal.exe")
                && proposal.site.is_none()
                && proposal.steps.is_empty()
                && reply.recipient == before
                && reply.message == verify
                && reply.recipient.chars().count() <= 90
                && reply.message.chars().count() <= 180
                && !reply.recipient.chars().any(char::is_control)
                && !reply.message.chars().any(char::is_control)
        });
        if observation.hwnd == 0
            || observation.pid == 0
            || observation.window.is_empty()
            || observation.app.is_empty()
            || before_key.chars().count() < if valid_reply { 2 } else { 3 }
            || !cases::normalized(&format!("{} {}", observation.window, observation.text))
                .contains(&before_key)
            || cases::normalized(&verify).chars().count() < 3
            || before_key == cases::normalized(&verify)
            || (proposal.reply.is_some() && !valid_reply)
            || (proposal.steps.is_empty() && !valid_reply)
            || proposal.steps.len() > 8
            || proposal.steps.iter().any(|step| {
                step.control.trim().is_empty()
                    || step.control.chars().count() > 240
                    || match step.action {
                        Action::SetValue => step
                            .value
                            .as_ref()
                            .is_none_or(|value| value.chars().count() > 1000),
                        _ => step.value.is_some(),
                    }
            })
        {
            return None;
        }
        let app = observation.app.clone();
        let site = proposal.site.and_then(|site| site_host(&site));
        if is_browser(&app) && site.is_none() {
            return None;
        }
        Some(Self {
            id: Uuid::new_v4(),
            version,
            observation_id: observation.id,
            hwnd: observation.hwnd,
            pid: observation.pid,
            title: observation.window.clone(),
            app,
            site,
            before,
            verify,
            steps: proposal.steps,
            reply: proposal.reply,
        })
    }

    pub fn same_recipe(&self, other: &Self) -> bool {
        self.app == other.app
            && self.title == other.title
            && self.site == other.site
            && self.before == other.before
            && self.verify == other.verify
            && self.steps == other.steps
            && self.reply == other.reply
    }
}

fn is_browser(app: &str) -> bool {
    matches!(
        app.to_ascii_lowercase().as_str(),
        "chrome.exe"
            | "msedge.exe"
            | "firefox.exe"
            | "brave.exe"
            | "opera.exe"
            | "vivaldi.exe"
            | "chromium.exe"
    )
}

fn site_host(value: &str) -> Option<String> {
    let value = value.trim();
    let url = if value.contains("://") {
        reqwest::Url::parse(value).ok()?
    } else {
        reqwest::Url::parse(&format!("https://{value}")).ok()?
    };
    (matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| {
        url.host_str().map(|host| {
            url.port()
                .map_or_else(|| host.to_owned(), |port| format!("{host}:{port}"))
        })
    })
    .flatten()
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    WaitingTarget,
    Running,
    Done,
    Stopped,
    Unverified,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub status: RunStatus,
    pub changed: usize,
    pub detail: String,
    #[serde(default)]
    pub started_ms: Option<u64>,
    #[serde(default)]
    pub ended_ms: Option<u64>,
}

impl Run {
    pub fn is_running(&self) -> bool {
        matches!(self.status, RunStatus::WaitingTarget | RunStatus::Running)
    }
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Once,
    Timed,
    Always,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub id: Uuid,
    pub case_id: Uuid,
    pub mode: Mode,
    pub expires_ms: Option<u64>,
    pub plan: Plan,
    #[serde(default)]
    pub from_global_setting: bool,
}

pub(super) fn run(
    plan: &Plan,
    check: impl Fn() -> Result<(), String>,
    progress: impl Fn(usize) -> Result<(), String>,
) -> Result<String, String> {
    let started = Instant::now();
    let stamp = loop {
        check()?;
        if let Some((hwnd, pid, stamp)) = foreground() {
            if hwnd == plan.hwnd && pid == plan.pid {
                thread::sleep(Duration::from_millis(450));
                if foreground() == Some((hwnd, pid, stamp)) {
                    break stamp;
                }
            }
        }
        if started.elapsed() >= Duration::from_secs(35) {
            return Err("Target window did not become active and idle".into());
        }
        thread::sleep(Duration::from_millis(200));
    };
    if let Some(reply) = &plan.reply {
        #[cfg(windows)]
        return super::reply::run(plan, reply, stamp, check, progress);
        #[cfg(not(windows))]
        {
            let _ = reply;
            return Err("Chat replies are available on Windows".into());
        }
    }
    let mut computer = GlobalToolRegistry::builtin();
    let mut idle = stamp;
    let check_target = |idle| -> Result<(), String> {
        check()?;
        if foreground() != Some((plan.hwnd, plan.pid, idle)) {
            return Err("User input or window focus changed; assistant released control".into());
        }
        Ok(())
    };
    check_target(idle)?;
    let list = call(&mut computer, &json!({"action":"list_windows"}))?;
    check_target(idle)?;
    let target = find_target(&list, plan, plan.hwnd, plan.pid)?;
    let state = |computer: &mut GlobalToolRegistry| observe_target(computer, plan, target);
    let mut current = state(&mut computer)?;
    check_target(idle)?;
    validate_site(plan, &current)?;
    if verified(&current, &plan.verify) {
        return Ok(format!("Already verified: {}", plan.verify));
    }
    if !visible(&current, &plan.before) {
        return Err("Current page no longer matches the approved starting evidence".into());
    }
    for (index, step) in plan.steps.iter().enumerate() {
        check_target(idle)?;
        validate_site(plan, &current)?;
        if verified(&current, &plan.verify) {
            return Ok(format!("Verified: {}", plan.verify));
        }
        let (action, secondary, advertised) = match step.action {
            Action::SetValue => ("set_value", None, "SetValue"),
            Action::Invoke => ("secondary_action", Some("Invoke"), "Invoke"),
            Action::Select => ("secondary_action", Some("Select"), "Select"),
            Action::Click => ("click", None, "Click"),
        };
        let mut elements = current["elements"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| {
                let supports = step.action == Action::Click
                    || (step.action == Action::SetValue
                        && item["role"].as_str() == Some("ControlType.Edit"))
                    || item["actions"].as_array().is_some_and(|actions| {
                        actions
                            .iter()
                            .any(|action| action.as_str().is_some_and(|name| name == advertised))
                    });
                supports
                    && item["name"].as_str().map(str::trim) == Some(step.control.trim())
                    && item["enabled"].as_bool() == Some(true)
                    && item["password"].as_bool() != Some(true)
            });
        let element = elements
            .next()
            .filter(|_| elements.next().is_none())
            .ok_or_else(|| {
                format!(
                    "Expected one enabled '{}' control with {}",
                    step.control, advertised
                )
            })?;
        let mut request = json!({
            "action":action, "window":target, "stateId":current["stateId"], "elementIndex":element["index"],
            "includeScreenshot":false, "includeText":true,
            "automationView":if is_browser(&plan.app) {"raw"} else {"control"},
            "requireForeground":true, "expectedInputStamp":idle,
        });
        if let Some(secondary) = secondary {
            request["secondaryAction"] = json!(secondary);
        }
        if let Some(value) = &step.value {
            request["text"] = json!(value);
            request["fallbackToTyping"] = json!(true);
        }
        check_target(idle)?;
        // Persist the uncertain boundary before dispatch; a failed or interrupted action is never replayed.
        progress(index + 1)?;
        current = call(&mut computer, &request)?;
        // Some browser accessibility providers implement even semantic actions with
        // synthetic input. Compare with the stamp taken immediately after dispatch.
        idle = current["lastInputStamp"]
            .as_u64()
            .and_then(|input| u32::try_from(input).ok())
            .ok_or("Could not verify desktop input after the step")?;
        check_target(idle)?;
        thread::sleep(Duration::from_millis(170));
    }
    for _ in 0..8 {
        check_target(idle)?;
        current = state(&mut computer)?;
        validate_site(plan, &current)?;
        if verified(&current, &plan.verify) {
            return Ok(format!("Verified: {}", plan.verify));
        }
        thread::sleep(Duration::from_millis(300));
    }
    Err("Action dispatched, but the expected result did not appear. Check the site before retrying.".into())
}

/// Read the receipt of a prior run, including after a restart with a new window handle.
pub(super) fn verify_receipt(plan: &Plan) -> Result<String, String> {
    let mut computer = GlobalToolRegistry::builtin();
    let until = Instant::now() + Duration::from_secs(25);
    loop {
        if let Some((hwnd, pid, _)) = foreground() {
            let list = call(&mut computer, &json!({"action":"list_windows"}))?;
            if let Ok(target) = find_target(&list, plan, hwnd, pid) {
                #[cfg(windows)]
                if let Some(reply) = &plan.reply {
                    return super::reply::verify(hwnd, pid, reply);
                }
                let current = observe_target(&mut computer, plan, target)?;
                if foreground().is_none_or(|(active, owner, _)| active != hwnd || owner != pid) {
                    return Err("The target window changed during receipt verification".into());
                }
                validate_site(plan, &current)?;
                return verified(&current, &plan.verify)
                    .then(|| format!("Verified: {}", plan.verify))
                    .ok_or_else(|| {
                        "Receipt not visible. Open the result for this request and verify again."
                            .into()
                    });
            }
        }
        if Instant::now() >= until {
            return Err("Open the approved target window to verify the receipt".into());
        }
        thread::sleep(Duration::from_millis(300));
    }
}

fn find_target<'a>(list: &'a Value, plan: &Plan, hwnd: u64, pid: u32) -> Result<&'a Value, String> {
    list["windows"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| {
            item["id"].as_u64() == Some(hwnd)
                && item["processId"].as_u64() == Some(u64::from(pid))
                && item["title"].as_str() == Some(plan.title.as_str())
                && item["app"].as_str().is_some_and(|app| {
                    app.rsplit(['\\', '/'])
                        .next()
                        .is_some_and(|tail| tail.eq_ignore_ascii_case(&plan.app))
                })
        })
        .ok_or_else(|| "Observed target window or application is no longer available".into())
}

fn observe_target(
    computer: &mut GlobalToolRegistry,
    plan: &Plan,
    target: &Value,
) -> Result<Value, String> {
    call(
        computer,
        &json!({
            "action":"get_window_state", "window":target, "includeScreenshot":false, "includeText":true,
            "automationView":if is_browser(&plan.app) {"raw"} else {"control"}
        }),
    )
}

fn call(computer: &mut GlobalToolRegistry, request: &Value) -> Result<Value, String> {
    let text = computer
        .execute("Computer", request)
        .map_err(|error| error.to_string())?;
    let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    if value["requiresObservation"].as_bool() == Some(true) {
        return Err("The target needs a fresh observation".into());
    }
    Ok(value)
}

fn visible(state: &Value, phrase: &str) -> bool {
    let phrase = cases::normalized(phrase);
    if phrase.is_empty() {
        return false;
    }
    let matches = |value: &Value| {
        value
            .as_str()
            .is_some_and(|text| cases::normalized(text).contains(&phrase))
    };
    matches(&state["window"]["title"])
        || matches(&state["text"])
        || matches(&state["documentText"])
        || state["elements"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|item| matches(&item["name"]) || matches(&item["value"]))
}

fn verified(state: &Value, phrase: &str) -> bool {
    let expected = cases::normalized(phrase);
    let exact_line = |value: &Value| {
        value
            .as_str()
            .is_some_and(|text| text.lines().any(|line| cases::normalized(line) == expected))
    };
    !expected.is_empty()
        && (exact_line(&state["text"])
            || exact_line(&state["documentText"])
            || state["elements"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|item| {
                    matches!(
                        item["role"].as_str(),
                        Some(
                            "ControlType.Text"
                                | "ControlType.Document"
                                | "ControlType.StatusBar"
                                | "ControlType.Pane"
                        )
                    ) && (exact_line(&item["name"])
                        || exact_line(&item["value"])
                        || exact_line(&item["documentText"]))
                }))
}
fn validate_site(plan: &Plan, state: &Value) -> Result<(), String> {
    if let Some(site) = &plan.site {
        let actual = state["elements"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|item| {
                let name = item["name"].as_str()?.to_lowercase();
                (name.contains("address and search")
                    || name.contains("search or enter address")
                    || name.contains("地址和搜索")
                    || name.contains("地址栏")
                    || name.contains("地址与搜索"))
                .then(|| item["value"].as_str().and_then(site_host))
                .flatten()
            });
        if actual.as_ref() != Some(site) {
            return Err(format!(
                "Could not verify the approved website {site} in the browser address bar"
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn foreground() -> Option<(u64, u32, u32)> {
    super::screen::foreground_input()
}

#[cfg(not(windows))]
fn foreground() -> Option<(u64, u32, u32)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_accepts_a_standalone_receipt_from_a_readable_control() {
        let state = json!({
            "window": {"title": "Approved A948"},
            "documentText": "Not Approved A948\nWaiting for confirmation",
            "elements": [{"role":"ControlType.Edit", "value":"Approved A948"}]
        });
        assert!(!verified(&state, "Approved A948"));
        let status = json!({
            "documentText": "Approval fixture\nApproved A948\n",
            "elements": [{"role":"ControlType.Text", "name":"Approved A948"}]
        });
        assert!(verified(&status, "Approved A948"));
        assert!(!verified(&status, "Approved A94"));
    }
}

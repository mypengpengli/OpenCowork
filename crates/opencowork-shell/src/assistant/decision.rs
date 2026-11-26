use reqwest::{redirect::Policy, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, time::Duration};

const GOOGLE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/decisions";

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Auth {
    GoogleKey,
    Bearer,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AppPolicy {
    Allow,
    TextOnly,
    LocalOnly,
    Ignore,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Settings {
    pub endpoint: String,
    pub model: String,
    pub api_key_env: String,
    pub auth: Auth,
    pub use_for_analysis: bool,
    pub analyze_automatically: bool,
    pub use_images: bool,
    pub show_notifications: bool,
    pub auto_run_ready: bool,
    pub auto_run_replies: bool,
    pub text_provider: String,
    pub vision_provider: String,
    pub skill_provider: String,
    pub app_rules: BTreeMap<String, AppPolicy>,
    pub daily_limit: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            endpoint: GOOGLE_URL.into(),
            model: "gemini-decision-1".into(),
            api_key_env: "GEMINI_API_KEY".into(),
            auth: Auth::GoogleKey,
            use_for_analysis: false,
            analyze_automatically: false,
            use_images: false,
            show_notifications: false,
            auto_run_ready: false,
            auto_run_replies: false,
            text_provider: String::new(),
            vision_provider: String::new(),
            skill_provider: String::new(),
            app_rules: BTreeMap::new(),
            daily_limit: 100,
        }
    }
}

impl Settings {
    pub fn app_policy(&self, app: &str) -> AppPolicy {
        self.app_rules
            .get(&app.to_ascii_lowercase())
            .copied()
            .unwrap_or(AppPolicy::Allow)
    }

    pub fn restricts_uploads(&self) -> bool {
        self.app_rules
            .values()
            .any(|policy| *policy != AppPolicy::Allow)
    }

    pub fn permits_text(&self, app: &str) -> bool {
        (!app.is_empty() || !self.restricts_uploads())
            && matches!(self.app_policy(app), AppPolicy::Allow | AppPolicy::TextOnly)
    }

    fn url(&self) -> Result<Url, String> {
        let endpoint = Url::parse(self.endpoint.trim())
            .map_err(|_| "Enter the full Decisions endpoint URL")?;
        if !(endpoint.scheme() == "https"
            || endpoint.scheme() == "http"
                && matches!(
                    endpoint.host_str(),
                    Some("localhost" | "127.0.0.1" | "[::1]")
                ))
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(
                "Use an HTTPS endpoint without credentials, query parameters or fragments".into(),
            );
        }
        Ok(endpoint)
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        self.url()?;
        if self.model.trim().is_empty() || self.model.len() > 128 {
            return Err("Enter a valid Decisions model name".into());
        }
        if self.api_key_env.len() > 128
            || !self
                .api_key_env
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return Err("Enter an environment variable name, not the key itself".into());
        }
        if self.daily_limit > 1000 {
            return Err("Daily prediction limit must be between 0 and 1000".into());
        }
        if self.app_rules.len() > 100
            || self.app_rules.keys().any(|name| {
                name.len() > 255
                    || name != &name.to_ascii_lowercase()
                    || name.contains(['/', '\\', '\n', '\r', '='])
                    || !name.ends_with(".exe")
                    || name.trim() != name
            })
        {
            return Err(
                "Use up to 100 lowercase executable names (such as wxworklocal.exe)".into(),
            );
        }
        Ok(())
    }

    fn key(&self, cwd: &Path, endpoint: &Url) -> Result<String, String> {
        let env = if self.api_key_env.is_empty() {
            let (_, profile) = super::super::provider_diagnostics::profile(cwd, false)?;
            let main_url = Url::parse(&profile.resolved_base_url())
                .map_err(|_| "Invalid primary Provider URL")?;
            if main_url.origin() != endpoint.origin() {
                return Err(
                    "For a different host, set a Decisions API key environment variable".into(),
                );
            }
            profile.api_key_env
        } else {
            self.api_key_env.clone()
        };
        std::env::var(&env)
            .ok()
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| format!("Missing API key in {env}"))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResultView {
    pub(super) choice: String,
    pub(super) confidence: f64,
    latency_ms: u64,
    pub(super) input_tokens: Option<u64>,
}

pub(super) fn probe(settings: &Settings, cwd: &Path) -> Result<ResultView, String> {
    choose(
        settings,
        cwd,
        "模拟申请：项目是二期，申请人说稍后补发目标账号的工号。当前没有工号。",
        BTreeMap::from([
            ("wait".into(), "等待申请人补发工号".into()),
            ("submit".into(), "马上提交授权，所需账号仍然未知".into()),
        ]),
    )
}

pub(super) fn choose(
    settings: &Settings,
    cwd: &Path,
    evidence: &str,
    criteria: BTreeMap<String, String>,
) -> Result<ResultView, String> {
    settings.validate()?;
    let endpoint = settings.url()?;
    let key = settings.key(cwd, &endpoint)?;
    let client = opencowork_api::provider_http_client_builder()
        .timeout(Duration::from_secs(15))
        .redirect(Policy::none())
        .build()
        .map_err(|_| "Could not initialize the Decisions HTTP client")?;
    let request = client.post(endpoint);
    let request = match settings.auth {
        Auth::GoogleKey => request.header("x-goog-api-key", key),
        Auth::Bearer => request.bearer_auth(key),
    };
    let started = std::time::Instant::now();
    let response = request.json(&json!({
        "model": settings.model.trim(),
        "input": evidence,
        "questions": {
            "next_action": {
                "type": "choice",
                "instructions": "Choose the next applicable action using only the supplied evidence.",
                "criteria": criteria
            }
        }
    })).send().map_err(|error| {
        if error.is_timeout() { "Decisions endpoint timed out" } else { "Could not reach the Decisions endpoint" }
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            404 | 405 | 501 => format!("HTTP {status}: check whether this relay exposes the native Decisions POST endpoint"),
            401 | 403 => format!("HTTP {status}: check the Decisions API key, authentication header and access"),
            _ => format!("Decisions endpoint returned HTTP {status}"),
        });
    }
    let value = response
        .json::<Value>()
        .map_err(|_| "The response was not Decisions API JSON")?;
    let (choice, confidence) = parse_choice(&value, &criteria)?;
    Ok(ResultView {
        choice,
        confidence,
        latency_ms: started.elapsed().as_millis() as u64,
        input_tokens: value
            .pointer("/usage/total_input_tokens")
            .and_then(Value::as_u64),
    })
}

fn parse_choice(
    value: &Value,
    criteria: &BTreeMap<String, String>,
) -> Result<(String, f64), String> {
    let answer = &value["answers"]["next_action"];
    let choice = answer["choice"]
        .as_str()
        .filter(|id| criteria.contains_key(*id))
        .ok_or("The endpoint did not return an option from the Decisions request")?;
    let confidence = answer["confidence"]
        .as_f64()
        .filter(|score| (0.0..=1.0).contains(score))
        .ok_or("The Decisions response did not contain a valid confidence")?;
    Ok((choice.to_owned(), confidence))
}

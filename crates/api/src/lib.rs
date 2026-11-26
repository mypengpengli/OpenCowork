mod openai_compat;
mod request_permit;

/// Create an HTTP client for configured providers. Windows desktop installs can use
/// the user's manually configured system proxy even when the process has no proxy variables.
pub fn provider_http_client_builder() -> reqwest::blocking::ClientBuilder {
    let builder = reqwest::blocking::Client::builder();
    #[cfg(windows)]
    if !["HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"]
        .iter()
        .any(|name| std::env::var(name).is_ok_and(|value| !value.trim().is_empty()))
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let settings = (|| -> Option<_> {
            let settings = RegKey::predef(HKEY_CURRENT_USER)
                .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
                .ok()?;
            if settings.get_value::<u32, _>("ProxyEnable").ok()? == 0 {
                return None;
            }
            let address: String = settings.get_value("ProxyServer").ok()?;
            let address = if address.contains('=') {
                address
                    .split(';')
                    .find_map(|item| item.trim().strip_prefix("https="))?
            } else {
                address.trim()
            };
            let bypass: String = settings.get_value("ProxyOverride").unwrap_or_default();
            let endpoint = if address.contains("://") {
                address.to_owned()
            } else {
                format!("http://{address}")
            };
            let endpoint = reqwest::Url::parse(&endpoint).ok()?;
            if !matches!(endpoint.scheme(), "http" | "https") {
                return None;
            }
            Some((endpoint, bypass))
        })();
        if let Some((endpoint, bypass)) = settings {
            let bypass = bypass
                .split(';')
                .map(str::trim)
                .map(str::to_ascii_lowercase)
                .collect::<Vec<_>>();
            let env_bypass = reqwest::NoProxy::from_env();
            return builder.proxy(
                reqwest::Proxy::custom(move |url| {
                    let host = url.host_str()?.to_ascii_lowercase();
                    let excluded = matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]")
                        || host
                            .parse::<std::net::IpAddr>()
                            .is_ok_and(|address| address.is_loopback())
                        || bypass.iter().any(|pattern| {
                            pattern == &host
                                || pattern == "<local>" && !host.contains('.')
                                || pattern
                                    .strip_prefix('*')
                                    .is_some_and(|suffix| host.ends_with(suffix))
                        });
                    (url.scheme() == "https" && !excluded).then(|| endpoint.clone())
                })
                .no_proxy(env_bypass),
            );
        }
    }
    builder
}

pub use openai_compat::{
    default_openai_profile_for_model, OpenAiCompatClient, OpenAiCompatProfile,
    DEFAULT_OPENAI_BASE_URL, DEFAULT_PROVIDER_MAX_RETRIES, DEFAULT_XAI_BASE_URL,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModelCapabilities {
    pub max_concurrent_requests: Option<usize>,
    pub context_window: Option<usize>,
    pub max_output_tokens: Option<usize>,
    pub supports_images: Option<bool>,
    pub requires_reasoning_content: bool,
}

impl ModelCapabilities {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_concurrent_requests == Some(0) {
            return Err("Provider concurrency must be positive".into());
        }
        if self.context_window == Some(0) || self.max_output_tokens == Some(0) {
            return Err("Model token limits must be positive".into());
        }
        if let (Some(window), Some(output)) = (self.context_window, self.max_output_tokens) {
            if output >= window {
                return Err("Output token limit must be smaller than the context window".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputMessage {
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub reasoning: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_urls: Vec<String>,
    pub role: String,
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<InputToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageRequest {
    pub model: String,
    pub system_prompt: Vec<String>,
    pub messages: Vec<InputMessage>,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StreamEvent {
    TextDelta(String),
    ToolUse {
        id: String,
        name: String,
        input: String,
    },
    MessageStop,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderRequest {
    #[serde(default)]
    pub max_output_tokens: Option<usize>,
    pub model: String,
    pub system_prompt: Vec<String>,
    pub messages: Vec<InputMessage>,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderEvent {
    ToolCallDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments_delta: String,
    },
    ReasoningDelta {
        text: String,
        field: String,
    },
    Finish {
        reason: String,
    },
    TextDelta(String),
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    Usage {
        input_tokens: u32,
        output_tokens: u32,
        cache_creation_input_tokens: u32,
        cache_read_input_tokens: u32,
    },
    MessageStop,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderResponse {
    pub events: Vec<ProviderEvent>,
}

pub trait ProviderClient {
    fn identity(&self) -> Option<String> {
        None
    }
    fn execute(&mut self, request: ProviderRequest) -> Result<ProviderResponse, String>;

    fn stream_execute(&mut self, request: ProviderRequest) -> Result<Vec<ProviderEvent>, String> {
        Ok(self.execute(request)?.events)
    }

    fn stream_execute_with(
        &mut self,
        request: ProviderRequest,
        on_event: &mut dyn FnMut(&ProviderEvent),
    ) -> Result<Vec<ProviderEvent>, String> {
        let events = self.stream_execute(request)?;
        for event in &events {
            on_event(event);
        }
        Ok(events)
    }
}

impl<T> ProviderClient for &mut T
where
    T: ProviderClient + ?Sized,
{
    fn identity(&self) -> Option<String> {
        (**self).identity()
    }
    fn execute(&mut self, request: ProviderRequest) -> Result<ProviderResponse, String> {
        (**self).execute(request)
    }

    fn stream_execute(&mut self, request: ProviderRequest) -> Result<Vec<ProviderEvent>, String> {
        (**self).stream_execute(request)
    }

    fn stream_execute_with(
        &mut self,
        request: ProviderRequest,
        on_event: &mut dyn FnMut(&ProviderEvent),
    ) -> Result<Vec<ProviderEvent>, String> {
        (**self).stream_execute_with(request, on_event)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReplayProviderClient {
    scripted_responses: VecDeque<ProviderResponse>,
}

impl ReplayProviderClient {
    #[must_use]
    pub fn new(scripted_responses: Vec<ProviderResponse>) -> Self {
        Self {
            scripted_responses: scripted_responses.into(),
        }
    }
}

impl ProviderClient for ReplayProviderClient {
    fn execute(&mut self, _request: ProviderRequest) -> Result<ProviderResponse, String> {
        self.scripted_responses
            .pop_front()
            .ok_or_else(|| "no scripted provider response remaining".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{ProviderClient, ProviderEvent, ProviderResponse, ReplayProviderClient};

    #[test]
    fn replays_provider_responses_in_order() {
        let mut client = ReplayProviderClient::new(vec![ProviderResponse {
            events: vec![ProviderEvent::TextDelta("hello".to_string())],
        }]);
        let response = client
            .execute(super::ProviderRequest {
                max_output_tokens: None,
                model: "demo".to_string(),
                system_prompt: Vec::new(),
                messages: Vec::new(),
                tools: Vec::new(),
            })
            .expect("provider response");
        assert_eq!(response.events.len(), 1);
    }
}

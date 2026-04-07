mod openai_compat;

pub use openai_compat::{
    default_openai_profile_for_model, OpenAiCompatClient, OpenAiCompatProfile,
    DEFAULT_OPENAI_BASE_URL, DEFAULT_XAI_BASE_URL,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;

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
    pub model: String,
    pub system_prompt: Vec<String>,
    pub messages: Vec<InputMessage>,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderEvent {
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
                model: "demo".to_string(),
                system_prompt: Vec::new(),
                messages: Vec::new(),
                tools: Vec::new(),
            })
            .expect("provider response");
        assert_eq!(response.events.len(), 1);
    }
}

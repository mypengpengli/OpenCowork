use reqwest::blocking::{Client, Response};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::time::Duration;

use crate::{
    InputMessage, InputToolCall, ProviderClient, ProviderEvent, ProviderRequest, ProviderResponse,
};

pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_XAI_BASE_URL: &str = "https://api.x.ai/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatProfile {
    pub provider_name: String,
    pub api_key_env: String,
    pub base_url_env: Option<String>,
    pub base_url: String,
    pub timeout_ms: u64,
}

impl OpenAiCompatProfile {
    #[must_use]
    pub fn openai() -> Self {
        Self {
            provider_name: "OpenAI".to_string(),
            api_key_env: "OPENAI_API_KEY".to_string(),
            base_url_env: Some("OPENAI_BASE_URL".to_string()),
            base_url: DEFAULT_OPENAI_BASE_URL.to_string(),
            timeout_ms: 90_000,
        }
    }

    #[must_use]
    pub fn xai() -> Self {
        Self {
            provider_name: "xAI".to_string(),
            api_key_env: "XAI_API_KEY".to_string(),
            base_url_env: Some("XAI_BASE_URL".to_string()),
            base_url: DEFAULT_XAI_BASE_URL.to_string(),
            timeout_ms: 90_000,
        }
    }

    #[must_use]
    pub fn custom(
        provider_name: impl Into<String>,
        api_key_env: impl Into<String>,
        base_url: impl Into<String>,
        base_url_env: Option<String>,
    ) -> Self {
        Self {
            provider_name: provider_name.into(),
            api_key_env: api_key_env.into(),
            base_url_env,
            base_url: base_url.into(),
            timeout_ms: 90_000,
        }
    }

    #[must_use]
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    #[must_use]
    pub fn resolved_base_url(&self) -> String {
        self.base_url_env
            .as_ref()
            .and_then(|name| std::env::var(name).ok())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| self.base_url.clone())
    }
}

#[must_use]
pub fn default_openai_profile_for_model(model: &str) -> OpenAiCompatProfile {
    let lower = model.trim().to_ascii_lowercase();
    if lower.starts_with("grok") || lower.starts_with("xai") {
        OpenAiCompatProfile::xai()
    } else {
        OpenAiCompatProfile::openai()
    }
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatClient {
    http: Client,
    profile: OpenAiCompatProfile,
    api_key: String,
}

impl OpenAiCompatClient {
    pub fn from_profile(profile: OpenAiCompatProfile) -> Result<Self, String> {
        let api_key =
            std::env::var(&profile.api_key_env).map_err(|_| missing_api_key_message(&profile))?;
        if api_key.trim().is_empty() {
            return Err(missing_api_key_message(&profile));
        }

        Ok(Self {
            http: Client::builder()
                .timeout(Duration::from_millis(profile.timeout_ms))
                .build()
                .map_err(|error| error.to_string())?,
            profile,
            api_key,
        })
    }

    fn request_url(&self) -> String {
        format!(
            "{}/chat/completions",
            self.profile.resolved_base_url().trim_end_matches('/')
        )
    }
}

impl ProviderClient for OpenAiCompatClient {
    fn execute(&mut self, request: ProviderRequest) -> Result<ProviderResponse, String> {
        let response = self
            .http
            .post(self.request_url())
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json")
            .json(&build_chat_completion_request(&request, false))
            .send()
            .map_err(|error| error.to_string())?;
        let payload = read_success_payload(response, &self.profile)?;
        normalize_chat_completion_payload(&payload)
    }

    fn stream_execute(&mut self, request: ProviderRequest) -> Result<Vec<ProviderEvent>, String> {
        self.stream_execute_with(request, &mut |_event| {})
    }

    fn stream_execute_with(
        &mut self,
        request: ProviderRequest,
        on_event: &mut dyn FnMut(&ProviderEvent),
    ) -> Result<Vec<ProviderEvent>, String> {
        let response = self
            .http
            .post(self.request_url())
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json")
            .json(&build_chat_completion_request(&request, true))
            .send()
            .map_err(|error| error.to_string())?;
        read_stream_events(response, &self.profile, on_event)
    }
}

fn missing_api_key_message(profile: &OpenAiCompatProfile) -> String {
    format!(
        "{} API key is missing. Set {}.",
        profile.provider_name, profile.api_key_env
    )
}

fn read_success_payload(
    response: Response,
    profile: &OpenAiCompatProfile,
) -> Result<Value, String> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(format!(
            "{} provider request failed with {}: {}",
            profile.provider_name,
            status,
            body.trim()
        ));
    }
    response.json::<Value>().map_err(|error| error.to_string())
}

fn build_chat_completion_request(request: &ProviderRequest, stream: bool) -> Value {
    let system_messages = request.system_prompt.iter().map(|content| {
        json!({
            "role": "system",
            "content": content,
        })
    });
    let conversation_messages = request.messages.iter().map(build_chat_completion_message);
    let mut root = json!({
        "model": request.model,
        "messages": system_messages.chain(conversation_messages).collect::<Vec<_>>(),
        "stream": stream,
    });

    if stream {
        root["stream_options"] = json!({
            "include_usage": true
        });
    }

    if !request.tools.is_empty() {
        root["tools"] = Value::Array(
            request
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.input_schema,
                        }
                    })
                })
                .collect(),
        );
        root["tool_choice"] = Value::String("auto".to_string());
    }

    root
}

fn build_chat_completion_message(message: &InputMessage) -> Value {
    let mut object = Map::new();
    object.insert("role".to_string(), Value::String(message.role.clone()));
    object.insert(
        "content".to_string(),
        message
            .content
            .as_ref()
            .map_or(Value::Null, |value| Value::String(value.clone())),
    );

    if message.role == "assistant" && !message.tool_calls.is_empty() {
        object.insert(
            "tool_calls".to_string(),
            Value::Array(
                message
                    .tool_calls
                    .iter()
                    .map(render_tool_call)
                    .collect::<Vec<_>>(),
            ),
        );
    }

    if message.role == "tool" {
        if let Some(tool_call_id) = &message.tool_call_id {
            object.insert(
                "tool_call_id".to_string(),
                Value::String(tool_call_id.clone()),
            );
        }
    }

    Value::Object(object)
}

fn render_tool_call(tool_call: &InputToolCall) -> Value {
    json!({
        "id": tool_call.id,
        "type": "function",
        "function": {
            "name": tool_call.name,
            "arguments": tool_call.input.to_string(),
        }
    })
}

fn normalize_chat_completion_payload(payload: &Value) -> Result<ProviderResponse, String> {
    let Some(message) = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
    else {
        return Err("provider response missing choices[0].message".to_string());
    };

    let mut events = Vec::new();
    for fragment in extract_text_fragments(message.get("content")) {
        events.push(ProviderEvent::TextDelta(fragment));
    }

    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for tool_call in tool_calls {
            let Some(id) = tool_call.get("id").and_then(Value::as_str) else {
                return Err("provider tool call missing id".to_string());
            };
            let Some(name) = tool_call
                .get("function")
                .and_then(Value::as_object)
                .and_then(|function| function.get("name"))
                .and_then(Value::as_str)
            else {
                return Err("provider tool call missing function.name".to_string());
            };
            let input = tool_call
                .get("function")
                .and_then(Value::as_object)
                .and_then(|function| function.get("arguments"))
                .and_then(Value::as_str)
                .map(parse_tool_arguments)
                .transpose()?
                .unwrap_or_else(|| json!({}));

            events.push(ProviderEvent::ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                input,
            });
        }
    }

    if let Some(usage) = payload.get("usage").and_then(Value::as_object) {
        events.push(ProviderEvent::Usage {
            input_tokens: parse_u32_field(usage, "prompt_tokens"),
            output_tokens: parse_u32_field(usage, "completion_tokens"),
            cache_creation_input_tokens: parse_cached_tokens(
                usage,
                "input_tokens_details",
                "cached_tokens",
            ),
            cache_read_input_tokens: 0,
        });
    }

    events.push(ProviderEvent::MessageStop);
    Ok(ProviderResponse { events })
}

#[derive(Debug, Default)]
struct StreamState {
    events: Vec<ProviderEvent>,
    tool_calls: BTreeMap<usize, StreamToolCallState>,
    usage: Option<ProviderEvent>,
}

#[derive(Debug, Default)]
struct StreamToolCallState {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

fn read_stream_events(
    response: Response,
    profile: &OpenAiCompatProfile,
    on_event: &mut dyn FnMut(&ProviderEvent),
) -> Result<Vec<ProviderEvent>, String> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(format!(
            "{} provider stream failed with {}: {}",
            profile.provider_name,
            status,
            body.trim()
        ));
    }

    let mut state = StreamState::default();
    parse_sse_stream(BufReader::new(response), &mut state, on_event)?;
    state.finish(on_event)
}

fn parse_sse_stream(
    reader: impl BufRead,
    state: &mut StreamState,
    on_event: &mut dyn FnMut(&ProviderEvent),
) -> Result<(), String> {
    let mut event_lines = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|error| error.to_string())?;
        if line.is_empty() {
            flush_sse_event(&mut event_lines, state, on_event)?;
            continue;
        }
        event_lines.push(line);
    }
    flush_sse_event(&mut event_lines, state, on_event)
}

fn flush_sse_event(
    lines: &mut Vec<String>,
    state: &mut StreamState,
    on_event: &mut dyn FnMut(&ProviderEvent),
) -> Result<(), String> {
    if lines.is_empty() {
        return Ok(());
    }

    let payload = lines
        .iter()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    lines.clear();

    if payload.is_empty() || payload == "[DONE]" {
        return Ok(());
    }

    let chunk: Value = serde_json::from_str(&payload)
        .map_err(|error| format!("invalid provider SSE chunk: {error}"))?;
    state.apply_chunk(&chunk, on_event)
}

impl StreamState {
    fn apply_chunk(
        &mut self,
        chunk: &Value,
        on_event: &mut dyn FnMut(&ProviderEvent),
    ) -> Result<(), String> {
        if let Some(usage) = chunk.get("usage").and_then(Value::as_object) {
            self.usage = Some(ProviderEvent::Usage {
                input_tokens: parse_u32_field(usage, "prompt_tokens"),
                output_tokens: parse_u32_field(usage, "completion_tokens"),
                cache_creation_input_tokens: parse_cached_tokens(
                    usage,
                    "input_tokens_details",
                    "cached_tokens",
                ),
                cache_read_input_tokens: 0,
            });
        }

        let Some(choices) = chunk.get("choices").and_then(Value::as_array) else {
            return Ok(());
        };

        for choice in choices {
            let Some(delta) = choice.get("delta").and_then(Value::as_object) else {
                continue;
            };

            if let Some(content) = delta.get("content") {
                for fragment in extract_text_fragments(Some(content)) {
                    let event = ProviderEvent::TextDelta(fragment);
                    on_event(&event);
                    self.events.push(event);
                }
            }

            if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for tool_call in tool_calls {
                    let index = tool_call
                        .get("index")
                        .and_then(Value::as_u64)
                        .and_then(|value| usize::try_from(value).ok())
                        .unwrap_or(0);
                    let state = self.tool_calls.entry(index).or_default();
                    if let Some(id) = tool_call.get("id").and_then(Value::as_str) {
                        state.id = Some(id.to_string());
                    }
                    if let Some(function) = tool_call.get("function").and_then(Value::as_object) {
                        if let Some(name) = function.get("name").and_then(Value::as_str) {
                            state.name = Some(name.to_string());
                        }
                        if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
                            state.arguments.push_str(arguments);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn finish(
        mut self,
        on_event: &mut dyn FnMut(&ProviderEvent),
    ) -> Result<Vec<ProviderEvent>, String> {
        for (_index, tool_call) in self.tool_calls {
            let id = tool_call
                .id
                .ok_or_else(|| "provider stream tool call missing id".to_string())?;
            let name = tool_call
                .name
                .ok_or_else(|| "provider stream tool call missing name".to_string())?;
            let input = parse_tool_arguments(&tool_call.arguments)?;
            let event = ProviderEvent::ToolCall { id, name, input };
            on_event(&event);
            self.events.push(event);
        }
        if let Some(usage) = self.usage {
            on_event(&usage);
            self.events.push(usage);
        }
        let stop = ProviderEvent::MessageStop;
        on_event(&stop);
        self.events.push(stop);
        Ok(self.events)
    }
}

fn parse_tool_arguments(value: &str) -> Result<Value, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(trimmed)
        .map_err(|error| format!("provider tool arguments are not valid JSON: {error}"))
}

fn extract_text_fragments(value: Option<&Value>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };

    match value {
        Value::Null => Vec::new(),
        Value::String(text) => (!text.is_empty())
            .then_some(text.clone())
            .into_iter()
            .collect(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| match item {
                Value::Object(object) => object
                    .get("text")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_u32_field(object: &Map<String, Value>, key: &str) -> u32 {
    object
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0)
}

fn parse_cached_tokens(object: &Map<String, Value>, container: &str, field: &str) -> u32 {
    object
        .get(container)
        .and_then(Value::as_object)
        .map_or(0, |details| parse_u32_field(details, field))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{InputMessage, InputToolCall, ProviderEvent, ProviderRequest, ToolDefinition};

    use super::{
        build_chat_completion_request, default_openai_profile_for_model,
        normalize_chat_completion_payload, OpenAiCompatProfile, DEFAULT_OPENAI_BASE_URL,
    };

    #[test]
    fn chooses_xai_profile_for_grok_models() {
        let profile = default_openai_profile_for_model("grok-3");
        assert_eq!(profile.provider_name, "xAI");
        assert_eq!(profile.api_key_env, "XAI_API_KEY");
    }

    #[test]
    fn serializes_tool_history_for_openai_compat_requests() {
        let payload = build_chat_completion_request(
            &ProviderRequest {
                model: "gpt-5.4-mini".to_string(),
                system_prompt: vec!["system".to_string()],
                messages: vec![
                    InputMessage {
                        role: "assistant".to_string(),
                        content: Some("Planning".to_string()),
                        tool_calls: vec![InputToolCall {
                            id: "call-1".to_string(),
                            name: "read_file".to_string(),
                            input: json!({"path": "README.md"}),
                        }],
                        tool_call_id: None,
                    },
                    InputMessage {
                        role: "tool".to_string(),
                        content: Some("{\"content\":\"ok\"}".to_string()),
                        tool_calls: Vec::new(),
                        tool_call_id: Some("call-1".to_string()),
                    },
                ],
                tools: vec![ToolDefinition {
                    name: "read_file".to_string(),
                    description: Some("Read a file".to_string()),
                    input_schema: json!({"type": "object"}),
                }],
            },
            false,
        );

        assert_eq!(payload["messages"][0]["role"], "system");
        assert_eq!(payload["messages"][1]["role"], "assistant");
        assert_eq!(
            payload["messages"][1]["tool_calls"][0]["function"]["name"],
            "read_file"
        );
        assert_eq!(payload["messages"][2]["tool_call_id"], "call-1");
    }

    #[test]
    fn normalizes_text_and_tool_calls_from_response() {
        let response = normalize_chat_completion_payload(&json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Need a tool.",
                    "tool_calls": [{
                        "id": "call-1",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\":\"README.md\"}"
                        }
                    }]
                }
            }],
            "usage": {
                "prompt_tokens": 12,
                "completion_tokens": 8,
                "input_tokens_details": {
                    "cached_tokens": 3
                }
            }
        }))
        .expect("normalized");

        assert!(matches!(
            &response.events[0],
            ProviderEvent::TextDelta(text) if text == "Need a tool."
        ));
        assert!(matches!(
            &response.events[1],
            ProviderEvent::ToolCall { name, .. } if name == "read_file"
        ));
        assert!(matches!(
            &response.events[2],
            ProviderEvent::Usage { input_tokens, output_tokens, cache_creation_input_tokens, .. }
                if *input_tokens == 12 && *output_tokens == 8 && *cache_creation_input_tokens == 3
        ));
    }

    #[test]
    fn resolves_default_openai_base_url_when_env_is_absent() {
        let profile = OpenAiCompatProfile::openai();
        assert_eq!(profile.base_url, DEFAULT_OPENAI_BASE_URL);
    }

    #[test]
    fn parses_streaming_sse_chunks_into_runtime_events() {
        let stream = [
            "data: {\"choices\":[{\"delta\":{\"content\":\"Need \"}}]}",
            "",
            "data: {\"choices\":[{\"delta\":{\"content\":\"tool\"}}]}",
            "",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\\\"README\"}}]}}]}",
            "",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\".md\\\"}\"}}]}}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":6}}",
            "",
            "data: [DONE]",
            "",
        ]
        .join("\n");
        let mut state = super::StreamState::default();
        super::parse_sse_stream(std::io::Cursor::new(stream), &mut state, &mut |_event| {})
            .expect("parse stream");
        let events = state.finish(&mut |_event| {}).expect("finish events");

        assert!(matches!(
            &events[0],
            ProviderEvent::TextDelta(text) if text == "Need "
        ));
        assert!(matches!(
            &events[1],
            ProviderEvent::TextDelta(text) if text == "tool"
        ));
        assert!(matches!(
            &events[2],
            ProviderEvent::ToolCall { name, input, .. }
                if name == "read_file" && input["path"] == "README.md"
        ));
        assert!(matches!(
            &events[3],
            ProviderEvent::Usage { input_tokens, output_tokens, .. }
                if *input_tokens == 10 && *output_tokens == 6
        ));
        assert!(matches!(events.last(), Some(ProviderEvent::MessageStop)));
    }
}

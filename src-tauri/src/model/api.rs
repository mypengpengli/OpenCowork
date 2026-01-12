use crate::storage::{ApiConfig, StorageManager};
use crate::commands::ChatHistoryMessage;
use chrono::Local;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

pub struct ApiClient {
    config: ApiConfig,
    client: Client,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Tool>>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Message {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<MessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

// Tool Use 相关结构体
#[derive(Serialize, Deserialize, Clone)]
pub struct Tool {
    #[serde(rename = "type")]
    tool_type: String,
    function: ToolFunction,
}

#[derive(Serialize, Deserialize, Clone)]
struct ToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct ContentPart {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<ImageUrl>,
}

#[derive(Serialize, Deserialize, Clone)]
struct ImageUrl {
    url: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<ToolCall>>,
}

/// Tool Use 对话结果
pub enum ChatWithToolsResult {
    /// AI 直接返回文本
    Text(String),
    /// AI 请求调用工具
    ToolCalls(Vec<ToolCall>),
}

impl ApiClient {
    pub fn new(config: &ApiConfig) -> Self {
        Self {
            config: config.clone(),
            client: Client::new(),
        }
    }

    pub async fn test_connection(&self) -> Result<(), String> {
        let url = format!("{}/models", self.config.endpoint);

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await
            .map_err(|e| {
                write_exchange_log("api-test", &url, "(none)", None, None, Some(&e.to_string()));
                format!("连接失败: {}", e)
            })?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        write_exchange_log("api-test", &url, "(none)", Some(status), Some(&text), None);

        if status.is_success() {
            Ok(())
        } else {
            Err(format!("API 返回错误 {}: {}", status, text))
        }
    }

    pub async fn chat(&self, system_prompt: &str, user_message: &str) -> Result<String, String> {
        let url = format!("{}/chat/completions", self.config.endpoint);

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: Some(MessageContent::Text(system_prompt.to_string())),
                    tool_calls: None,
                    tool_call_id: None,
                },
                Message {
                    role: "user".to_string(),
                    content: Some(MessageContent::Text(user_message.to_string())),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            max_tokens: 2048,
            tools: None,
        };

        let request_json = serde_json::to_string_pretty(&request)
            .unwrap_or_else(|e| format!("无法序列化请求: {}", e));

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                write_exchange_log("api-chat", &url, &request_json, None, None, Some(&e.to_string()));
                format!("请求失败: {}", e)
            })?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        write_exchange_log("api-chat", &url, &request_json, Some(status), Some(&text), None);

        if !status.is_success() {
            return Err(format!("API 错误 {}: {}", status, text));
        }

        let chat_response: ChatResponse = serde_json::from_str(&text)
            .map_err(|e| format!("解析响应失败: {}", e))?;

        chat_response
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .ok_or_else(|| "没有返回内容".to_string())
    }



    pub async fn chat_with_history(
        &self,
        system_prompt: &str,
        user_message: &str,
        history: Option<Vec<ChatHistoryMessage>>,
    ) -> Result<String, String> {
        let url = format!("{}/chat/completions", self.config.endpoint);

        let mut messages = vec![Message {
            role: "system".to_string(),
            content: Some(MessageContent::Text(system_prompt.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        // Add conversation history if provided
        if let Some(hist) = history {
            for msg in hist {
                messages.push(Message {
                    role: msg.role,
                    content: Some(MessageContent::Text(msg.content)),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
        }

        // Add current user message
        messages.push(Message {
            role: "user".to_string(),
            content: Some(MessageContent::Text(user_message.to_string())),
            tool_calls: None,
            tool_call_id: None,
        });

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages,
            max_tokens: 2048,
            tools: None,
        };

        let request_json = serde_json::to_string_pretty(&request)
            .unwrap_or_else(|e| format!("无法序列化请求: {}", e));

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                write_exchange_log("api-chat-history", &url, &request_json, None, None, Some(&e.to_string()));
                format!("请求失败: {}", e)
            })?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        write_exchange_log("api-chat-history", &url, &request_json, Some(status), Some(&text), None);

        if !status.is_success() {
            return Err(format!("API 错误 {}: {}", status, text));
        }

        let chat_response: ChatResponse = serde_json::from_str(&text)
            .map_err(|e| format!("解析响应失败: {}", e))?;

        chat_response
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .ok_or_else(|| "没有返回内容".to_string())
    }
    pub async fn analyze_image(&self, image_base64: &str, prompt: &str) -> Result<String, String> {
        let url = format!("{}/chat/completions", self.config.endpoint);

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: Some(MessageContent::Parts(vec![
                    ContentPart {
                        content_type: "text".to_string(),
                        text: Some(prompt.to_string()),
                        image_url: None,
                    },
                    ContentPart {
                        content_type: "image_url".to_string(),
                        text: None,
                        image_url: Some(ImageUrl {
                            url: format!("data:image/jpeg;base64,{}", image_base64),
                        }),
                    },
                ])),
                tool_calls: None,
                tool_call_id: None,
            }],
            max_tokens: 10000,
            tools: None,
        };

        let request_json = serde_json::to_string_pretty(&request)
            .unwrap_or_else(|e| format!("无法序列化请求: {}", e));

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                write_exchange_log("api-image", &url, &request_json, None, None, Some(&e.to_string()));
                format!("请求失败: {}", e)
            })?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        write_exchange_log("api-image", &url, &request_json, Some(status), Some(&text), None);

        if !status.is_success() {
            return Err(format!("API 错误 {}: {}", status, text));
        }

        let chat_response: ChatResponse = serde_json::from_str(&text)
            .map_err(|e| format!("解析响应失败: {}", e))?;

        chat_response
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .ok_or_else(|| "没有返回内容".to_string())
    }
    pub async fn test_connection_with_fallback(&self) -> Result<(), String> {
        if self.test_connection().await.is_ok() {
            return Ok(());
        }

        // Some providers block /models; fall back to a minimal chat request.
        self.test_chat_connection().await
    }

    async fn test_chat_connection(&self) -> Result<(), String> {
        let url = format!("{}/chat/completions", self.config.endpoint);

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: Some(MessageContent::Text("ping".to_string())),
                tool_calls: None,
                tool_call_id: None,
            }],
            max_tokens: 1,
            tools: None,
        };

        let request_json = serde_json::to_string_pretty(&request)
            .unwrap_or_else(|e| format!("Unable to serialize request: {}", e));

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                write_exchange_log("api-test-chat", &url, &request_json, None, None, Some(&e.to_string()));
                format!("Request failed: {}", e)
            })?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        write_exchange_log("api-test-chat", &url, &request_json, Some(status), Some(&text), None);

        if status.is_success() {
            Ok(())
        } else {
            Err(format!("API error {}: {}", status, text))
        }
    }

    /// 创建 invoke_skill 工具定义
    pub fn create_skill_tool(skills: &[crate::skills::SkillMetadata]) -> Vec<Tool> {
        if skills.is_empty() {
            return vec![];
        }

        // 构建 skills 的 enum 描述
        let skill_names: Vec<String> = skills
            .iter()
            .filter(|s| s.user_invocable.unwrap_or(true))
            .map(|s| s.name.clone())
            .collect();

        if skill_names.is_empty() {
            return vec![];
        }

        let skill_descriptions: Vec<String> = skills
            .iter()
            .filter(|s| s.user_invocable.unwrap_or(true))
            .map(|s| format!("- {}: {}", s.name, s.description))
            .collect();

        vec![Tool {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "invoke_skill".to_string(),
                description: format!(
                    "调用一个技能来完成特定任务。可用的技能有：\n{}",
                    skill_descriptions.join("\n")
                ),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "skill_name": {
                            "type": "string",
                            "enum": skill_names,
                            "description": "要调用的技能名称"
                        },
                        "args": {
                            "type": "string",
                            "description": "传递给技能的参数（可选）"
                        }
                    },
                    "required": ["skill_name"]
                }),
            },
        }]
    }

    /// 带 Tool Use 的对话
    pub async fn chat_with_tools(
        &self,
        system_prompt: &str,
        user_message: &str,
        history: Option<Vec<ChatHistoryMessage>>,
        tools: Vec<Tool>,
    ) -> Result<ChatWithToolsResult, String> {
        let url = format!("{}/chat/completions", self.config.endpoint);

        let mut messages = vec![Message {
            role: "system".to_string(),
            content: Some(MessageContent::Text(system_prompt.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        // Add conversation history if provided
        if let Some(hist) = history {
            for msg in hist {
                messages.push(Message {
                    role: msg.role,
                    content: Some(MessageContent::Text(msg.content)),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
        }

        // Add current user message
        messages.push(Message {
            role: "user".to_string(),
            content: Some(MessageContent::Text(user_message.to_string())),
            tool_calls: None,
            tool_call_id: None,
        });

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages,
            max_tokens: 2048,
            tools: if tools.is_empty() { None } else { Some(tools) },
        };

        let request_json = serde_json::to_string_pretty(&request)
            .unwrap_or_else(|e| format!("无法序列化请求: {}", e));

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                write_exchange_log("api-chat-tools", &url, &request_json, None, None, Some(&e.to_string()));
                format!("请求失败: {}", e)
            })?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        write_exchange_log("api-chat-tools", &url, &request_json, Some(status), Some(&text), None);

        if !status.is_success() {
            return Err(format!("API 错误 {}: {}", status, text));
        }

        let chat_response: ChatResponse = serde_json::from_str(&text)
            .map_err(|e| format!("解析响应失败: {}", e))?;

        let choice = chat_response
            .choices
            .first()
            .ok_or_else(|| "没有返回内容".to_string())?;

        // 检查是否有 tool_calls
        if let Some(ref tool_calls) = choice.message.tool_calls {
            if !tool_calls.is_empty() {
                return Ok(ChatWithToolsResult::ToolCalls(tool_calls.clone()));
            }
        }

        // 否则返回文本内容
        let content = choice
            .message
            .content
            .clone()
            .ok_or_else(|| "没有返回内容".to_string())?;

        Ok(ChatWithToolsResult::Text(content))
    }

    /// 继续带 tool 结果的对话
    pub async fn continue_with_tool_result(
        &self,
        system_prompt: &str,
        messages_so_far: Vec<Message>,
        tool_call_id: &str,
        tool_result: &str,
        tools: Vec<Tool>,
    ) -> Result<ChatWithToolsResult, String> {
        let url = format!("{}/chat/completions", self.config.endpoint);

        let mut messages = vec![Message {
            role: "system".to_string(),
            content: Some(MessageContent::Text(system_prompt.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        // 添加之前的消息
        messages.extend(messages_so_far);

        // 添加 tool 结果
        messages.push(Message {
            role: "tool".to_string(),
            content: Some(MessageContent::Text(tool_result.to_string())),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
        });

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages,
            max_tokens: 2048,
            tools: if tools.is_empty() { None } else { Some(tools) },
        };

        let request_json = serde_json::to_string_pretty(&request)
            .unwrap_or_else(|e| format!("无法序列化请求: {}", e));

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                write_exchange_log("api-chat-tool-result", &url, &request_json, None, None, Some(&e.to_string()));
                format!("请求失败: {}", e)
            })?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        write_exchange_log("api-chat-tool-result", &url, &request_json, Some(status), Some(&text), None);

        if !status.is_success() {
            return Err(format!("API 错误 {}: {}", status, text));
        }

        let chat_response: ChatResponse = serde_json::from_str(&text)
            .map_err(|e| format!("解析响应失败: {}", e))?;

        let choice = chat_response
            .choices
            .first()
            .ok_or_else(|| "没有返回内容".to_string())?;

        // 检查是否有更多 tool_calls
        if let Some(ref tool_calls) = choice.message.tool_calls {
            if !tool_calls.is_empty() {
                return Ok(ChatWithToolsResult::ToolCalls(tool_calls.clone()));
            }
        }

        // 否则返回文本内容
        let content = choice
            .message
            .content
            .clone()
            .ok_or_else(|| "没有返回内容".to_string())?;

        Ok(ChatWithToolsResult::Text(content))
    }
}

fn write_exchange_log(
    prefix: &str,
    url: &str,
    request_body: &str,
    status: Option<StatusCode>,
    response_body: Option<&str>,
    error: Option<&str>,
) {
    let mut log = String::new();
    log.push_str(&format!("time: {}\n", Local::now().to_rfc3339()));
    log.push_str(&format!("url: {}\n", url));
    log.push_str("request:\n");
    log.push_str(request_body);
    log.push('\n');

    if let Some(status) = status {
        log.push_str(&format!("\nstatus: {}\n", status));
    }
    if let Some(body) = response_body {
        log.push_str("\nresponse:\n");
        log.push_str(body);
        log.push('\n');
    }
    if let Some(err) = error {
        log.push_str("\nerror:\n");
        log.push_str(err);
        log.push('\n');
    }

    if let Err(err) = StorageManager::new().write_log_snapshot(prefix, &log) {
        eprintln!("写入日志失败: {}", err);
    }
}

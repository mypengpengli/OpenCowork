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
pub struct Message {
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
pub enum MessageContent {
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
pub(crate) struct ContentPart {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<ImageUrl>,
}

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct ImageUrl {
    url: String,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    param: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Option<Vec<Choice>>,
    #[serde(default)]
    error: Option<ApiError>,
}

impl ChatResponse {
    fn first_choice(&self) -> Result<&Choice, String> {
        self.choices
            .as_ref()
            .and_then(|choices| choices.first())
            .ok_or_else(|| "API 响应缺少 choices".to_string())
    }
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
    ToolCalls {
        calls: Vec<ToolCall>,
        messages: Vec<Message>,
    },
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

        let chat_response = Self::parse_chat_response(&text)?;
        let choice = chat_response.first_choice()?;
        choice
            .message
            .content
            .clone()
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

        let chat_response = Self::parse_chat_response(&text)?;
        let choice = chat_response.first_choice()?;
        choice
            .message
            .content
            .clone()
            .ok_or_else(|| "没有返回内容".to_string())
    }

    pub async fn chat_with_history_with_images(
        &self,
        system_prompt: &str,
        user_message: &str,
        history: Option<Vec<ChatHistoryMessage>>,
        image_urls: &[String],
    ) -> Result<String, String> {
        let url = format!("{}/chat/completions", self.config.endpoint);

        let mut messages = vec![Message {
            role: "system".to_string(),
            content: Some(MessageContent::Text(system_prompt.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

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

        let user_content = Self::build_user_message_content(user_message, image_urls);
        messages.push(Message {
            role: "user".to_string(),
            content: Some(user_content),
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

        let chat_response = Self::parse_chat_response(&text)?;
        let choice = chat_response.first_choice()?;
        choice
            .message
            .content
            .clone()
            .ok_or_else(|| "没有返回内容".to_string())
    }

    fn build_user_message_content(user_message: &str, image_urls: &[String]) -> MessageContent {
        if image_urls.is_empty() {
            return MessageContent::Text(user_message.to_string());
        }

        let mut parts = Vec::new();
        parts.push(ContentPart {
            content_type: "text".to_string(),
            text: Some(user_message.to_string()),
            image_url: None,
        });

        for url in image_urls {
            parts.push(ContentPart {
                content_type: "image_url".to_string(),
                text: None,
                image_url: Some(ImageUrl { url: url.clone() }),
            });
        }

        MessageContent::Parts(parts)
    }

    fn format_api_error(error: &ApiError) -> String {
        let mut details = Vec::new();
        if let Some(code) = &error.code {
            if !code.is_empty() {
                details.push(format!("code={}", code));
            }
        }
        if let Some(kind) = &error.r#type {
            if !kind.is_empty() {
                details.push(format!("type={}", kind));
            }
        }
        if let Some(param) = &error.param {
            if !param.is_empty() {
                details.push(format!("param={}", param));
            }
        }
        if details.is_empty() {
            format!("API 错误: {}", error.message)
        } else {
            format!("API 错误: {} ({})", error.message, details.join(", "))
        }
    }

    fn parse_chat_response(text: &str) -> Result<ChatResponse, String> {
        let chat_response: ChatResponse = serde_json::from_str(text)
            .map_err(|e| format!("解析响应失败: {}", e))?;
        if let Some(error) = &chat_response.error {
            return Err(Self::format_api_error(error));
        }
        Ok(chat_response)
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

        let chat_response = Self::parse_chat_response(&text)?;
        let choice = chat_response.first_choice()?;
        choice
            .message
            .content
            .clone()
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

    /// 创建技能相关工具定义（invoke_skill + manage_skill）
    pub fn create_skill_tools(skills: &[crate::skills::SkillMetadata]) -> Vec<Tool> {
        let mut tools = Vec::new();

        // File and command tools
        tools.push(Tool {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "Read".to_string(),
                description: "Read a text file from disk.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to read" },
                        "max_bytes": { "type": "integer", "description": "Optional max bytes to read" }
                    },
                    "required": ["path"]
                }),
            },
        });

        tools.push(Tool {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "Write".to_string(),
                description: "Write text content to a file.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to write" },
                        "content": { "type": "string", "description": "Content to write" },
                        "append": { "type": "boolean", "description": "Append instead of overwrite" }
                    },
                    "required": ["path", "content"]
                }),
            },
        });

        tools.push(Tool {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "Edit".to_string(),
                description: "Replace text in a file.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to edit" },
                        "old": { "type": "string", "description": "Text to replace" },
                        "new": { "type": "string", "description": "Replacement text" },
                        "replace_all": { "type": "boolean", "description": "Replace all occurrences (default true)" }
                    },
                    "required": ["path", "old", "new"]
                }),
            },
        });

        tools.push(Tool {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "Update".to_string(),
                description: "Alias for Edit.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to edit" },
                        "old": { "type": "string", "description": "Text to replace" },
                        "new": { "type": "string", "description": "Replacement text" },
                        "replace_all": { "type": "boolean", "description": "Replace all occurrences (default true)" }
                    },
                    "required": ["path", "old", "new"]
                }),
            },
        });

        tools.push(Tool {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "Glob".to_string(),
                description: "List files matching a glob pattern.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Glob pattern" },
                        "max_results": { "type": "integer", "description": "Optional max results" }
                    },
                    "required": ["pattern"]
                }),
            },
        });

        tools.push(Tool {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "Grep".to_string(),
                description: "Search for text in files.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Search pattern" },
                        "path": { "type": "string", "description": "File or directory to search" },
                        "glob": { "type": "string", "description": "Optional glob filter (e.g. **/*.txt)" },
                        "regex": { "type": "boolean", "description": "Treat pattern as regex" },
                        "case_sensitive": { "type": "boolean", "description": "Case-sensitive search" },
                        "max_results": { "type": "integer", "description": "Optional max results" }
                    },
                    "required": ["pattern"]
                }),
            },
        });

        tools.push(Tool {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "Bash".to_string(),
                description: "Run a shell command. On Windows this uses cmd /C; prefer dir or powershell -Command.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Command to run" },
                        "cwd": { "type": "string", "description": "Working directory" },
                        "timeout_ms": { "type": "integer", "description": "Timeout in milliseconds" }
                    },
                    "required": ["command"]
                }),
            },
        });
        tools.push(Tool {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "run_command".to_string(),
                description: "Alias for Bash (same behavior).".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Command to run" },
                        "cwd": { "type": "string", "description": "Working directory" },
                        "timeout_ms": { "type": "integer", "description": "Timeout in milliseconds" }
                    },
                    "required": ["command"]
                }),
            },
        });

        // 1. manage_skill 工具 - 始终可用，用于创建/更新/删除技能
        tools.push(Tool {
            tool_type: "function".to_string(),
            function: ToolFunction {
                name: "manage_skill".to_string(),
                description: "管理技能：创建新技能、更新现有技能或删除技能。当用户想要创建、修改或删除技能时使用此工具。".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["create", "update", "delete"],
                            "description": "操作类型：create=创建新技能，update=更新现有技能，delete=删除技能"
                        },
                        "name": {
                            "type": "string",
                            "description": "技能名称，只能包含小写字母、数字和连字符，1-64字符，不能以连字符开头或结尾"
                        },
                        "description": {
                            "type": "string",
                            "description": "技能描述，说明这个技能做什么、什么时候使用（create/update 时必填）"
                        },
                        "instructions": {
                            "type": "string",
                            "description": "技能的详细指令，Markdown 格式（create/update 时必填）"
                        },
                        "allowed_tools": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "??????????????????? Read, Grep?"
                        },
                        "model": {
                            "type": "string",
                            "description": "??????????????"
                        },
                        "context": {
                            "type": "string",
                            "description": "???????????screen ? none?"
                        },
                        "user_invocable": {
                            "type": "boolean",
                            "description": "????????????? /skill ??"
                        },
                        "metadata": {
                            "type": "object",
                            "additionalProperties": { "type": "string" },
                            "description": "?????????????"
                        },
                    },
                    "required": ["action", "name"]
                }),
            },
        });

        // 2. invoke_skill 工具 - 仅当有可用技能时添加
        if !skills.is_empty() {
            let skill_names: Vec<String> = skills
                .iter()
                .filter(|s| s.user_invocable.unwrap_or(true))
                .map(|s| s.name.clone())
                .collect();

            if !skill_names.is_empty() {
                let skill_descriptions: Vec<String> = skills
                    .iter()
                    .filter(|s| s.user_invocable.unwrap_or(true))
                    .map(|s| format!("- {}: {}", s.name, s.description))
                    .collect();

                tools.push(Tool {
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
                });
            }
        }

        tools
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
        let mut messages_for_return = Vec::new();

        // Add conversation history if provided
        if let Some(hist) = history {
            for msg in hist {
                let message = Message {
                    role: msg.role,
                    content: Some(MessageContent::Text(msg.content)),
                    tool_calls: None,
                    tool_call_id: None,
                };
                messages.push(message.clone());
                messages_for_return.push(message);
            }
        }

        // Add current user message
        let user_message = Message {
            role: "user".to_string(),
            content: Some(MessageContent::Text(user_message.to_string())),
            tool_calls: None,
            tool_call_id: None,
        };
        messages.push(user_message.clone());
        messages_for_return.push(user_message);

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

        let chat_response = Self::parse_chat_response(&text)?;
        let choice = chat_response.first_choice()?;

        // 检查是否有 tool_calls
        if let Some(ref tool_calls) = choice.message.tool_calls {
            if !tool_calls.is_empty() {
                let assistant_message = Message {
                    role: "assistant".to_string(),
                    content: choice.message.content.clone().map(MessageContent::Text),
                    tool_calls: Some(tool_calls.clone()),
                    tool_call_id: None,
                };
                messages_for_return.push(assistant_message);
                return Ok(ChatWithToolsResult::ToolCalls {
                    calls: tool_calls.clone(),
                    messages: messages_for_return,
                });
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

    /// 带 Tool Use 的对话（包含图片附件）
    pub async fn chat_with_tools_with_images(
        &self,
        system_prompt: &str,
        user_message: &str,
        history: Option<Vec<ChatHistoryMessage>>,
        tools: Vec<Tool>,
        image_urls: &[String],
    ) -> Result<ChatWithToolsResult, String> {
        let url = format!("{}/chat/completions", self.config.endpoint);

        let mut messages = vec![Message {
            role: "system".to_string(),
            content: Some(MessageContent::Text(system_prompt.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];
        let mut messages_for_return = Vec::new();

        if let Some(hist) = history {
            for msg in hist {
                let message = Message {
                    role: msg.role,
                    content: Some(MessageContent::Text(msg.content)),
                    tool_calls: None,
                    tool_call_id: None,
                };
                messages.push(message.clone());
                messages_for_return.push(message);
            }
        }

        let user_content = Self::build_user_message_content(user_message, image_urls);
        let user_message = Message {
            role: "user".to_string(),
            content: Some(user_content),
            tool_calls: None,
            tool_call_id: None,
        };
        messages.push(user_message.clone());
        messages_for_return.push(user_message);

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

        let chat_response = Self::parse_chat_response(&text)?;
        let choice = chat_response.first_choice()?;

        if let Some(ref tool_calls) = choice.message.tool_calls {
            if !tool_calls.is_empty() {
                let assistant_message = Message {
                    role: "assistant".to_string(),
                    content: choice.message.content.clone().map(MessageContent::Text),
                    tool_calls: Some(tool_calls.clone()),
                    tool_call_id: None,
                };
                messages_for_return.push(assistant_message);
                return Ok(ChatWithToolsResult::ToolCalls {
                    calls: tool_calls.clone(),
                    messages: messages_for_return,
                });
            }
        }

        let content = choice
            .message
            .content
            .clone()
            .ok_or_else(|| "没有返回内容".to_string())?;

        Ok(ChatWithToolsResult::Text(content))
    }

    /// 继续带 tool 结果的对话
    pub async fn continue_with_tool_results(
        &self,
        system_prompt: &str,
        messages_so_far: Vec<Message>,
        tool_results: Vec<(String, String)>,
        tools: Vec<Tool>,
    ) -> Result<ChatWithToolsResult, String> {
        let url = format!("{}/chat/completions", self.config.endpoint);

        let mut messages = vec![Message {
            role: "system".to_string(),
            content: Some(MessageContent::Text(system_prompt.to_string())),
            tool_calls: None,
            tool_call_id: None,
        }];

        let mut messages_for_return = messages_so_far;

        // 添加之前的消息
        messages.extend(messages_for_return.iter().cloned());

        // 添加 tool 结果
        for (tool_call_id, tool_result) in tool_results {
            let tool_message = Message {
                role: "tool".to_string(),
                content: Some(MessageContent::Text(tool_result)),
                tool_calls: None,
                tool_call_id: Some(tool_call_id),
            };
            messages.push(tool_message.clone());
            messages_for_return.push(tool_message);
        }

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

        let chat_response = Self::parse_chat_response(&text)?;
        let choice = chat_response.first_choice()?;

        // 检查是否有更多 tool_calls
        if let Some(ref tool_calls) = choice.message.tool_calls {
            if !tool_calls.is_empty() {
                let assistant_message = Message {
                    role: "assistant".to_string(),
                    content: choice.message.content.clone().map(MessageContent::Text),
                    tool_calls: Some(tool_calls.clone()),
                    tool_call_id: None,
                };
                messages_for_return.push(assistant_message);
                return Ok(ChatWithToolsResult::ToolCalls {
                    calls: tool_calls.clone(),
                    messages: messages_for_return,
                });
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
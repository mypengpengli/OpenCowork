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
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: MessageContent,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Serialize, Deserialize)]
struct ContentPart {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<ImageUrl>,
}

#[derive(Serialize, Deserialize)]
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
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
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
                    content: MessageContent::Text(system_prompt.to_string()),
                },
                Message {
                    role: "user".to_string(),
                    content: MessageContent::Text(user_message.to_string()),
                },
            ],
            max_tokens: 2048,
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
            .map(|c| c.message.content.clone())
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
            content: MessageContent::Text(system_prompt.to_string()),
        }];

        // Add conversation history if provided
        if let Some(hist) = history {
            for msg in hist {
                messages.push(Message {
                    role: msg.role,
                    content: MessageContent::Text(msg.content),
                });
            }
        }

        // Add current user message
        messages.push(Message {
            role: "user".to_string(),
            content: MessageContent::Text(user_message.to_string()),
        });

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages,
            max_tokens: 2048,
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
            .map(|c| c.message.content.clone())
            .ok_or_else(|| "没有返回内容".to_string())
    }
    pub async fn analyze_image(&self, image_base64: &str, prompt: &str) -> Result<String, String> {
        let url = format!("{}/chat/completions", self.config.endpoint);

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: MessageContent::Parts(vec![
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
                ]),
            }],
            max_tokens: 10000,
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
            .map(|c| c.message.content.clone())
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
                content: MessageContent::Text("ping".to_string()),
            }],
            max_tokens: 1,
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

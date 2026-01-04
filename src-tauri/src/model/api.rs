use crate::storage::ApiConfig;
use reqwest::Client;
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
            .map_err(|e| format!("连接失败: {}", e))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("API 返回错误: {}", response.status()))
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

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("API 错误 {}: {}", status, text));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
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
            max_tokens: 1024,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("API 错误 {}: {}", status, text));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| format!("解析响应失败: {}", e))?;

        chat_response
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| "没有返回内容".to_string())
    }
}

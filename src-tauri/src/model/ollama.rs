use crate::storage::OllamaConfig;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OllamaClient {
    config: OllamaConfig,
    client: Client,
}

#[derive(Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    system: Option<String>,
    images: Option<Vec<String>>,
    stream: bool,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

#[derive(Deserialize)]
struct TagsResponse {
    models: Vec<ModelInfo>,
}

#[derive(Deserialize)]
struct ModelInfo {
    name: String,
}

impl OllamaClient {
    pub fn new(config: &OllamaConfig) -> Self {
        Self {
            config: config.clone(),
            client: Client::new(),
        }
    }

    pub async fn test_connection(&self) -> Result<(), String> {
        let url = format!("{}/api/tags", self.config.endpoint);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("连接 Ollama 失败: {}", e))?;

        if response.status().is_success() {
            let tags: TagsResponse = response
                .json()
                .await
                .map_err(|e| format!("解析响应失败: {}", e))?;

            // 检查模型是否存在
            let model_exists = tags
                .models
                .iter()
                .any(|m| m.name.starts_with(&self.config.model));

            if model_exists {
                Ok(())
            } else {
                Err(format!(
                    "模型 {} 未找到，请先运行 'ollama pull {}'",
                    self.config.model, self.config.model
                ))
            }
        } else {
            Err(format!("Ollama 返回错误: {}", response.status()))
        }
    }

    pub async fn chat(&self, system_prompt: &str, user_message: &str) -> Result<String, String> {
        let url = format!("{}/api/generate", self.config.endpoint);

        let request = GenerateRequest {
            model: self.config.model.clone(),
            prompt: user_message.to_string(),
            system: Some(system_prompt.to_string()),
            images: None,
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("Ollama 错误 {}: {}", status, text));
        }

        let generate_response: GenerateResponse = response
            .json()
            .await
            .map_err(|e| format!("解析响应失败: {}", e))?;

        Ok(generate_response.response)
    }

    pub async fn analyze_image(&self, image_base64: &str, prompt: &str) -> Result<String, String> {
        let url = format!("{}/api/generate", self.config.endpoint);

        let request = GenerateRequest {
            model: self.config.model.clone(),
            prompt: prompt.to_string(),
            system: None,
            images: Some(vec![image_base64.to_string()]),
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("Ollama 错误 {}: {}", status, text));
        }

        let generate_response: GenerateResponse = response
            .json()
            .await
            .map_err(|e| format!("解析响应失败: {}", e))?;

        Ok(generate_response.response)
    }
}

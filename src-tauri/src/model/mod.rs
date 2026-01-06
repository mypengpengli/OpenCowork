mod api;
mod error;
mod ollama;
pub mod traits;

pub use api::*;
pub use error::*;
pub use ollama::*;

use crate::storage::ModelConfig;

pub struct ModelManager;

impl ModelManager {
    pub fn new() -> Self {
        Self
    }

    pub async fn test_connection(&self, config: &ModelConfig) -> Result<(), String> {
        match config.provider.as_str() {
            "api" => {
                let api_client = ApiClient::new(&config.api);
                api_client.test_connection_with_fallback().await
            }
            "ollama" => {
                let ollama_client = OllamaClient::new(&config.ollama);
                ollama_client.test_connection().await
            }
            _ => Err("未知的模型提供者".to_string()),
        }
    }

    pub async fn chat(
        &self,
        config: &ModelConfig,
        context: &str,
        message: &str,
    ) -> Result<String, String> {
        let system_prompt = format!(
            r#"你是一个屏幕监控助手，帮助用户回顾和理解他们的操作历史。

{}

请根据上述操作记录，回答用户的问题。如果记录中没有相关信息，请如实告知。"#,
            context
        );

        match config.provider.as_str() {
            "api" => {
                let api_client = ApiClient::new(&config.api);
                api_client.chat(&system_prompt, message).await
            }
            "ollama" => {
                let ollama_client = OllamaClient::new(&config.ollama);
                ollama_client.chat(&system_prompt, message).await
            }
            _ => Err("未知的模型提供者".to_string()),
        }
    }

    pub async fn analyze_image(
        &self,
        config: &ModelConfig,
        image_base64: &str,
        prompt: &str,
    ) -> Result<String, String> {
        match config.provider.as_str() {
            "api" => {
                let api_client = ApiClient::new(&config.api);
                api_client.analyze_image(image_base64, prompt).await
            }
            "ollama" => {
                let ollama_client = OllamaClient::new(&config.ollama);
                ollama_client.analyze_image(image_base64, prompt).await
            }
            _ => Err("未知的模型提供者".to_string()),
        }
    }
}

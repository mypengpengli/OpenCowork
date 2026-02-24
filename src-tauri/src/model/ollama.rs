use crate::storage::{OllamaConfig, StorageManager};
use crate::commands::ChatHistoryMessage;
use chrono::Local;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub struct OllamaClient {
    config: OllamaConfig,
    client: Client,
}

const OLLAMA_CONNECT_TIMEOUT_SECS: u64 = 10;
const OLLAMA_REQUEST_TIMEOUT_SECS: u64 = 300;

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
            client: build_ollama_client(),
        }
    }

    pub async fn test_connection(&self) -> Result<(), String> {
        let url = format!("{}/api/tags", self.config.endpoint);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| {
                write_exchange_log("ollama-test", &url, "(none)", None, None, Some(&e.to_string()));
                format!("连接 Ollama 失败: {}", e)
            })?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        write_exchange_log("ollama-test", &url, "(none)", Some(status), Some(&text), None);

        if status.is_success() {
            let tags: TagsResponse = serde_json::from_str(&text)
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
            Err(format!("Ollama 返回错误 {}: {}", status, text))
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

        let request_json = serde_json::to_string_pretty(&request)
            .unwrap_or_else(|e| format!("无法序列化请求: {}", e));

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                write_exchange_log("ollama-chat", &url, &request_json, None, None, Some(&e.to_string()));
                format!("请求失败: {}", e)
            })?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        write_exchange_log("ollama-chat", &url, &request_json, Some(status), Some(&text), None);

        if !status.is_success() {
            return Err(format!("Ollama 错误 {}: {}", status, text));
        }

        let generate_response: GenerateResponse = serde_json::from_str(&text)
            .map_err(|e| format!("解析响应失败: {}", e))?;

        Ok(generate_response.response)
    }



    pub async fn chat_with_history(
        &self,
        system_prompt: &str,
        user_message: &str,
        history: Option<Vec<ChatHistoryMessage>>,
    ) -> Result<String, String> {
        let url = format!("{}/api/generate", self.config.endpoint);

        // Build prompt with history
        let mut full_prompt = String::new();
        if let Some(hist) = history {
            for msg in hist {
                let role_label = if msg.role == "user" { "用户" } else { "助手" };
                full_prompt.push_str(&format!("{}：{}

", role_label, msg.content));
            }
        }
        full_prompt.push_str(&format!("用户：{}", user_message));

        let request = GenerateRequest {
            model: self.config.model.clone(),
            prompt: full_prompt,
            system: Some(system_prompt.to_string()),
            images: None,
            stream: false,
        };

        let request_json = serde_json::to_string_pretty(&request)
            .unwrap_or_else(|e| format!("无法序列化请求: {}", e));

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                write_exchange_log("ollama-chat-history", &url, &request_json, None, None, Some(&e.to_string()));
                format!("请求失败: {}", e)
            })?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        write_exchange_log("ollama-chat-history", &url, &request_json, Some(status), Some(&text), None);

        if !status.is_success() {
            return Err(format!("Ollama 错误 {}: {}", status, text));
        }

        let generate_response: GenerateResponse = serde_json::from_str(&text)
            .map_err(|e| format!("解析响应失败: {}", e))?;

        Ok(generate_response.response)
    }

    pub async fn chat_with_history_with_images(
        &self,
        system_prompt: &str,
        user_message: &str,
        history: Option<Vec<ChatHistoryMessage>>,
        images: &[String],
    ) -> Result<String, String> {
        let url = format!("{}/api/generate", self.config.endpoint);

        let mut full_prompt = String::new();
        if let Some(hist) = history {
            for msg in hist {
                let role_label = if msg.role == "user" { "用户" } else { "助手" };
                full_prompt.push_str(&format!("{}：{}

", role_label, msg.content));
            }
        }
        full_prompt.push_str(&format!("用户：{}", user_message));

        let request = GenerateRequest {
            model: self.config.model.clone(),
            prompt: full_prompt,
            system: Some(system_prompt.to_string()),
            images: if images.is_empty() { None } else { Some(images.to_vec()) },
            stream: false,
        };

        let request_json = serde_json::to_string_pretty(&request)
            .unwrap_or_else(|e| format!("无法序列化请求: {}", e));

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                write_exchange_log("ollama-chat-history", &url, &request_json, None, None, Some(&e.to_string()));
                format!("请求失败: {}", e)
            })?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        write_exchange_log("ollama-chat-history", &url, &request_json, Some(status), Some(&text), None);

        if !status.is_success() {
            return Err(format!("Ollama 错误 {}: {}", status, text));
        }

        let generate_response: GenerateResponse = serde_json::from_str(&text)
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

        let request_json = serde_json::to_string_pretty(&request)
            .unwrap_or_else(|e| format!("无法序列化请求: {}", e));

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                write_exchange_log("ollama-image", &url, &request_json, None, None, Some(&e.to_string()));
                format!("请求失败: {}", e)
            })?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        write_exchange_log("ollama-image", &url, &request_json, Some(status), Some(&text), None);

        if !status.is_success() {
            return Err(format!("Ollama 错误 {}: {}", status, text));
        }

        let generate_response: GenerateResponse = serde_json::from_str(&text)
            .map_err(|e| format!("解析响应失败: {}", e))?;

        Ok(generate_response.response)
    }
}

fn build_ollama_client() -> Client {
    Client::builder()
        .connect_timeout(Duration::from_secs(OLLAMA_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(OLLAMA_REQUEST_TIMEOUT_SECS))
        .build()
        .unwrap_or_else(|_| Client::new())
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

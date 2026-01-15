mod api;
mod error;
mod ollama;
pub mod traits;

pub use api::*;
pub use error::*;
pub use ollama::*;

use crate::storage::ModelConfig;
use crate::commands::ChatHistoryMessage;
use crate::skills::SkillMetadata;

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



    pub async fn chat_with_history(
        &self,
        config: &ModelConfig,
        context: &str,
        message: &str,
        history: Option<Vec<ChatHistoryMessage>>,
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
                api_client.chat_with_history(&system_prompt, message, history).await
            }
            "ollama" => {
                let ollama_client = OllamaClient::new(&config.ollama);
                ollama_client.chat_with_history(&system_prompt, message, history).await
            }
            _ => Err("未知的模型提供者".to_string()),
        }
    }

    pub async fn chat_with_history_with_images(
        &self,
        config: &ModelConfig,
        context: &str,
        message: &str,
        history: Option<Vec<ChatHistoryMessage>>,
        image_urls: Vec<String>,
        image_base64: Vec<String>,
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
                api_client
                    .chat_with_history_with_images(&system_prompt, message, history, &image_urls)
                    .await
            }
            "ollama" => {
                let ollama_client = OllamaClient::new(&config.ollama);
                ollama_client
                    .chat_with_history_with_images(&system_prompt, message, history, &image_base64)
                    .await
            }
            _ => Err("未知的模型提供者".to_string()),
        }
    }

    /// 使用自定义 system prompt 进行对话（用于 skills）
    pub async fn chat_with_system_prompt(
        &self,
        config: &ModelConfig,
        system_prompt: &str,
        message: &str,
        history: Option<Vec<ChatHistoryMessage>>,
    ) -> Result<String, String> {
        match config.provider.as_str() {
            "api" => {
                let api_client = ApiClient::new(&config.api);
                api_client.chat_with_history(system_prompt, message, history).await
            }
            "ollama" => {
                let ollama_client = OllamaClient::new(&config.ollama);
                ollama_client.chat_with_history(system_prompt, message, history).await
            }
            _ => Err("未知的模型提供者".to_string()),
        }
    }

    pub async fn chat_with_system_prompt_with_images(
        &self,
        config: &ModelConfig,
        system_prompt: &str,
        message: &str,
        history: Option<Vec<ChatHistoryMessage>>,
        image_urls: Vec<String>,
        image_base64: Vec<String>,
    ) -> Result<String, String> {
        match config.provider.as_str() {
            "api" => {
                let api_client = ApiClient::new(&config.api);
                api_client
                    .chat_with_history_with_images(system_prompt, message, history, &image_urls)
                    .await
            }
            "ollama" => {
                let ollama_client = OllamaClient::new(&config.ollama);
                ollama_client
                    .chat_with_history_with_images(system_prompt, message, history, &image_base64)
                    .await
            }
            _ => Err("未知的模型提供者".to_string()),
        }
    }

    /// 带 Tool Use 的对话（仅 API 模式支持）
    pub async fn chat_with_tools(
        &self,
        config: &ModelConfig,
        context: &str,
        message: &str,
        history: Option<Vec<ChatHistoryMessage>>,
        available_skills: &[SkillMetadata],
    ) -> Result<ChatWithToolsResult, String> {
        let system_prompt = format!(
            r#"你是一个屏幕监控助手，帮助用户回顾和理解他们的操作历史。

{}

请根据上述操作记录，回答用户的问题。如果记录中没有相关信息，请如实告知。

你有以下能力：
1. 如果用户的请求需要使用某个技能来完成，请调用 invoke_skill 工具。
2. 如果用户想要创建、修改或删除技能，请调用 manage_skill 工具。
3. 你可以使用 Read/Write/Edit/Update/Glob/Grep 工具读写和搜索文件。
4. 你可以使用 Bash 工具运行命令（受权限限制）。"#,
            context
        );

        self.chat_with_tools_with_system_prompt(
            config,
            &system_prompt,
            message,
            history,
            available_skills,
        )
        .await
    }

    pub async fn chat_with_tools_with_images(
        &self,
        config: &ModelConfig,
        context: &str,
        message: &str,
        history: Option<Vec<ChatHistoryMessage>>,
        available_skills: &[SkillMetadata],
        image_urls: Vec<String>,
        image_base64: Vec<String>,
    ) -> Result<ChatWithToolsResult, String> {
        let system_prompt = format!(
            r#"你是一个屏幕监控助手，帮助用户回顾和理解他们的操作历史。

{}

请根据上述操作记录，回答用户的问题。如果记录中没有相关信息，请如实告知。

你有以下能力：
1. 如果用户的请求需要使用某个技能来完成，请调用 invoke_skill 工具。
2. 如果用户想要创建、修改或删除技能，请调用 manage_skill 工具。
3. 你可以使用 Read/Write/Edit/Update/Glob/Grep 工具读写和搜索文件。
4. 你可以使用 Bash 工具运行命令（受权限限制）。"#,
            context
        );

        self.chat_with_tools_with_system_prompt_with_images(
            config,
            &system_prompt,
            message,
            history,
            available_skills,
            image_urls,
            image_base64,
        )
        .await
    }

    pub async fn chat_with_tools_with_system_prompt(
        &self,
        config: &ModelConfig,
        system_prompt: &str,
        message: &str,
        history: Option<Vec<ChatHistoryMessage>>,
        available_skills: &[SkillMetadata],
    ) -> Result<ChatWithToolsResult, String> {
        match config.provider.as_str() {
            "api" => {
                let api_client = ApiClient::new(&config.api);
                let tools = ApiClient::create_skill_tools(available_skills);
                api_client
                    .chat_with_tools(system_prompt, message, history, tools)
                    .await
            }
            "ollama" => {
                let ollama_client = OllamaClient::new(&config.ollama);
                let result = ollama_client
                    .chat_with_history(system_prompt, message, history)
                    .await?;
                Ok(ChatWithToolsResult::Text(result))
            }
            _ => Err("未知的模型提供者".to_string()),
        }
    }

    pub async fn chat_with_tools_with_system_prompt_with_images(
        &self,
        config: &ModelConfig,
        system_prompt: &str,
        message: &str,
        history: Option<Vec<ChatHistoryMessage>>,
        available_skills: &[SkillMetadata],
        image_urls: Vec<String>,
        image_base64: Vec<String>,
    ) -> Result<ChatWithToolsResult, String> {
        match config.provider.as_str() {
            "api" => {
                let api_client = ApiClient::new(&config.api);
                let tools = ApiClient::create_skill_tools(available_skills);
                api_client
                    .chat_with_tools_with_images(system_prompt, message, history, tools, &image_urls)
                    .await
            }
            "ollama" => {
                let ollama_client = OllamaClient::new(&config.ollama);
                let result = ollama_client
                    .chat_with_history_with_images(system_prompt, message, history, &image_base64)
                    .await?;
                Ok(ChatWithToolsResult::Text(result))
            }
            _ => Err("未知的模型提供者".to_string()),
        }
    }

    pub async fn continue_with_tool_results(
        &self,
        config: &ModelConfig,
        system_prompt: &str,
        messages_so_far: Vec<api::Message>,
        tool_results: Vec<(String, String)>,
        available_skills: &[SkillMetadata],
    ) -> Result<ChatWithToolsResult, String> {
        match config.provider.as_str() {
            "api" => {
                let api_client = ApiClient::new(&config.api);
                let tools = ApiClient::create_skill_tools(available_skills);
                api_client
                    .continue_with_tool_results(system_prompt, messages_so_far, tool_results, tools)
                    .await
            }
            "ollama" => Err("Ollama 不支持 tool use".to_string()),
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

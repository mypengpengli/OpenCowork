use async_trait::async_trait;

/// 模型提供者的统一接口
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// 测试连接
    async fn test_connection(&self) -> Result<(), String>;

    /// 文本对话
    async fn chat(&self, system_prompt: &str, user_message: &str) -> Result<String, String>;

    /// 图片分析
    async fn analyze_image(&self, image_base64: &str, prompt: &str) -> Result<String, String>;
}

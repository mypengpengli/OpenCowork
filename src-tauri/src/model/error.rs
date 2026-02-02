use chrono::Local;
use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct ModelErrorAlert {
    pub timestamp: String,
    pub error_type: String,
    pub message: String,
    pub suggestion: String,
    pub detail: String,
    pub source: String,
}

pub fn build_model_error_alert(detail: &str, source: &str) -> ModelErrorAlert {
    let info = classify_model_error(detail);

    ModelErrorAlert {
        timestamp: Local::now().to_rfc3339(),
        error_type: info.error_type.to_string(),
        message: info.message,
        suggestion: info.suggestion,
        detail: detail.to_string(),
        source: source.to_string(),
    }
}

struct ModelErrorInfo {
    error_type: &'static str,
    message: String,
    suggestion: String,
}

fn classify_model_error(detail: &str) -> ModelErrorInfo {
    let lower = detail.to_lowercase();

    if lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("invalid api key")
        || lower.contains("authentication")
    {
        return ModelErrorInfo {
            error_type: "unauthorized",
            message: "API 未授权或 Key 无效".to_string(),
            suggestion: "检查 API Key、权限和接口地址是否匹配".to_string(),
        };
    }

    if lower.contains("insufficient_quota")
        || lower.contains("quota")
        || lower.contains("balance")
        || lower.contains("billing")
        || lower.contains("payment")
        || detail.contains("余额")
        || detail.contains("欠费")
        || detail.contains("配额")
    {
        return ModelErrorInfo {
            error_type: "insufficient_quota",
            message: "余额或配额不足".to_string(),
            suggestion: "检查账户余额或更换可用账号".to_string(),
        };
    }

    if lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
    {
        return ModelErrorInfo {
            error_type: "rate_limit",
            message: "请求过于频繁或触发限流".to_string(),
            suggestion: "降低频率或稍后重试".to_string(),
        };
    }

    if lower.contains("timeout") || lower.contains("timed out") {
        return ModelErrorInfo {
            error_type: "timeout",
            message: "请求超时".to_string(),
            suggestion: "检查网络或稍后重试".to_string(),
        };
    }

    if lower.contains("dns")
        || lower.contains("failed to lookup address")
        || lower.contains("connection refused")
        || lower.contains("connection reset")
        || lower.contains("connect")
        || lower.contains("network")
        || detail.contains("网络")
        || detail.contains("无法连接")
        || detail.contains("连接失败")
    {
        return ModelErrorInfo {
            error_type: "network",
            message: "网络连接失败".to_string(),
            suggestion: "检查网络、代理或接口地址".to_string(),
        };
    }

    if lower.contains("400")
        || lower.contains("404")
        || lower.contains("invalid")
        || (lower.contains("model") && lower.contains("not found"))
    {
        return ModelErrorInfo {
            error_type: "invalid_request",
            message: "请求参数或模型名称无效".to_string(),
            suggestion: "确认模型名称与接口是否兼容 OpenAI 格式".to_string(),
        };
    }

    if lower.contains("500")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("504")
    {
        return ModelErrorInfo {
            error_type: "server_error",
            message: "服务端错误".to_string(),
            suggestion: "稍后重试或切换节点".to_string(),
        };
    }

    ModelErrorInfo {
        error_type: "unknown",
        message: "模型调用失败".to_string(),
        suggestion: "查看错误详情或日志".to_string(),
    }
}

pub fn is_transient_model_error(detail: &str) -> bool {
    matches!(
        classify_model_error(detail).error_type,
        "timeout" | "network" | "rate_limit" | "server_error"
    )
}


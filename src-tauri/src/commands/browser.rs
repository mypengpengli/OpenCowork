use super::*;
use serde_json::{json, Value};

pub(super) async fn execute(
    config: &Config,
    access: &ToolAccess,
    owner: &str,
    args: Value,
    cancel: Option<&CancellationToken>,
) -> Result<String, String> {
    let server = config
        .tools
        .browser_server
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or("Select a Playwright MCP server in settings to enable browser control")?;
    let action = args["action"].as_str().ok_or("Missing browser action")?;
    let (tool, mut params) = match action {
        "navigate" => {
            let url = args["url"].as_str().ok_or("Missing URL")?;
            let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
            if !["http", "https"].contains(&parsed.scheme()) {
                return Err("Browser navigation supports HTTP(S) URLs only".into());
            }
            ("browser_navigate", json!({"url":url}))
        }
        "snapshot" => ("browser_snapshot", json!({})),
        "click" => (
            "browser_click",
            json!({"ref":args["ref"].as_str().ok_or("Use a ref from the latest snapshot")?,"element":args["element"].as_str().unwrap_or("selected element")}),
        ),
        "type" => (
            "browser_type",
            json!({"ref":args["ref"].as_str().ok_or("Use a ref from the latest snapshot")?,"element":args["element"].as_str().unwrap_or("text field"),"text":args["text"].as_str().ok_or("Missing text")?,"submit":false}),
        ),
        "screenshot" => (
            "browser_take_screenshot",
            json!({"type":"png", "scale":"css"}),
        ),
        _ => return Err("Unsupported browser action".into()),
    };
    if matches!(action, "click" | "type") {
        // Playwright MCP renamed ref to target. Negotiate from the advertised schema.
        let tools: Value =
            serde_json::from_str(&mcp::list(config, owner, &access.base_dir, cancel).await?)
                .map_err(|e| e.to_string())?;
        let schema = tools
            .as_array()
            .and_then(|tools| {
                tools
                    .iter()
                    .find(|item| item["server"] == server && item["tool"]["name"] == tool)
            })
            .ok_or("Browser action is not advertised or authorized by the configured server")?;
        if schema["tool"]["inputSchema"]["properties"]
            .get("target")
            .is_some()
        {
            let reference = params.as_object_mut().unwrap().remove("ref").unwrap();
            params["target"] = reference;
        }
    }
    mcp::call(
        config,
        owner,
        &access.base_dir,
        server,
        tool,
        params,
        cancel,
    )
    .await
}

use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader as AsyncBufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
}

struct Session {
    stdin: ChildStdin,
    stdout: AsyncBufReader<ChildStdout>,
    _child: Child,
    _tree: process::ProcessTree,
    next_id: u64,
}
type Slot = Arc<TokioMutex<Option<Session>>>;
fn sessions() -> &'static TokioMutex<HashMap<String, Slot>> {
    static SESSIONS: OnceLock<TokioMutex<HashMap<String, Slot>>> = OnceLock::new();
    SESSIONS.get_or_init(Default::default)
}

async fn read_frame(stdout: &mut AsyncBufReader<ChildStdout>) -> Result<Value, String> {
    let mut data = Vec::new();
    loop {
        let buffer = stdout.fill_buf().await.map_err(|e| e.to_string())?;
        if buffer.is_empty() {
            return Err("MCP server closed stdout".into());
        }
        let n = buffer
            .iter()
            .position(|b| *b == b'\n')
            .map(|n| n + 1)
            .unwrap_or(buffer.len());
        if data.len() + n > 8 * 1024 * 1024 {
            return Err("MCP response exceeds 8 MiB".into());
        }
        let done = buffer[n - 1] == b'\n';
        data.extend_from_slice(&buffer[..n]);
        stdout.consume(n);
        if done {
            return serde_json::from_slice(&data).map_err(|e| format!("Invalid MCP JSON: {e}"));
        }
    }
}

impl Session {
    async fn send(&mut self, value: Value) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        self.stdin
            .write_all(&bytes)
            .await
            .map_err(|e| e.to_string())?;
        self.stdin.flush().await.map_err(|e| e.to_string())
    }
    async fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params}))
            .await?;
        loop {
            let value = read_frame(&mut self.stdout).await?;
            if value.get("method").is_some() {
                if let Some(id) = value.get("id") {
                    let response = if value["method"] == "ping" {
                        json!({"jsonrpc":"2.0","id":id,"result":{}})
                    } else {
                        json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Client capability not supported"}})
                    };
                    self.send(response).await?;
                }
                continue;
            }
            if value.get("id") != Some(&json!(id)) {
                continue;
            }
            if let Some(error) = value.get("error") {
                return Err(format!("MCP error: {error}"));
            }
            return value
                .get("result")
                .cloned()
                .ok_or("MCP response missing result".into());
        }
    }
}

fn server_command(config: &McpServerConfig) -> Result<TokioCommand, String> {
    if config.command.trim().is_empty() {
        return Err("MCP command is empty".into());
    }
    // Node launchers on Windows are command scripts. Do not interpolate arbitrary args into cmd.
    #[cfg(windows)]
    if config.command.to_ascii_lowercase().ends_with(".cmd")
        || config.command.to_ascii_lowercase().ends_with(".bat")
    {
        return Err("Use node.exe with the server's .js entrypoint on Windows (or an .exe), instead of .cmd/.bat launchers".into());
    }
    let mut command = TokioCommand::new(&config.command);
    command.args(&config.args).envs(&config.env);
    Ok(command)
}

async fn connect(config: &McpServerConfig, cwd: &Path) -> Result<Session, String> {
    let mut command = server_command(config)?;
    process::managed_command(&mut command);
    command
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|e| format!("MCP {}: {e}", config.name))?;
    let tree = process::ProcessTree::attach(child.id().ok_or("MCP process exited")?)?;
    let stdin = child.stdin.take().ok_or("Missing MCP stdin")?;
    let stdout = AsyncBufReader::new(child.stdout.take().ok_or("Missing MCP stdout")?);
    let mut session = Session {
        stdin,
        stdout,
        _child: child,
        _tree: tree,
        next_id: 0,
    };
    let initialized = session.request("initialize", json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"OpenCowork","version":env!("CARGO_PKG_VERSION")}})).await?;
    let version = initialized["protocolVersion"]
        .as_str()
        .ok_or("Missing MCP protocol version")?;
    if !["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"].contains(&version) {
        return Err(format!("Unsupported MCP protocol: {version}"));
    }
    session
        .send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .await?;
    Ok(session)
}

async fn request(
    config: &McpServerConfig,
    owner: &str,
    cwd: &Path,
    method: &str,
    params: Value,
    cancel: Option<&CancellationToken>,
) -> Result<Value, String> {
    let key = format!("{owner}/{}", config.name);
    let slot = sessions().lock().await.entry(key).or_default().clone();
    let mut session = slot.lock().await;
    let request = async {
        if session.is_none() {
            *session = Some(connect(config, cwd).await?);
        }
        session.as_mut().unwrap().request(method, params).await
    };
    let deadline = timeout(TokioDuration::from_secs(120), request);
    let result = if let Some(cancel) = cancel {
        tokio::select! {
            _ = cancel.cancelled() => Err(REQUEST_CANCELLED_ERROR.into()),
            result = deadline => result.map_err(|_| "MCP request timed out".to_string()).and_then(|r| r),
        }
    } else {
        deadline
            .await
            .map_err(|_| "MCP request timed out".to_string())
            .and_then(|r| r)
    };
    // A cancelled/failed transport is never reused with an unread response.
    if result.is_err() {
        *session = None;
    }
    result
}

pub(super) async fn shutdown(owner: &str) {
    let prefix = format!("{owner}/");
    sessions()
        .lock()
        .await
        .retain(|key, _| !key.starts_with(&prefix));
}

pub(super) async fn list(
    config: &Config,
    owner: &str,
    cwd: &Path,
    cancel: Option<&CancellationToken>,
) -> Result<String, String> {
    if config.tools.mode == "unset" {
        return Err(TOOL_MODE_UNSET_ERROR.into());
    }
    let mut results = Vec::new();
    for server in config.tools.mcp_servers.iter().filter(|s| s.enabled) {
        let mut cursor = None;
        let mut pages = 0;
        loop {
            let params = cursor
                .as_ref()
                .map(|c: &String| json!({"cursor":c}))
                .unwrap_or(json!({}));
            match request(server, owner, cwd, "tools/list", params, cancel).await {
                Ok(result) => {
                    if let Some(tools) = result["tools"].as_array() {
                        for tool in tools {
                            let name = tool["name"].as_str().unwrap_or("");
                            if config.tools.mode == "allow_all"
                                || server.allowed_tools.iter().any(|s| s == name)
                            {
                                results.push(json!({"server":server.name,"tool":tool}));
                            }
                        }
                    }
                    cursor = result["nextCursor"].as_str().map(String::from);
                    pages += 1;
                    if cursor.is_none() {
                        break;
                    }
                    if pages >= 20 {
                        return Err("MCP tool listing exceeded 20 pages".into());
                    }
                }
                Err(err) if err == REQUEST_CANCELLED_ERROR => return Err(err),
                Err(err) => {
                    results.push(json!({"server":server.name,"error":err}));
                    break;
                }
            }
        }
    }
    serde_json::to_string(&results).map_err(|e| e.to_string())
}

pub(super) async fn call(
    config: &Config,
    owner: &str,
    cwd: &Path,
    server: &str,
    tool: &str,
    arguments: Value,
    cancel: Option<&CancellationToken>,
) -> Result<String, String> {
    if config.tools.mode == "unset" {
        return Err(TOOL_MODE_UNSET_ERROR.into());
    }
    let server = config
        .tools
        .mcp_servers
        .iter()
        .find(|s| s.enabled && s.name == server)
        .ok_or("MCP server not enabled")?;
    if config.tools.mode != "allow_all" && !server.allowed_tools.iter().any(|s| s == tool) {
        return Err("MCP tool not authorized in server allowed_tools".into());
    }
    if !arguments.is_object() {
        return Err("MCP arguments must be a JSON object".into());
    }
    let mut result = request(
        server,
        owner,
        cwd,
        "tools/call",
        json!({"name":tool,"arguments":arguments}),
        cancel,
    )
    .await?;
    if result["isError"] == true {
        return Err(format!("MCP tool failed: {result}"));
    }
    // Keep image payloads out of text context and expose real previewable artifacts.
    if let Some(content) = result["content"].as_array_mut() {
        for block in content {
            if block["type"] != "image" {
                continue;
            }
            let extension = match block["mimeType"].as_str() {
                Some("image/png") => "png",
                Some("image/jpeg") => "jpg",
                Some("image/webp") => "webp",
                _ => return Err("Unsupported MCP image type".into()),
            };
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(block["data"].as_str().ok_or("Missing MCP image data")?)
                .map_err(|e| e.to_string())?;
            let access = build_tool_access(config, &StorageManager::new(), Some(cwd));
            let directory =
                ensure_path_allowed(&access, &cwd.join(".task_outputs").display().to_string())?;
            fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
            let path = directory.join(format!("{}.{}", next_background_task_id(), extension));
            fs::write(&path, bytes).map_err(|e| e.to_string())?;
            tasks::artifact(owner, &path.display().to_string())?;
            *block = json!({"type":"text","text":format!("Image artifact saved: {}. Use the browser snapshot for page structure.", path.display())});
        }
    }
    Ok(result.to_string())
}

#[tauri::command]
pub async fn test_mcp_connections() -> Result<String, String> {
    let storage = StorageManager::new();
    let config = storage.load_config().map_err(|e| e.to_string())?;
    let access = build_tool_access(&config, &storage, None);
    let owner = format!("connection-test-{}", next_background_task_id());
    let result = list(&config, &owner, &access.base_dir, None).await;
    shutdown(&owner).await;
    result
}

pub(super) fn validate(config: &Config) -> Result<(), String> {
    let mut names = HashSet::new();
    for server in &config.tools.mcp_servers {
        if server.name.is_empty()
            || server.name.contains(['/', '\\'])
            || !names.insert(&server.name)
        {
            return Err("MCP server names must be nonempty, unique and contain no slash".into());
        }
        server_command(server)?;
    }
    if let Some(name) = config
        .tools
        .browser_server
        .as_deref()
        .filter(|s| !s.is_empty())
    {
        if !config
            .tools
            .mcp_servers
            .iter()
            .any(|s| s.name == name && s.enabled)
        {
            return Err("Browser server must name an enabled MCP server".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Config, PathBuf, String) {
        let mut config = Config::default();
        config.tools.mode = "whitelist".into();
        let directory = std::env::temp_dir().join(next_background_task_id());
        fs::create_dir_all(&directory).unwrap();
        config.tools.allowed_dirs = vec![directory.display().to_string()];
        config.tools.mcp_servers = vec![McpServerConfig {
            name: "fixture".into(),
            enabled: true,
            command: "node".into(),
            args: vec![Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/mock-mcp.cjs")
                .display()
                .to_string()],
            env: HashMap::new(),
            allowed_tools: vec![
                "echo".into(),
                "wait".into(),
                "browser_navigate".into(),
                "browser_snapshot".into(),
                "browser_take_screenshot".into(),
            ],
        }];
        config.tools.browser_server = Some("fixture".into());
        (config, directory, next_background_task_id())
    }

    #[tokio::test]
    async fn test_mcp_initialize_pagination_authorization_and_session_reuse() {
        let (config, cwd, owner) = fixture();
        let tools = list(&config, &owner, &cwd, None).await.unwrap();
        assert!(tools.contains("echo"));
        assert!(!tools.contains("forbidden"));
        assert!(call(
            &config,
            &owner,
            &cwd,
            "fixture",
            "forbidden",
            json!({}),
            None
        )
        .await
        .is_err());
        for expected in 1..=2 {
            let result = call(
                &config,
                &owner,
                &cwd,
                "fixture",
                "echo",
                json!({"value":"hello"}),
                None,
            )
            .await
            .unwrap();
            let result: Value = serde_json::from_str(&result).unwrap();
            let text: Value =
                serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(text["calls"], expected);
            assert_eq!(text["arguments"]["value"], "hello");
        }
        shutdown(&owner).await;
        assert!(!sessions()
            .lock()
            .await
            .contains_key(&format!("{owner}/fixture")));
    }

    #[tokio::test]
    async fn test_mcp_cancelled_transport_reconnects_without_stale_response() {
        let (config, cwd, owner) = fixture();
        list(&config, &owner, &cwd, None).await.unwrap();
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let stopper = tokio::spawn(async move {
            sleep(TokioDuration::from_millis(50)).await;
            trigger.cancel();
        });
        let error = call(
            &config,
            &owner,
            &cwd,
            "fixture",
            "wait",
            json!({}),
            Some(&cancel),
        )
        .await
        .unwrap_err();
        assert_eq!(error, REQUEST_CANCELLED_ERROR);
        stopper.await.unwrap();
        assert!(
            call(&config, &owner, &cwd, "fixture", "echo", json!({}), None)
                .await
                .is_ok()
        );
        shutdown(&owner).await;
    }

    #[tokio::test]
    async fn test_browser_navigate_and_screenshot_artifact() {
        let (config, cwd, owner) = fixture();
        let access = build_tool_access(&config, &StorageManager::new(), Some(&cwd));
        assert!(browser::execute(
            &config,
            &access,
            &owner,
            json!({"action":"navigate","url":"file:///etc/passwd"}),
            None
        )
        .await
        .is_err());
        let result = browser::execute(
            &config,
            &access,
            &owner,
            json!({"action":"navigate","url":"https://example.com"}),
            None,
        )
        .await
        .unwrap();
        assert!(result.contains("browser_navigate"));
        let result = browser::execute(
            &config,
            &access,
            &owner,
            json!({"action":"screenshot"}),
            None,
        )
        .await
        .unwrap();
        assert!(result.contains("Image artifact saved"));
        assert!(!result.contains("iVBOR"));
        assert_eq!(fs::read_dir(cwd.join(".task_outputs")).unwrap().count(), 1);
        shutdown(&owner).await;
    }

    #[tokio::test]
    #[ignore = "Requires OPENCOWORK_PLAYWRIGHT_MCP pointing to an installed cli.js and Microsoft Edge"]
    async fn test_real_playwright_browser_workflow() {
        use std::io::{Read, Write};
        let entry = std::env::var("OPENCOWORK_PLAYWRIGHT_MCP")
            .expect("Set OPENCOWORK_PLAYWRIGHT_MCP to Playwright MCP cli.js");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_server = stop.clone();
        let server = std::thread::spawn(move || {
            while !stop_server.load(Ordering::SeqCst) {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    let _ = stream.read(&mut [0; 4096]);
                    let body = r#"<!doctype html><title>OpenCowork browser fixture</title><label>Name<input id="name"></label><button onclick="document.querySelector('output').textContent='Hello '+document.querySelector('input').value">Greet</button><output>Ready</output>"#;
                    let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
                } else {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        });
        let (mut config, cwd, owner) = fixture();
        config.tools.mcp_servers[0].args = vec![
            entry,
            "--headless".into(),
            "--isolated".into(),
            "--browser".into(),
            "msedge".into(),
        ];
        config.tools.mcp_servers[0]
            .allowed_tools
            .extend(["browser_type".into(), "browser_click".into()]);
        let access = build_tool_access(&config, &StorageManager::new(), Some(&cwd));
        let result = async {
            browser::execute(
                &config,
                &access,
                &owner,
                json!({"action":"navigate","url":url}),
                None,
            )
            .await?;
            let page =
                browser::execute(&config, &access, &owner, json!({"action":"snapshot"}), None)
                    .await?;
            let text = serde_json::from_str::<Value>(&page).unwrap()["content"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|c| c["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let input_ref = Regex::new(r#"textbox "Name" \[ref=([^\]]+)\]"#)
                .unwrap()
                .captures(&text)
                .ok_or_else(|| format!("Missing input in snapshot: {text}"))?[1]
                .to_string();
            let button_ref = Regex::new(r#"button "Greet" \[ref=([^\]]+)\]"#)
                .unwrap()
                .captures(&text)
                .ok_or_else(|| format!("Missing button in snapshot: {text}"))?[1]
                .to_string();
            browser::execute(
                &config,
                &access,
                &owner,
                json!({"action":"type","ref":input_ref,"text":"OpenCowork"}),
                None,
            )
            .await?;
            browser::execute(
                &config,
                &access,
                &owner,
                json!({"action":"click","ref":button_ref}),
                None,
            )
            .await?;
            let page =
                browser::execute(&config, &access, &owner, json!({"action":"snapshot"}), None)
                    .await?;
            if !page.contains("Hello OpenCowork") {
                return Err(format!("Missing changed page: {page}"));
            }
            let screenshot = browser::execute(
                &config,
                &access,
                &owner,
                json!({"action":"screenshot"}),
                None,
            )
            .await?;
            if !screenshot.contains("Image artifact saved") {
                return Err(format!("No screenshot artifact: {screenshot}"));
            }
            Ok::<(), String>(())
        }
        .await;
        shutdown(&owner).await;
        stop.store(true, Ordering::SeqCst);
        server.join().unwrap();
        result.unwrap();
    }
}

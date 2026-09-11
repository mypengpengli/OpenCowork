use super::*;
use opencowork_api::*;
pub(super) fn profile(
    cwd: &Path,
    background: bool,
) -> Result<(String, OpenAiCompatProfile), String> {
    apply_shell_api_key_override(cwd)?;
    let c = ConfigLoader::default_for(cwd)
        .load()
        .map_err(|e| e.to_string())?;
    let model = if background {
        c.merged()
            .pointer("/execution/backgroundModel")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .or(c.model())
    } else {
        c.model()
    }
    .unwrap_or("gpt-5.4-mini")
    .to_string();
    let mut profile = if let Some(p) = c.provider() {
        OpenAiCompatProfile::custom(
            p.name(),
            p.api_key_env(),
            p.base_url(),
            p.base_url_env().map(str::to_owned),
        )
    } else {
        default_openai_profile_for_model(&model)
    }
    .with_timeout_ms(20000);
    profile.reasoning_effort = c
        .merged()
        .pointer("/execution/reasoningEffort")
        .and_then(Value::as_str)
        .filter(|s| ["none", "minimal", "low", "medium", "high"].contains(s))
        .map(str::to_owned);
    Ok((model, profile))
}
pub(super) fn worker() -> Result<(), String> {
    let (model, profile) = profile(&env::current_dir().map_err(|e| e.to_string())?, false)?;
    let mut client = OpenAiCompatClient::from_profile(profile)?;
    let mut checks = Vec::new();
    for kind in ["connection", "tools", "vision", "streaming"] {
        let prompt = match kind {
            "tools" => "Call probe_stamp with value 7. Do not answer in prose.",
            "vision" => "What color is the supplied image? Answer one English color word.",
            _ => "Reply OK.",
        };
        let mut request = ProviderRequest {
            model: model.clone(),
            max_output_tokens: Some(256),
            system_prompt: vec![],
            messages: vec![InputMessage {
                role: "user".into(),
                content: Some(prompt.into()),
                image_urls: vec![],
                tool_calls: vec![],
                tool_call_id: None,
            }],
            tools: vec![],
        };
        if kind == "tools" {
            request.tools.push(ToolDefinition{name:"probe_stamp".into(),description:Some("Harmless diagnostic; return the requested value".into()),input_schema:serde_json::json!({"type":"object","properties":{"value":{"type":"integer"}},"required":["value"]})});
        }
        if kind == "vision" {
            request.messages[0]
                .image_urls
                .push(include_str!("probe-image.txt").trim().into());
        }
        let start = std::time::Instant::now();
        let result = if kind == "streaming" {
            client
                .stream_execute(request)
                .map(|events| ProviderResponse { events })
        } else {
            client.execute(request)
        };
        let check = match result {
            Ok(r) => {
                let text = r
                    .events
                    .iter()
                    .filter_map(|e| {
                        if let ProviderEvent::TextDelta(t) = e {
                            Some(t.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<String>();
                let verified=match kind {"tools"=>r.events.iter().any(|e|matches!(e,ProviderEvent::ToolCall{name,input,..} if name=="probe_stamp" && input["value"]==7)),"vision"=>text.trim().trim_end_matches('.').eq_ignore_ascii_case("red"),_=>!text.trim().is_empty()};
                let tokens = r
                    .events
                    .iter()
                    .filter_map(|e| {
                        if let ProviderEvent::Usage {
                            input_tokens,
                            output_tokens,
                            cache_read_input_tokens,
                            cache_creation_input_tokens,
                        } = e
                        {
                            Some(
                                *input_tokens as u64
                                    + *output_tokens as u64
                                    + *cache_read_input_tokens as u64
                                    + *cache_creation_input_tokens as u64,
                            )
                        } else {
                            None
                        }
                    })
                    .sum::<u64>();
                serde_json::json!({"kind":kind,"status":if verified{"verified"}else{"inconclusive"},"tokens":tokens})
            }
            Err(e) => serde_json::json!({"kind":kind,"status":"failed","error":e}),
        };
        let mut check = check;
        check["elapsedMs"] = (start.elapsed().as_millis() as u64).into();
        checks.push(check);
    }
    println!(
        "{}",
        serde_json::json!({"model":model,"protocol":"chat-completions","reasoning":"Configured effort is sent when selected; hidden reasoning quality cannot be inferred from connectivity.","checks":checks})
    );
    Ok(())
}
pub(super) async fn probe(State(state): State<ShellState>) -> Result<Json<Value>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new(env::current_exe().map_err(internal_error)?);
        cmd.arg("--provider-probe")
            .current_dir(&state.cwd)
            .env("OPENCOWORK_CONFIG_HOME", &state.config_home)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let mut child = cmd.spawn().map_err(internal_error)?;
        let _tree = opencowork_runtime::ProcessTree::attach(child.id()).map_err(internal_error)?;
        let start = std::time::Instant::now();
        while child.try_wait().map_err(internal_error)?.is_none() {
            if start.elapsed().as_secs() > 100 {
                return Err(ApiError::bad_request("Provider diagnostic timed out"));
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let output = child.wait_with_output().map_err(internal_error)?;
        serde_json::from_slice(&output.stdout)
            .map(Json)
            .map_err(|_| {
                ApiError::bad_request("Diagnostic could not start; check provider credentials")
            })
    })
    .await
    .map_err(internal_error)?
}

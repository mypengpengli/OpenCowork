use super::*;
use crate::model::{ApiClient, Message};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;

#[tokio::test]
async fn test_repeated_image_prompts_keep_their_original_images() {
    let messages: Vec<Message> = serde_json::from_value(json!([
        {"role":"user","content":[{"type":"text","text":"inspect"},{"type":"image_url","image_url":{"url":"data:image/png;base64,first"}}]},
        {"role":"assistant","content":"first image"},
        {"role":"user","content":[{"type":"text","text":"inspect"},{"type":"image_url","image_url":{"url":"data:image/png;base64,second"}}]}
    ])).unwrap();
    let result = prepare_api_messages(
        messages,
        &Config::default(),
        &ModelManager::new(),
        "system",
        None,
        None,
        false,
    )
    .await
    .unwrap();
    assert!(serde_json::to_string(&result[0])
        .unwrap()
        .contains("base64,first"));
    assert!(serde_json::to_string(&result[2])
        .unwrap()
        .contains("base64,second"));
}

fn mock_api(responses: Vec<Value>) -> (Config, std::thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut config = Config::default();
    config.model.api.endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    config.model.api.api_key = "local-test-placeholder".into();
    config.model.api.max_output_tokens = 1024;
    config.model.api.request_format = "chat_completions".into();
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for response in responses {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(20)))
                .unwrap();
            let mut reader = BufReader::new(stream);
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            requests.push(serde_json::from_slice(&body).unwrap());
            let body = response.to_string();
            write!(reader.get_mut(), "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        }
        requests
    });
    (config, server)
}

#[tokio::test]
async fn test_truncated_reply_retains_executed_write_without_replaying_it() {
    let dir = std::env::temp_dir().join(next_background_task_id());
    fs::create_dir_all(&dir).unwrap();
    let call = json!({"id":"write-once","type":"function","function":{"name":"Write","arguments":json!({"path":"result.txt","content":"once","append":true}).to_string()}});
    let (mut config, server) = mock_api(vec![
        json!({"choices":[{"finish_reason":"tool_calls","message":{"tool_calls":[call]}}]}),
        json!({"choices":[{"finish_reason":"length","message":{"content":"Written. "}}]}),
        json!({"choices":[{"finish_reason":"stop","message":{"content":"Verified."}}]}),
    ]);
    config.tools.mode = "whitelist".into();
    config.tools.allowed_dirs = vec![dir.display().to_string()];
    let result = ApiClient::new(&config.model.api)
        .chat_with_tools("system", "write once", None, vec![])
        .await
        .unwrap();
    let final_result = run_tool_loop(
        &StorageManager::new(),
        &config,
        &ModelManager::new(),
        &SkillManager::new(),
        "system",
        result,
        &[],
        &None,
        Some(&dir),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(final_result.response, "Written. Verified.");
    assert_eq!(fs::read_to_string(dir.join("result.txt")).unwrap(), "once");
    let requests = server.join().unwrap();
    assert_eq!(requests[2]["max_tokens"], 1024);
    let messages = requests[2]["messages"].as_array().unwrap();
    assert_eq!(
        messages
            .iter()
            .filter(|m| m["tool_call_id"] == "write-once")
            .count(),
        1
    );
    assert!(messages
        .iter()
        .any(|m| m["role"] == "tool" && m["content"].as_str().unwrap().contains("result.txt")));
}

#[tokio::test]
async fn test_truncated_tool_arguments_are_never_dispatched() {
    let (config, server) = mock_api(vec![
        json!({"choices":[{"finish_reason":"length","message":{"tool_calls":[{"id":"a","type":"function","function":{"name":"Write","arguments":"{\"path\":"}}]}}]}),
    ]);
    let result = ApiClient::new(&config.model.api)
        .chat_with_tools("system", "write", None, vec![])
        .await;
    assert!(matches!(result, Err(error) if error.contains("MODEL_OUTPUT_TRUNCATED")));
    assert_eq!(server.join().unwrap().len(), 1);
}

#[tokio::test]
async fn test_tool_output_compaction_is_not_undone_by_multimodal_restore() {
    let mut config = Config::default();
    config.storage.max_context_tokens = 32_000;
    config.model.api.max_output_tokens = 1024;
    let messages: Vec<Message> = serde_json::from_value(json!([
        {"role":"user","content":[{"type":"text","text":"inspect"},{"type":"image_url","image_url":{"url":"data:image/png;base64,AA=="}}]},
        {"role":"assistant","tool_calls":[{"id":"a","type":"function","function":{"name":"Read","arguments":"{}"}}]},
        {"role":"tool","tool_call_id":"a","content":"x".repeat(200_000)}
    ])).unwrap();
    let result = prepare_api_messages(
        messages,
        &config,
        &ModelManager::new(),
        "system",
        None,
        None,
        true,
    )
    .await
    .unwrap();
    assert!(result.last().unwrap().history().content.len() < 20_000);
    assert!(serde_json::to_string(&result[0])
        .unwrap()
        .contains("image_url"));
    assert_eq!(
        result.last().unwrap().history().tool_call_id.as_deref(),
        Some("a")
    );
}

#[tokio::test]
async fn test_parallel_reads_preserve_order_and_write_barriers() {
    let dir = std::env::temp_dir().join(next_background_task_id());
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.txt"), "before").unwrap();
    fs::write(dir.join("b.txt"), "second").unwrap();
    let make_call = |id: &str, name: &str, args: Value| json!({"id":id,"type":"function","function":{"name":name,"arguments":args.to_string()}});
    let calls = json!([
        make_call("1", "Read", json!({"path":"a.txt"})),
        make_call("2", "Read", json!({"path":"b.txt"})),
        make_call("3", "Write", json!({"path":"a.txt","content":"after"})),
        make_call("4", "Read", json!({"path":"a.txt"}))
    ]);
    let (mut config, server) = mock_api(vec![
        json!({"choices":[{"finish_reason":"stop","message":{"content":"done"}}]}),
    ]);
    config.tools.mode = "whitelist".into();
    config.tools.allowed_dirs = vec![dir.display().to_string()];
    let result = ChatWithToolsResult::ToolCalls {
        calls: serde_json::from_value(calls.clone()).unwrap(),
        messages: serde_json::from_value(json!([{"role":"assistant","tool_calls":calls}])).unwrap(),
    };
    run_tool_loop(
        &StorageManager::new(),
        &config,
        &ModelManager::new(),
        &SkillManager::new(),
        "system",
        result,
        &[],
        &None,
        Some(&dir),
        None,
        None,
    )
    .await
    .unwrap();
    let requests = server.join().unwrap();
    let outputs: Vec<_> = requests[0]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .collect();
    assert_eq!(
        outputs
            .iter()
            .map(|m| m["tool_call_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["1", "2", "3", "4"]
    );
    assert_eq!(outputs[0]["content"], "before");
    assert_eq!(outputs[1]["content"], "second");
    assert_eq!(outputs[3]["content"], "after");
}

#[tokio::test]
async fn test_repeated_semantic_compaction_keeps_original_constraints() {
    let summary = json!({"choices":[{"finish_reason":"stop","message":{"content":"The earlier file was inspected; work remains unverified."}}]});
    let (config, server) = mock_api(vec![summary.clone(), summary]);
    let mut history = vec![history_message(
        "user",
        "Never modify billing.csv. Use Chinese in the final answer.".into(),
    )];
    for _ in 0..20 {
        history.push(history_message("assistant", "Inspected a file.".into()));
    }
    let mut compacted = prepare_history(
        Some(history),
        "system",
        "continue",
        &config,
        &ModelManager::new(),
        None,
        None,
        true,
    )
    .await
    .unwrap()
    .unwrap();
    for _ in 0..10 {
        compacted.push(history_message("assistant", "More inspection.".into()));
    }
    let twice = prepare_history(
        Some(compacted),
        "system",
        "continue",
        &config,
        &ModelManager::new(),
        None,
        None,
        true,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(twice.iter().any(|m| m.role == "user"
        && m.content
            .contains("Never modify billing.csv. Use Chinese in the final answer.")));
    let requests = server.join().unwrap();
    assert!(requests.iter().all(|request| request["tools"].is_null()));
}

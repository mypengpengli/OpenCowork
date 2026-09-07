use super::{internal_error, ApiError, ChatRequest, ChatResponse, ShellState};
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_core::Stream;
use opencowork_app::AppEvent;
use opencowork_runtime::{ContentBlock, ConversationMessage, ProcessTree, Session, SessionStore};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::io::{Read, Write};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

pub(super) type TurnRegistry = Arc<Mutex<BTreeMap<String, ActiveTurn>>>;

pub(super) struct ActiveTurn {
    pub(super) session_id: String,
    cancel: watch::Sender<bool>,
}

struct TurnLease {
    turns: TurnRegistry,
    id: String,
}

impl Drop for TurnLease {
    fn drop(&mut self) {
        if let Ok(mut turns) = self.turns.lock() {
            turns.remove(&self.id);
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Message {
    Started { turn_id: String, session_id: String },
    Event { event: AppEvent },
    Complete { response: ChatResponse },
    Error { message: String },
}

#[derive(Serialize, Deserialize)]
struct WorkerInput {
    payload: ChatRequest,
    session: Session,
}

struct ResponseStream(mpsc::Receiver<Bytes>);

impl Stream for ResponseStream {
    type Item = Result<Bytes, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_recv(cx).map(|item| item.map(Ok))
    }
}

fn encode(message: &Message) -> Bytes {
    // All protocol fields are JSON-safe Rust primitives / serde values.
    let mut bytes = serde_json::to_vec(message).expect("serialize chat protocol");
    bytes.push(b'\n');
    Bytes::from(bytes)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

pub(super) async fn stream_turn(
    State(state): State<ShellState>,
    Json(mut payload): Json<ChatRequest>,
) -> Result<Response, ApiError> {
    if payload.input.trim().is_empty() {
        return Err(ApiError::bad_request("input must not be empty"));
    }
    let turn_id = payload
        .turn_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let session_id = payload
        .session_id
        .clone()
        .unwrap_or_else(|| format!("session-{}", Uuid::new_v4()));
    if !valid_id(&turn_id) || !valid_id(&session_id) {
        return Err(ApiError::bad_request("invalid turn or session ID"));
    }
    let (cancel_tx, cancel_rx) = watch::channel(false);
    {
        let mut turns = state.turns.lock().map_err(internal_error)?;
        if turns.contains_key(&turn_id) || turns.values().any(|turn| turn.session_id == session_id)
        {
            return Err(ApiError {
                status: StatusCode::CONFLICT,
                message: "This session already has a running turn.".into(),
            });
        }
        turns.insert(
            turn_id.clone(),
            ActiveTurn {
                session_id: session_id.clone(),
                cancel: cancel_tx,
            },
        );
    }
    let lease = TurnLease {
        turns: Arc::clone(&state.turns),
        id: turn_id.clone(),
    };
    let store = SessionStore::new(state.config_home.join("sessions"));
    let existing = payload.session_id.is_some();
    let load_store = store.clone();
    let load_id = session_id.clone();
    let session = tokio::task::spawn_blocking(move || {
        if existing {
            load_store.load(&load_id)
        } else {
            Ok(Session::new())
        }
    })
    .await;
    let session = match session {
        Ok(Ok(session)) => session,
        result => {
            return Err(ApiError::bad_request(format!(
                "Could not load session: {result:?}"
            )));
        }
    };
    payload.session_id = Some(session_id.clone());
    let (sender, receiver) = mpsc::channel(128);
    tokio::spawn(drive_turn(
        state,
        turn_id,
        session_id,
        WorkerInput { payload, session },
        store,
        sender,
        cancel_rx,
        lease,
    ));
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-ndjson; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(ResponseStream(receiver)),
    )
        .into_response())
}

pub(super) async fn cancel_turn(
    State(state): State<ShellState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let turns = state.turns.lock().map_err(internal_error)?;
    // Idempotent, including a completion racing with the stop button.
    let requested = turns
        .get(&id)
        .is_some_and(|turn| turn.cancel.send(true).is_ok());
    Ok(Json(serde_json::json!({ "requested": requested })))
}

async fn drive_turn(
    state: ShellState,
    turn_id: String,
    session_id: String,
    input: WorkerInput,
    store: SessionStore,
    sender: mpsc::Sender<Bytes>,
    mut cancel: watch::Receiver<bool>,
    lease: TurnLease,
) {
    let mut progress = PartialTurn::new(input.session.clone(), &input.payload.input);
    let result = execute_worker(
        &state,
        &turn_id,
        &session_id,
        input,
        &sender,
        &mut cancel,
        &mut progress,
    )
    .await;
    let mut response = match result {
        Ok(response) => response,
        Err(message) => {
            let cancelled = *cancel.borrow() || sender.is_closed();
            progress.finish(&message);
            ChatResponse {
                session_id: session_id.clone(),
                session: progress.session,
                events: progress.events,
                iterations: progress.iterations,
                estimated_prompt_tokens: 0,
                compacted: false,
                status: if cancelled { "cancelled" } else { "failed" }.into(),
                error: Some(message),
            }
        }
    };
    response.session = response.session.with_id(&session_id);
    let save_id = session_id.clone();
    let save_session = response.session.clone();
    let saved =
        tokio::task::spawn_blocking(move || store.save_named(&save_id, &save_session)).await;
    drop(lease);
    let message = match saved {
        Ok(Ok(_)) => Message::Complete { response },
        error => Message::Error {
            message: format!("Could not save session: {error:?}"),
        },
    };
    let _ = tokio::time::timeout(Duration::from_secs(2), sender.send(encode(&message))).await;
}

async fn execute_worker(
    state: &ShellState,
    turn_id: &str,
    session_id: &str,
    input: WorkerInput,
    sender: &mpsc::Sender<Bytes>,
    cancel: &mut watch::Receiver<bool>,
    progress: &mut PartialTurn,
) -> Result<ChatResponse, String> {
    if *cancel.borrow() {
        return Err("Turn stopped.".into());
    }
    let mut command =
        tokio::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command
        .arg("--chat-worker")
        .current_dir(&state.cwd)
        .env("OPENCOWORK_CONFIG_HOME", &state.config_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let mut tree = ProcessTree::attach(child.id().ok_or("Worker process exited")?)
        .map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or("Worker stdin missing")?;
    let mut lines = BufReader::new(child.stdout.take().ok_or("Worker stdout missing")?).lines();
    let encoded = serde_json::to_vec(&input).map_err(|e| e.to_string())?;
    stdin.write_all(&encoded).await.map_err(|e| e.to_string())?;
    drop(stdin); // Worker waits for EOF before initializing providers or tools.
    sender
        .send(encode(&Message::Started {
            turn_id: turn_id.into(),
            session_id: session_id.into(),
        }))
        .await
        .map_err(|_| "Client disconnected".to_string())?;
    let result = loop {
        tokio::select! {
            biased;
            _ = cancel.changed() => break Err("Turn stopped.".into()),
            _ = sender.closed() => break Err("Client disconnected; turn stopped.".into()),
            line = lines.next_line() => {
                let message: Message = match line {
                    Ok(Some(line)) => match serde_json::from_str(&line) {
                        Ok(message) => message,
                        Err(error) => break Err(format!("Invalid worker event: {error}")),
                    },
                    Ok(None) => break Err("Worker exited before completing the turn.".into()),
                    Err(error) => break Err(error.to_string()),
                };
                match message {
                    Message::Complete { response } => break Ok(response),
                    Message::Error { message } => break Err(message),
                    Message::Event { ref event } => progress.apply(event.clone()),
                    Message::Started { .. } => {},
                }
                tokio::select! {
                    _ = cancel.changed() => break Err("Turn stopped.".into()),
                    result = sender.send(encode(&message)) => {
                        if result.is_err() { break Err("Client disconnected; turn stopped.".into()); }
                    }
                }
            }
        }
    };
    if result.is_ok() {
        // The terminal response need not wait for the existing background memory
        // refresh. Keep its worker alive, but bound cleanup if it gets stuck.
        tokio::spawn(async move {
            if matches!(tokio::time::timeout(Duration::from_secs(70), child.wait()).await, Ok(Ok(status)) if status.success())
            {
                let _ = tree.disarm();
            } else {
                let _ = tree.terminate();
                let _ = child.wait().await;
            }
        });
    } else {
        // The private job/group includes commands, MCP children and descendants.
        let _ = tree.terminate();
        let _ = child.wait().await;
    }
    result
}

pub(super) fn worker_main() -> Result<(), String> {
    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|e| e.to_string())?;
    let input: WorkerInput = serde_json::from_str(&buffer).map_err(|e| e.to_string())?;
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let write_message = |message: Message| {
        let mut output = std::io::stdout().lock();
        output.write_all(&encode(&message))?;
        output.flush()
    };
    let mut stream_error = None;
    let result = super::run_chat_blocking(&cwd, input.payload, input.session, &mut |event| {
        if let Err(error) = write_message(Message::Event { event }) {
            stream_error = Some(error.to_string());
        }
    });
    if let Some(error) = stream_error {
        return Err(error);
    }
    let memory_session = result
        .as_ref()
        .ok()
        .map(|response| response.session.clone());
    write_message(match result {
        Ok(response) => Message::Complete { response },
        Err(message) => Message::Error { message },
    })
    .map_err(|e| e.to_string())?;
    if let Some(mut session) = memory_session {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while session
            .session_memory_state
            .extraction_started_at_unix_ms
            .is_some()
            && std::time::Instant::now() < deadline
        {
            opencowork_runtime::wait_for_session_memory_refresh(
                &mut session,
                &cwd,
                &opencowork_runtime::default_config_home(),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

struct PartialTurn {
    session: Session,
    pending: Vec<ContentBlock>,
    usage: Option<opencowork_runtime::TokenUsage>,
    events: Vec<AppEvent>,
    iterations: usize,
    unresolved: BTreeMap<String, String>,
}

impl PartialTurn {
    fn new(mut session: Session, input: &str) -> Self {
        session.messages.push(ConversationMessage::user(input));
        Self {
            session,
            pending: vec![],
            usage: None,
            events: vec![],
            iterations: 0,
            unresolved: BTreeMap::new(),
        }
    }

    fn flush(&mut self) {
        if !self.pending.is_empty() {
            self.session.messages.push(ConversationMessage::assistant(
                std::mem::take(&mut self.pending),
                self.usage.take(),
            ));
        }
    }

    fn apply(&mut self, event: AppEvent) {
        match &event {
            AppEvent::AssistantTextDelta { text } => {
                if let Some(ContentBlock::Text { text: previous }) = self.pending.last_mut() {
                    previous.push_str(text);
                } else {
                    self.pending.push(ContentBlock::Text { text: text.clone() });
                }
            }
            AppEvent::ToolCall {
                id, name, input, ..
            } => {
                self.unresolved.insert(id.clone(), name.clone());
                self.pending.push(ContentBlock::ToolUse {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });
            }
            AppEvent::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            } => {
                self.flush();
                self.unresolved.remove(tool_use_id);
                self.session.messages.push(ConversationMessage::tool_result(
                    tool_use_id,
                    tool_name,
                    output,
                    *is_error,
                ));
            }
            AppEvent::Usage { usage } => self.usage = Some(*usage),
            AppEvent::MessageStop => {
                self.iterations += 1;
                self.flush();
            }
        }
        self.events.push(event);
    }

    fn finish(&mut self, reason: &str) {
        self.flush();
        for (id, name) in std::mem::take(&mut self.unresolved) {
            self.session
                .messages
                .push(ConversationMessage::tool_result(id, name, reason, true));
        }
        self.session
            .messages
            .push(ConversationMessage::system(format!(
                "The preceding turn did not complete: {reason}"
            )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interruption_preserves_text_and_pairs_pending_tools() {
        let mut turn = PartialTurn::new(Session::new(), "inspect");
        turn.apply(AppEvent::AssistantTextDelta {
            text: "Checking".into(),
        });
        turn.apply(AppEvent::ToolCall {
            id: "call-1".into(),
            name: "bash".into(),
            input: "{}".into(),
            required_permission: "full".into(),
        });
        turn.finish("Stopped");
        assert_eq!(turn.session.messages[1].first_text(), Some("Checking"));
        assert!(
            matches!(&turn.session.messages[2].blocks[0], ContentBlock::ToolResult { tool_use_id, is_error: true, .. } if tool_use_id == "call-1")
        );
    }

    #[test]
    fn session_ids_cannot_escape_storage() {
        for id in ["", "../secret", "C:\\secret", "a/b", "a.json"] {
            assert!(!valid_id(id));
        }
        assert!(valid_id("session-abc_123"));
    }
}

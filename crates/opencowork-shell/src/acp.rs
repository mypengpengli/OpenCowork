use opencowork_app::AppRuntime;
use opencowork_runtime::{MessageRole, Session, SessionStore};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";
const DISCORD_COMMAND_PREFIX: &str = "!oc";
const DEFAULT_DISCORD_TOKEN_ENV: &str = "DISCORD_BOT_TOKEN";
const DEFAULT_DISCORD_POLL_MS: u64 = 5_000;
const MAX_DISCORD_MESSAGE_LEN: usize = 1_900;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpOverviewView {
    pub settings: AcpSettingsView,
    pub discord_status: DiscordStatusView,
    pub threads: Vec<AcpThreadView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpSettingsView {
    pub default_cwd: String,
    pub codex_args_text: String,
    pub claude_args_text: String,
    pub channels: Vec<AcpChannelConfigView>,
    pub discord_enabled: bool,
    pub discord_token: String,
    pub discord_token_env: String,
    pub discord_guild_id: String,
    pub discord_poll_ms: u64,
    pub command_prefix: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpChannelConfigView {
    pub channel_type: String,
    pub label: String,
    pub enabled: bool,
    pub config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiscordStatusView {
    pub enabled: bool,
    pub running: bool,
    pub guild_id: Option<String>,
    pub bot_user_id: Option<String>,
    pub bot_username: Option<String>,
    pub poll_ms: u64,
    pub scanned_channels: usize,
    pub last_poll_started_at_unix_ms: Option<u128>,
    pub last_poll_completed_at_unix_ms: Option<u128>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpThreadView {
    pub id: String,
    pub title: String,
    pub runtime: String,
    pub channel_kind: String,
    pub channel_id: String,
    pub channel_name: Option<String>,
    pub cwd: String,
    pub provider_session_id: Option<String>,
    pub status: String,
    pub last_error: Option<String>,
    pub last_result_preview: Option<String>,
    pub created_at_unix_ms: u128,
    pub updated_at_unix_ms: u128,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpSettingsUpdateRequest {
    pub default_cwd: Option<String>,
    pub codex_args_text: Option<String>,
    pub claude_args_text: Option<String>,
    pub channels: Option<Vec<AcpChannelConfigUpdateRequest>>,
    pub discord_enabled: Option<bool>,
    pub discord_token: Option<String>,
    pub discord_token_env: Option<String>,
    pub discord_guild_id: Option<String>,
    pub discord_poll_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpChannelConfigUpdateRequest {
    pub channel_type: String,
    pub label: Option<String>,
    pub config: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpThreadCreateRequest {
    pub title: Option<String>,
    pub runtime: String,
    pub channel_id: String,
    pub channel_name: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AcpRuntimeKind {
    Codex,
    Claude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AcpThreadStatus {
    Idle,
    Running,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AcpChannelType {
    Discord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcpThreadRecord {
    id: String,
    title: String,
    runtime: AcpRuntimeKind,
    channel_kind: String,
    channel_id: String,
    channel_name: Option<String>,
    cwd: String,
    provider_session_id: Option<String>,
    status: AcpThreadStatus,
    last_error: Option<String>,
    last_result_preview: Option<String>,
    created_at_unix_ms: u128,
    updated_at_unix_ms: u128,
}

#[derive(Debug, Clone, Default)]
struct AcpSettings {
    default_cwd: String,
    codex_args: Vec<String>,
    claude_args: Vec<String>,
    channels: Vec<AcpChannelConfig>,
}

#[derive(Debug, Clone)]
struct AcpChannelConfig {
    channel_type: AcpChannelType,
    label: String,
    settings: AcpChannelSettings,
}

#[derive(Debug, Clone)]
enum AcpChannelSettings {
    Discord(DiscordSettings),
}

#[derive(Debug, Clone)]
struct DiscordSettings {
    enabled: bool,
    token: String,
    token_env: String,
    guild_id: String,
    poll_ms: u64,
}

impl Default for DiscordSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            token: String::new(),
            token_env: String::new(),
            guild_id: String::new(),
            poll_ms: DEFAULT_DISCORD_POLL_MS,
        }
    }
}

#[derive(Debug, Clone)]
struct AcpRunOutput {
    provider_session_id: Option<String>,
    text: String,
    is_error: bool,
}

#[derive(Clone)]
pub struct AcpCoordinator {
    cwd: PathBuf,
    config_home: PathBuf,
    shared: Arc<AcpSharedState>,
}

#[derive(Default)]
struct AcpSharedState {
    file_lock: Mutex<()>,
    running_threads: Mutex<HashSet<String>>,
    discord_status: Mutex<DiscordStatusView>,
}

#[derive(Debug, Clone, Deserialize)]
struct DiscordChannel {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "type")]
    channel_type: u8,
}

#[derive(Debug, Clone, Deserialize)]
struct DiscordActiveThreadsResponse {
    #[serde(default)]
    threads: Vec<DiscordChannel>,
}

#[derive(Debug, Clone, Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
    #[serde(default)]
    bot: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct DiscordMessage {
    id: String,
    #[serde(default)]
    content: String,
    author: DiscordUser,
}

#[derive(Clone)]
struct DiscordRestClient {
    client: Client,
    token: String,
}

enum DiscordPromptMode {
    Acp(AcpThreadRecord),
    Assistant,
}

impl AcpCoordinator {
    pub fn new(cwd: impl Into<PathBuf>, config_home: impl Into<PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            config_home: config_home.into(),
            shared: Arc::new(AcpSharedState::default()),
        }
    }

    pub fn spawn_background_worker(&self) {
        let this = self.clone();
        let _ = thread::Builder::new()
            .name("opencowork-acp-discord".to_string())
            .spawn(move || this.discord_loop());
    }

    pub fn overview(&self, settings: &Value) -> Result<AcpOverviewView, String> {
        let parsed = AcpSettings::from_value(settings, &self.cwd);
        let status = self
            .shared
            .discord_status
            .lock()
            .map_err(|error| error.to_string())?
            .clone();
        Ok(AcpOverviewView {
            settings: parsed.to_view(),
            discord_status: status,
            threads: self.list_thread_views()?,
        })
    }

    pub fn apply_settings_update(
        &self,
        settings: &mut Value,
        payload: &AcpSettingsUpdateRequest,
    ) -> Result<(), String> {
        let current = AcpSettings::from_value(settings, &self.cwd);
        let root = object_mut(settings);
        let shell = object_mut(
            root.entry("shell".to_string())
                .or_insert_with(|| Value::Object(Map::new())),
        );
        let acp = object_mut(
            shell
                .entry("acp".to_string())
                .or_insert_with(|| Value::Object(Map::new())),
        );

        acp.insert(
            "defaultCwd".to_string(),
            Value::String(
                payload
                    .default_cwd
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .to_string(),
            ),
        );
        acp.insert(
            "codexArgs".to_string(),
            Value::Array(
                parse_args_text(payload.codex_args_text.as_deref())
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );
        acp.insert(
            "claudeArgs".to_string(),
            Value::Array(
                parse_args_text(payload.claude_args_text.as_deref())
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );

        let channels = if let Some(items) = payload.channels.as_deref() {
            parse_channel_update_requests(items)?
        } else {
            merge_legacy_channel_update(&current.channels, payload)
        };
        acp.insert(
            "channels".to_string(),
            Value::Array(channels.iter().map(AcpChannelConfig::to_value).collect()),
        );
        sync_legacy_discord_settings(acp, &channels);
        Ok(())
    }

    pub fn create_thread(
        &self,
        settings: &Value,
        payload: &AcpThreadCreateRequest,
    ) -> Result<(), String> {
        let parsed = AcpSettings::from_value(settings, &self.cwd);
        let runtime = parse_runtime(&payload.runtime)?;
        let channel_id = payload.channel_id.trim();
        if channel_id.is_empty() {
            return Err("channel id must not be empty".to_string());
        }
        let cwd = payload
            .cwd
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(&parsed.default_cwd);
        let title = payload
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                payload
                    .channel_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or(channel_id);

        let channel_name = payload
            .channel_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);

        self.bind_or_create_thread(channel_id, channel_name, title, runtime, cwd)
    }

    pub fn reset_thread(&self, thread_id: &str) -> Result<(), String> {
        let _guard = self.lock_files()?;
        let mut record = self.load_thread(thread_id)?;
        record.provider_session_id = match record.runtime {
            AcpRuntimeKind::Codex => None,
            AcpRuntimeKind::Claude => Some(Uuid::new_v4().to_string()),
        };
        record.status = AcpThreadStatus::Idle;
        record.last_error = None;
        record.last_result_preview = None;
        record.updated_at_unix_ms = now_ms();
        self.save_thread(&record)
    }

    pub fn delete_thread(&self, thread_id: &str) -> Result<(), String> {
        let _guard = self.lock_files()?;
        let path = self.thread_path(thread_id);
        if !path.exists() {
            return Err(format!("ACP thread `{thread_id}` was not found"));
        }
        fs::remove_file(path).map_err(|error| error.to_string())
    }

    fn discord_loop(self) {
        loop {
            let settings_value = match self.read_settings() {
                Ok(value) => value,
                Err(error) => {
                    self.set_discord_error(error);
                    thread::sleep(Duration::from_millis(DEFAULT_DISCORD_POLL_MS));
                    continue;
                }
            };
            let settings = AcpSettings::from_value(&settings_value, &self.cwd);
            let discord = settings.discord_settings();
            if !discord.enabled {
                self.update_discord_status(|status| {
                    status.enabled = false;
                    status.running = false;
                    status.guild_id = None;
                    status.bot_user_id = None;
                    status.bot_username = None;
                    status.scanned_channels = 0;
                    status.poll_ms = discord.poll_ms;
                    status.last_error = None;
                });
                thread::sleep(Duration::from_millis(discord.poll_ms));
                continue;
            }

            let token = discord.resolved_token();
            if token.trim().is_empty() {
                self.set_discord_error(
                    "Discord bridge is enabled but no bot token is configured. Set DISCORD_BOT_TOKEN or fill the bot token field.",
                );
                thread::sleep(Duration::from_millis(discord.poll_ms));
                continue;
            }
            if discord.guild_id.trim().is_empty() {
                self.set_discord_error("Discord bridge is enabled but guild id is missing");
                thread::sleep(Duration::from_millis(discord.poll_ms));
                continue;
            }

            match self.poll_discord_once(&settings, &token) {
                Ok(()) => {}
                Err(error) => self.set_discord_error(error),
            }
            thread::sleep(Duration::from_millis(discord.poll_ms));
        }
    }

    fn poll_discord_once(&self, settings: &AcpSettings, token: &str) -> Result<(), String> {
        let discord = settings.discord_settings();
        self.update_discord_status(|status| {
            status.enabled = true;
            status.running = true;
            status.guild_id = Some(discord.guild_id.clone());
            status.poll_ms = discord.poll_ms;
            status.last_poll_started_at_unix_ms = Some(now_ms());
        });

        let client = DiscordRestClient::new(token)?;
        let me = client.current_user()?;
        let mut channels = client.guild_channels(&discord.guild_id)?;
        let mut active_threads = client.active_threads(&discord.guild_id)?;
        channels.append(&mut active_threads);
        channels.retain(|channel| is_supported_discord_channel_type(channel.channel_type));

        let mut channel_names = HashMap::new();
        for channel in &channels {
            channel_names.insert(
                channel.id.clone(),
                channel
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| channel.id.clone()),
            );
        }

        let _guard = self.lock_files()?;
        let mut cursors = self.read_discord_cursors();
        drop(_guard);

        for channel in &channels {
            let after = cursors.get(&channel.id).cloned();
            let mut messages = client.channel_messages(&channel.id, after.as_deref())?;
            if after.is_none() {
                if let Some(newest) = messages.first() {
                    cursors.insert(channel.id.clone(), newest.id.clone());
                }
                continue;
            }

            messages.reverse();
            for message in messages {
                cursors.insert(channel.id.clone(), message.id.clone());
                if message.author.bot || message.author.id == me.id {
                    continue;
                }
                let content = message.content.trim();
                if content.is_empty() {
                    continue;
                }
                if content.starts_with(DISCORD_COMMAND_PREFIX) {
                    self.handle_discord_command(
                        &client,
                        settings,
                        &channel.id,
                        channel_names.get(&channel.id).cloned(),
                        content,
                    );
                    continue;
                }
                self.handle_discord_prompt(
                    client.clone(),
                    settings.clone(),
                    &channel.id,
                    channel_names.get(&channel.id).cloned(),
                    content.to_string(),
                );
            }
        }

        let _guard = self.lock_files()?;
        self.write_discord_cursors(&cursors)?;
        drop(_guard);

        self.update_discord_status(|status| {
            status.bot_user_id = Some(me.id.clone());
            status.bot_username = Some(me.username.clone());
            status.scanned_channels = channels.len();
            status.last_poll_completed_at_unix_ms = Some(now_ms());
            status.last_error = None;
        });
        Ok(())
    }

    fn handle_discord_command(
        &self,
        client: &DiscordRestClient,
        settings: &AcpSettings,
        channel_id: &str,
        channel_name: Option<String>,
        content: &str,
    ) {
        let args = content
            .split_whitespace()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        let subcommand = args.get(1).copied().unwrap_or("help");
        let result = match subcommand {
            "help" => Ok(help_text().to_string()),
            "bind" | "new" => {
                let runtime = args.get(2).copied().unwrap_or("");
                let title = if args.len() > 3 {
                    args[3..].join(" ")
                } else {
                    channel_name.clone().unwrap_or_else(|| channel_id.to_string())
                };
                parse_runtime(runtime).and_then(|runtime| {
                    self.bind_or_create_thread(
                        channel_id,
                        channel_name.clone(),
                        title.trim(),
                        runtime,
                        &settings.default_cwd,
                    )?;
                    let thread = self
                        .thread_by_channel(channel_id)?
                        .ok_or_else(|| "failed to create ACP thread".to_string())?;
                    Ok(format!(
                        "Bound this Discord channel to `{}`.\nACP thread: `{}`\nCWD: `{}`\nSend plain messages here to continue the thread.",
                        thread.runtime.label(),
                        thread.id,
                        thread.cwd
                    ))
                })
            }
            "status" => match self.thread_by_channel(channel_id) {
                Ok(Some(thread)) => Ok(render_thread_status(&thread)),
                Ok(None) => Ok(self.render_assistant_status(channel_id)),
                Err(error) => Err(error),
            },
            "reset" => match self.thread_by_channel(channel_id) {
                Ok(Some(thread)) => {
                    if let Err(error) = self.reset_thread(&thread.id) {
                        Err(error)
                    } else {
                        Ok(format!("Reset ACP thread `{}`.", thread.id))
                    }
                }
                Ok(None) => match self.reset_assistant_session(channel_id) {
                    Ok(true) => Ok("Reset the normal assistant session for this channel.".to_string()),
                    Ok(false) => {
                        Ok("This channel is already in normal assistant mode with no saved context.".to_string())
                    }
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            },
            "unbind" | "delete" => match self.thread_by_channel(channel_id) {
                Ok(Some(thread)) => {
                    if let Err(error) = self.delete_thread(&thread.id) {
                        Err(error)
                    } else {
                        Ok(format!(
                            "Removed ACP thread `{}` from this channel.\nPlain messages here now go back to normal assistant mode.",
                            thread.id
                        ))
                    }
                }
                Ok(None) => Ok(
                    "No ACP thread is bound to this channel.\nThis channel is already using normal assistant mode."
                        .to_string(),
                ),
                Err(error) => Err(error),
            },
            other => Ok(format!("Unknown ACP command `{other}`.\n{}", help_text())),
        };

        let response = match result {
            Ok(message) => message,
            Err(error) => format!("ACP command failed: {error}"),
        };
        let _ = client.send_message(channel_id, &response);
    }

    fn handle_discord_prompt(
        &self,
        client: DiscordRestClient,
        settings: AcpSettings,
        channel_id: &str,
        channel_name: Option<String>,
        prompt: String,
    ) {
        let mode = match self.thread_by_channel(channel_id) {
            Ok(Some(thread)) => DiscordPromptMode::Acp(thread),
            Ok(None) => DiscordPromptMode::Assistant,
            Err(error) => {
                let _ = client.send_message(channel_id, &format!("ACP lookup failed: {error}"));
                return;
            }
        };
        let running_key = match &mode {
            DiscordPromptMode::Acp(thread) => thread.id.clone(),
            DiscordPromptMode::Assistant => assistant_run_key(channel_id),
        };

        let mut running = match self.shared.running_threads.lock() {
            Ok(guard) => guard,
            Err(_) => {
                let _ = client.send_message(channel_id, "ACP thread lock is poisoned.");
                return;
            }
        };
        if !running.insert(running_key.clone()) {
            let message = match mode {
                DiscordPromptMode::Acp(_) => {
                    "This ACP thread is already running. Wait for the current run to finish before sending another prompt."
                }
                DiscordPromptMode::Assistant => {
                    "This channel's normal assistant session is already running. Wait for the current reply before sending another prompt."
                }
            };
            let _ = client.send_message(channel_id, message);
            return;
        }
        drop(running);

        let coordinator = self.clone();
        let channel_id = channel_id.to_string();
        let _ = thread::Builder::new()
            .name(format!("opencowork-acp-run-{}", running_key))
            .spawn(move || {
                match mode {
                    DiscordPromptMode::Acp(thread) => {
                        coordinator.mark_thread_running(&thread.id);
                        let display_name = channel_name.unwrap_or_else(|| channel_id.clone());
                        let _ = client.send_message(
                            &channel_id,
                            &format!(
                                "Running `{}` for ACP thread `{}` in `{}`...",
                                thread.runtime.label(),
                                thread.title,
                                display_name
                            ),
                        );

                        let run = coordinator.run_thread_prompt(&thread, &settings, &prompt);
                        match run {
                            Ok(output) => {
                                if let Err(error) =
                                    coordinator.mark_thread_success(&thread.id, &output)
                                {
                                    let _ = client.send_message(
                                        &channel_id,
                                        &format!("ACP state update failed: {error}"),
                                    );
                                }
                                let text = if output.text.trim().is_empty() {
                                    "Run completed, but the CLI did not return a text response."
                                        .to_string()
                                } else {
                                    output.text
                                };
                                for chunk in discord_chunks(&text) {
                                    let _ = client.send_message(&channel_id, &chunk);
                                }
                            }
                            Err(error) => {
                                let _ = coordinator.mark_thread_error(&thread.id, &error);
                                let _ = client
                                    .send_message(&channel_id, &format!("ACP run failed: {error}"));
                            }
                        }
                    }
                    DiscordPromptMode::Assistant => {
                        match coordinator.run_assistant_prompt(&channel_id, &prompt) {
                            Ok(text) => {
                                for chunk in discord_chunks(&text) {
                                    let _ = client.send_message(&channel_id, &chunk);
                                }
                            }
                            Err(error) => {
                                let _ = client.send_message(
                                    &channel_id,
                                    &format!("Assistant run failed: {error}"),
                                );
                            }
                        }
                    }
                }

                if let Ok(mut running) = coordinator.shared.running_threads.lock() {
                    running.remove(&running_key);
                }
            });
    }

    fn run_assistant_prompt(&self, channel_id: &str, prompt: &str) -> Result<String, String> {
        crate::apply_shell_api_key_override(&self.cwd)?;
        let app = AppRuntime::load(&self.cwd).map_err(|error| error.to_string())?;
        let model = app.model().to_string();
        let store = SessionStore::new(self.assistant_sessions_root());
        let session_id = assistant_session_id(channel_id);
        let session = if self.assistant_session_path(channel_id).exists() {
            store.load(&session_id).map_err(|error| error.to_string())?
        } else {
            Session::new()
        };
        let execution = app
            .run_turn(&model, session, prompt, None)
            .map_err(|error| error.to_string())?;
        store
            .save_named(&session_id, &execution.session)
            .map_err(|error| error.to_string())?;
        Ok(
            latest_assistant_text(&execution.session).unwrap_or_else(|| {
                "The assistant completed the run, but did not return a text response.".to_string()
            }),
        )
    }

    fn run_thread_prompt(
        &self,
        thread: &AcpThreadRecord,
        settings: &AcpSettings,
        prompt: &str,
    ) -> Result<AcpRunOutput, String> {
        match thread.runtime {
            AcpRuntimeKind::Codex => self.run_codex_prompt(thread, settings, prompt),
            AcpRuntimeKind::Claude => self.run_claude_prompt(thread, settings, prompt),
        }
    }

    fn run_codex_prompt(
        &self,
        thread: &AcpThreadRecord,
        settings: &AcpSettings,
        prompt: &str,
    ) -> Result<AcpRunOutput, String> {
        let mut command = Command::new(runtime_command("codex"));
        hide_command_window(&mut command);
        command.current_dir(resolve_thread_cwd(thread));
        if let Some(session_id) = thread.provider_session_id.as_deref() {
            command.args(["exec", "resume", session_id]);
        } else {
            command.args(["exec"]);
        }
        if settings.codex_args.is_empty() {
            command.args(["-s", "workspace-write", "-a", "never"]);
        } else {
            command.args(&settings.codex_args);
        }
        command.args(["--json", "--skip-git-repo-check", "-"]);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|error| error.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .map_err(|error| error.to_string())?;
        }
        let output = child
            .wait_with_output()
            .map_err(|error| error.to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let parsed = parse_codex_output(&stdout);
        if !output.status.success() {
            let reason = if !stderr.is_empty() { stderr } else { stdout };
            return Err(compact_error(&reason));
        }
        Ok(parsed)
    }

    fn run_claude_prompt(
        &self,
        thread: &AcpThreadRecord,
        settings: &AcpSettings,
        prompt: &str,
    ) -> Result<AcpRunOutput, String> {
        let session_id = thread
            .provider_session_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let mut command = Command::new(runtime_command("claude"));
        hide_command_window(&mut command);
        command.current_dir(resolve_thread_cwd(thread));
        command.args([
            "-p",
            "--verbose",
            "--output-format",
            "stream-json",
            "--session-id",
            &session_id,
        ]);
        if settings.claude_args.is_empty() {
            command.args(["--permission-mode", "acceptEdits"]);
        } else {
            command.args(&settings.claude_args);
        }
        command.arg("-");
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|error| error.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(prompt.as_bytes())
                .map_err(|error| error.to_string())?;
        }
        let output = child
            .wait_with_output()
            .map_err(|error| error.to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let mut parsed = parse_claude_output(&stdout);
        if parsed.provider_session_id.is_none() {
            parsed.provider_session_id = Some(session_id);
        }
        if !output.status.success() || parsed.is_error {
            let reason = if !stderr.is_empty() {
                stderr
            } else if !parsed.text.trim().is_empty() {
                parsed.text.clone()
            } else {
                stdout
            };
            return Err(compact_error(&reason));
        }
        Ok(parsed)
    }

    fn bind_or_create_thread(
        &self,
        channel_id: &str,
        channel_name: Option<String>,
        title: &str,
        runtime: AcpRuntimeKind,
        cwd: &str,
    ) -> Result<(), String> {
        let _guard = self.lock_files()?;
        let existing = self.thread_by_channel_locked(channel_id)?;
        let now = now_ms();
        let mut record = if let Some(mut thread) = existing {
            if thread.runtime != runtime {
                thread.provider_session_id = match runtime {
                    AcpRuntimeKind::Codex => None,
                    AcpRuntimeKind::Claude => Some(Uuid::new_v4().to_string()),
                };
            }
            thread.runtime = runtime;
            thread.title = title.to_string();
            thread.channel_name = channel_name;
            thread.cwd = cwd.to_string();
            thread.status = AcpThreadStatus::Idle;
            thread.last_error = None;
            thread.updated_at_unix_ms = now;
            thread
        } else {
            AcpThreadRecord {
                id: format!("acp-{now}"),
                title: title.to_string(),
                runtime,
                channel_kind: "discord".to_string(),
                channel_id: channel_id.to_string(),
                channel_name,
                cwd: cwd.to_string(),
                provider_session_id: match runtime {
                    AcpRuntimeKind::Codex => None,
                    AcpRuntimeKind::Claude => Some(Uuid::new_v4().to_string()),
                },
                status: AcpThreadStatus::Idle,
                last_error: None,
                last_result_preview: None,
                created_at_unix_ms: now,
                updated_at_unix_ms: now,
            }
        };
        record.updated_at_unix_ms = now;
        self.save_thread(&record)
    }

    fn thread_by_channel(&self, channel_id: &str) -> Result<Option<AcpThreadRecord>, String> {
        let _guard = self.lock_files()?;
        self.thread_by_channel_locked(channel_id)
    }

    fn thread_by_channel_locked(
        &self,
        channel_id: &str,
    ) -> Result<Option<AcpThreadRecord>, String> {
        Ok(self
            .load_threads()?
            .into_iter()
            .find(|thread| thread.channel_id == channel_id))
    }

    fn mark_thread_running(&self, thread_id: &str) {
        let _ = self.with_thread_mut(thread_id, |thread| {
            thread.status = AcpThreadStatus::Running;
            thread.last_error = None;
            thread.updated_at_unix_ms = now_ms();
        });
    }

    fn mark_thread_success(&self, thread_id: &str, output: &AcpRunOutput) -> Result<(), String> {
        self.with_thread_mut(thread_id, |thread| {
            thread.status = AcpThreadStatus::Idle;
            if let Some(session_id) = output.provider_session_id.as_deref() {
                thread.provider_session_id = Some(session_id.to_string());
            }
            thread.last_error = None;
            thread.last_result_preview = Some(preview_text(&output.text, 240));
            thread.updated_at_unix_ms = now_ms();
        })
    }

    fn mark_thread_error(&self, thread_id: &str, error: &str) -> Result<(), String> {
        self.with_thread_mut(thread_id, |thread| {
            thread.status = AcpThreadStatus::Error;
            thread.last_error = Some(compact_error(error));
            thread.updated_at_unix_ms = now_ms();
        })
    }

    fn with_thread_mut(
        &self,
        thread_id: &str,
        mut updater: impl FnMut(&mut AcpThreadRecord),
    ) -> Result<(), String> {
        let _guard = self.lock_files()?;
        let mut thread = self.load_thread(thread_id)?;
        updater(&mut thread);
        self.save_thread(&thread)
    }

    fn list_thread_views(&self) -> Result<Vec<AcpThreadView>, String> {
        let _guard = self.lock_files()?;
        let mut threads = self
            .load_threads()?
            .into_iter()
            .map(|thread| thread.into_view())
            .collect::<Vec<_>>();
        threads.sort_by(|left, right| right.updated_at_unix_ms.cmp(&left.updated_at_unix_ms));
        Ok(threads)
    }

    fn lock_files(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        self.shared
            .file_lock
            .lock()
            .map_err(|error| error.to_string())
    }

    fn read_settings(&self) -> Result<Value, String> {
        let path = self.cwd.join(".opencowork").join("settings.json");
        match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).map_err(|error| error.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(Value::Object(Map::new()))
            }
            Err(error) => Err(error.to_string()),
        }
    }

    fn workspace_root(&self) -> PathBuf {
        self.config_home
            .join("acp")
            .join("workspaces")
            .join(encode_workspace_path(&self.cwd))
    }

    fn assistant_sessions_root(&self) -> PathBuf {
        self.workspace_root().join("channel-sessions")
    }

    fn assistant_session_path(&self, channel_id: &str) -> PathBuf {
        self.assistant_sessions_root()
            .join(format!("{}.json", assistant_session_id(channel_id)))
    }

    fn assistant_session_exists(&self, channel_id: &str) -> bool {
        self.assistant_session_path(channel_id).exists()
    }

    fn reset_assistant_session(&self, channel_id: &str) -> Result<bool, String> {
        let path = self.assistant_session_path(channel_id);
        match fs::remove_file(path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.to_string()),
        }
    }

    fn render_assistant_status(&self, channel_id: &str) -> String {
        let session = if self.assistant_session_exists(channel_id) {
            "saved channel context found"
        } else {
            "no saved context yet"
        };
        format!(
            "No ACP thread is bound to this channel.\nMode: normal assistant\nSession: {session}\nPlain messages here use the project's default assistant session for this channel.\nUse `{} bind codex` or `{} bind claude` to switch this channel to direct ACP mode.",
            DISCORD_COMMAND_PREFIX, DISCORD_COMMAND_PREFIX
        )
    }

    fn threads_root(&self) -> PathBuf {
        self.workspace_root().join("threads")
    }

    fn thread_path(&self, thread_id: &str) -> PathBuf {
        self.threads_root().join(format!("{thread_id}.json"))
    }

    fn discord_cursors_path(&self) -> PathBuf {
        self.workspace_root().join("discord-cursors.json")
    }

    fn load_threads(&self) -> Result<Vec<AcpThreadRecord>, String> {
        let root = self.threads_root();
        if !root.exists() {
            return Ok(Vec::new());
        }
        let mut threads = Vec::new();
        for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let record = serde_json::from_str::<AcpThreadRecord>(
                &fs::read_to_string(&path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            threads.push(record);
        }
        Ok(threads)
    }

    fn load_thread(&self, thread_id: &str) -> Result<AcpThreadRecord, String> {
        let path = self.thread_path(thread_id);
        let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
        serde_json::from_str(&content).map_err(|error| error.to_string())
    }

    fn save_thread(&self, thread: &AcpThreadRecord) -> Result<(), String> {
        let path = self.thread_path(&thread.id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let content = serde_json::to_string_pretty(thread).map_err(|error| error.to_string())?;
        fs::write(path, content).map_err(|error| error.to_string())
    }

    fn read_discord_cursors(&self) -> HashMap<String, String> {
        let path = self.discord_cursors_path();
        let Ok(content) = fs::read_to_string(path) else {
            return HashMap::new();
        };
        serde_json::from_str(&content).unwrap_or_default()
    }

    fn write_discord_cursors(&self, cursors: &HashMap<String, String>) -> Result<(), String> {
        let path = self.discord_cursors_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let content = serde_json::to_string_pretty(cursors).map_err(|error| error.to_string())?;
        fs::write(path, content).map_err(|error| error.to_string())
    }

    fn update_discord_status(&self, mut updater: impl FnMut(&mut DiscordStatusView)) {
        if let Ok(mut status) = self.shared.discord_status.lock() {
            updater(&mut status);
        }
    }

    fn set_discord_error(&self, error: impl Into<String>) {
        let message = error.into();
        self.update_discord_status(|status| {
            status.running = false;
            status.last_error = Some(message.clone());
            status.last_poll_completed_at_unix_ms = Some(now_ms());
        });
    }
}

impl AcpSettings {
    fn from_value(value: &Value, cwd: &Path) -> Self {
        let acp = value
            .get("shell")
            .and_then(Value::as_object)
            .and_then(|shell| shell.get("acp"))
            .and_then(Value::as_object);
        let default_cwd = acp
            .and_then(|object| object.get("defaultCwd"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| cwd.display().to_string());
        let codex_args = acp
            .and_then(|object| object.get("codexArgs"))
            .and_then(Value::as_array)
            .map(|items| read_string_array(items))
            .unwrap_or_default();
        let claude_args = acp
            .and_then(|object| object.get("claudeArgs"))
            .and_then(Value::as_array)
            .map(|items| read_string_array(items))
            .unwrap_or_default();
        let channels = read_channel_configs(acp);
        Self {
            default_cwd,
            codex_args,
            claude_args,
            channels,
        }
    }

    fn to_view(&self) -> AcpSettingsView {
        let discord = self.discord_settings();
        AcpSettingsView {
            default_cwd: self.default_cwd.clone(),
            codex_args_text: self.codex_args.join("\n"),
            claude_args_text: self.claude_args.join("\n"),
            channels: self
                .channels
                .iter()
                .map(AcpChannelConfig::to_view)
                .collect(),
            discord_enabled: discord.enabled,
            discord_token: discord.token.clone(),
            discord_token_env: discord.token_env.clone(),
            discord_guild_id: discord.guild_id.clone(),
            discord_poll_ms: discord.poll_ms,
            command_prefix: DISCORD_COMMAND_PREFIX,
        }
    }

    fn discord_settings(&self) -> DiscordSettings {
        self.channels
            .iter()
            .find_map(AcpChannelConfig::as_discord)
            .unwrap_or_default()
    }
}

impl AcpChannelType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Discord => "discord",
        }
    }

    fn default_label(self) -> &'static str {
        match self {
            Self::Discord => "Discord",
        }
    }
}

impl AcpChannelConfig {
    fn discord(settings: DiscordSettings) -> Self {
        Self::discord_with_label(
            AcpChannelType::Discord.default_label().to_string(),
            settings,
        )
    }

    fn discord_with_label(label: String, settings: DiscordSettings) -> Self {
        Self {
            channel_type: AcpChannelType::Discord,
            label,
            settings: AcpChannelSettings::Discord(settings),
        }
    }

    fn from_saved_value(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let channel_type = object
            .get("channelType")
            .and_then(Value::as_str)
            .and_then(|raw| parse_channel_type(raw).ok())?;
        let label = object
            .get("label")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(channel_type.default_label())
            .to_string();
        let enabled = object.get("enabled").and_then(Value::as_bool);
        let config = object.get("config").unwrap_or(value);
        match channel_type {
            AcpChannelType::Discord => Some(Self::discord_with_label(
                label,
                DiscordSettings::from_value(config, enabled),
            )),
        }
    }

    fn to_view(&self) -> AcpChannelConfigView {
        match &self.settings {
            AcpChannelSettings::Discord(settings) => AcpChannelConfigView {
                channel_type: self.channel_type.as_str().to_string(),
                label: self.label.clone(),
                enabled: settings.enabled,
                config: json!({
                    "token": settings.token,
                    "tokenEnv": settings.token_env,
                    "guildId": settings.guild_id,
                    "pollMs": settings.poll_ms,
                    "commandPrefix": DISCORD_COMMAND_PREFIX,
                }),
            },
        }
    }

    fn to_value(&self) -> Value {
        match &self.settings {
            AcpChannelSettings::Discord(settings) => json!({
                "channelType": self.channel_type.as_str(),
                "label": self.label,
                "enabled": settings.enabled,
                "config": {
                    "token": settings.token,
                    "tokenEnv": settings.token_env,
                    "guildId": settings.guild_id,
                    "pollMs": settings.poll_ms,
                }
            }),
        }
    }

    fn as_discord(&self) -> Option<DiscordSettings> {
        match &self.settings {
            AcpChannelSettings::Discord(settings) => Some(settings.clone()),
        }
    }
}

impl DiscordSettings {
    fn from_value(value: &Value, enabled_override: Option<bool>) -> Self {
        let object = value.as_object();
        let mut settings = Self {
            enabled: false,
            token: object
                .and_then(|object| object.get("token"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("")
                .to_string(),
            token_env: object
                .and_then(|object| object.get("tokenEnv"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("")
                .to_string(),
            guild_id: object
                .and_then(|object| object.get("guildId"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("")
                .to_string(),
            poll_ms: object
                .and_then(|object| object.get("pollMs"))
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_DISCORD_POLL_MS)
                .max(1_000),
        };
        settings.enabled = enabled_override.unwrap_or_else(|| {
            object
                .and_then(|object| object.get("enabled"))
                .and_then(Value::as_bool)
                .unwrap_or_else(|| settings.is_configured())
        });
        settings
    }

    fn is_configured(&self) -> bool {
        !self.guild_id.trim().is_empty() && !self.resolved_token().trim().is_empty()
    }

    fn resolved_token(&self) -> String {
        if let Ok(value) = env::var(DEFAULT_DISCORD_TOKEN_ENV) {
            if !value.trim().is_empty() {
                return value.trim().to_string();
            }
        }
        if !self.token_env.trim().is_empty() {
            if let Ok(value) = env::var(self.token_env.trim()) {
                if !value.trim().is_empty() {
                    return value.trim().to_string();
                }
            }
        }
        self.token.trim().to_string()
    }
}

fn parse_channel_type(value: &str) -> Result<AcpChannelType, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "discord" => Ok(AcpChannelType::Discord),
        _ => Err(format!("unsupported channel type `{value}`")),
    }
}

fn read_channel_configs(acp: Option<&Map<String, Value>>) -> Vec<AcpChannelConfig> {
    let mut channels = acp
        .and_then(|object| object.get("channels"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(AcpChannelConfig::from_saved_value)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if channels.is_empty() {
        let legacy = acp
            .and_then(|object| object.get("discord"))
            .map(|value| DiscordSettings::from_value(value, None))
            .unwrap_or_default();
        channels.push(AcpChannelConfig::discord(legacy));
    }
    channels
}

fn parse_channel_update_requests(
    items: &[AcpChannelConfigUpdateRequest],
) -> Result<Vec<AcpChannelConfig>, String> {
    let mut channels = Vec::new();
    for item in items {
        let channel_type = parse_channel_type(&item.channel_type)?;
        let label = item
            .label
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(channel_type.default_label())
            .to_string();
        let config = item.config.as_ref().unwrap_or(&Value::Null);
        match channel_type {
            AcpChannelType::Discord => channels.push(AcpChannelConfig::discord_with_label(
                label,
                DiscordSettings::from_value(config, None),
            )),
        }
    }
    if channels.is_empty() {
        channels.push(AcpChannelConfig::discord(DiscordSettings::default()));
    }
    Ok(channels)
}

fn merge_legacy_channel_update(
    current: &[AcpChannelConfig],
    payload: &AcpSettingsUpdateRequest,
) -> Vec<AcpChannelConfig> {
    let mut channels = if current.is_empty() {
        vec![AcpChannelConfig::discord(DiscordSettings::default())]
    } else {
        current.to_vec()
    };
    let mut discord = channels
        .iter()
        .find_map(AcpChannelConfig::as_discord)
        .unwrap_or_default();
    if let Some(token) = payload.discord_token.as_deref() {
        discord.token = token.trim().to_string();
    }
    if let Some(token_env) = payload.discord_token_env.as_deref() {
        discord.token_env = token_env.trim().to_string();
    }
    if let Some(guild_id) = payload.discord_guild_id.as_deref() {
        discord.guild_id = guild_id.trim().to_string();
    }
    if let Some(poll_ms) = payload.discord_poll_ms {
        discord.poll_ms = poll_ms.max(1_000);
    }
    discord.enabled = payload
        .discord_enabled
        .unwrap_or_else(|| discord.is_configured());

    let label = channels
        .iter()
        .find(|channel| channel.channel_type == AcpChannelType::Discord)
        .map(|channel| channel.label.clone())
        .unwrap_or_else(|| AcpChannelType::Discord.default_label().to_string());
    let updated = AcpChannelConfig::discord_with_label(label, discord);
    if let Some(existing) = channels
        .iter_mut()
        .find(|channel| channel.channel_type == AcpChannelType::Discord)
    {
        *existing = updated;
    } else {
        channels.push(updated);
    }
    channels
}

fn sync_legacy_discord_settings(acp: &mut Map<String, Value>, channels: &[AcpChannelConfig]) {
    let Some(discord) = channels.iter().find_map(AcpChannelConfig::as_discord) else {
        acp.remove("discord");
        return;
    };
    acp.insert(
        "discord".to_string(),
        json!({
            "enabled": discord.enabled,
            "token": discord.token,
            "tokenEnv": discord.token_env,
            "guildId": discord.guild_id,
            "pollMs": discord.poll_ms,
        }),
    );
}

impl AcpRuntimeKind {
    fn label(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
}

impl AcpThreadStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Running => "running",
            Self::Error => "error",
        }
    }
}

impl AcpThreadRecord {
    fn into_view(self) -> AcpThreadView {
        AcpThreadView {
            id: self.id,
            title: self.title,
            runtime: self.runtime.label().to_string(),
            channel_kind: self.channel_kind,
            channel_id: self.channel_id,
            channel_name: self.channel_name,
            cwd: self.cwd,
            provider_session_id: self.provider_session_id,
            status: self.status.label().to_string(),
            last_error: self.last_error,
            last_result_preview: self.last_result_preview,
            created_at_unix_ms: self.created_at_unix_ms,
            updated_at_unix_ms: self.updated_at_unix_ms,
        }
    }
}

impl DiscordRestClient {
    fn new(token: &str) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            client,
            token: token.to_string(),
        })
    }

    fn current_user(&self) -> Result<DiscordUser, String> {
        self.get_json("/users/@me")
    }

    fn guild_channels(&self, guild_id: &str) -> Result<Vec<DiscordChannel>, String> {
        self.get_json(&format!("/guilds/{guild_id}/channels"))
    }

    fn active_threads(&self, guild_id: &str) -> Result<Vec<DiscordChannel>, String> {
        let response = self.get_json::<DiscordActiveThreadsResponse>(&format!(
            "/guilds/{guild_id}/threads/active"
        ))?;
        Ok(response.threads)
    }

    fn channel_messages(
        &self,
        channel_id: &str,
        after: Option<&str>,
    ) -> Result<Vec<DiscordMessage>, String> {
        let url = if let Some(after) = after {
            format!("/channels/{channel_id}/messages?after={after}&limit=50")
        } else {
            format!("/channels/{channel_id}/messages?limit=1")
        };
        self.get_json(&url)
    }

    fn send_message(&self, channel_id: &str, content: &str) -> Result<(), String> {
        self.post_json(
            &format!("/channels/{channel_id}/messages"),
            &json!({ "content": content }),
        )
    }

    fn get_json<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, String> {
        let response = self
            .client
            .get(format!("{DISCORD_API_BASE}{path}"))
            .header("Authorization", format!("Bot {}", self.token))
            .header("User-Agent", "OpenCoWork ACP Bridge")
            .send()
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(format!("Discord API GET failed: {status} {body}"));
        }
        response.json::<T>().map_err(|error| error.to_string())
    }

    fn post_json(&self, path: &str, body: &Value) -> Result<(), String> {
        let response = self
            .client
            .post(format!("{DISCORD_API_BASE}{path}"))
            .header("Authorization", format!("Bot {}", self.token))
            .header("User-Agent", "OpenCoWork ACP Bridge")
            .json(body)
            .send()
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(format!("Discord API POST failed: {status} {body}"));
        }
        Ok(())
    }
}

fn parse_runtime(value: &str) -> Result<AcpRuntimeKind, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "codex" => Ok(AcpRuntimeKind::Codex),
        "claude" => Ok(AcpRuntimeKind::Claude),
        _ => Err("runtime must be `codex` or `claude`".to_string()),
    }
}

fn parse_args_text(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or("")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn read_string_array(values: &[Value]) -> Vec<String> {
    values
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn object_mut(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().expect("object")
}

fn is_supported_discord_channel_type(channel_type: u8) -> bool {
    matches!(channel_type, 0 | 5 | 10 | 11 | 12)
}

fn resolve_thread_cwd(thread: &AcpThreadRecord) -> PathBuf {
    PathBuf::from(&thread.cwd)
}

fn runtime_command(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.cmd")
    } else {
        base.to_string()
    }
}

fn hide_command_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

fn parse_codex_output(stdout: &str) -> AcpRunOutput {
    let mut provider_session_id = None;
    let mut final_text = String::new();
    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("thread.started") => {
                provider_session_id = value
                    .get("thread_id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
            }
            Some("item.completed") => {
                let item = value.get("item").and_then(Value::as_object);
                if item
                    .and_then(|item| item.get("type"))
                    .and_then(Value::as_str)
                    == Some("agent_message")
                {
                    if let Some(text) = item
                        .and_then(|item| item.get("text"))
                        .and_then(Value::as_str)
                    {
                        final_text = text.to_string();
                    }
                }
            }
            _ => {}
        }
    }
    AcpRunOutput {
        provider_session_id,
        text: final_text,
        is_error: false,
    }
}

fn parse_claude_output(stdout: &str) -> AcpRunOutput {
    let mut provider_session_id = None;
    let mut final_text = String::new();
    let mut is_error = false;
    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if provider_session_id.is_none() {
            provider_session_id = value
                .get("session_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
        }
        match value.get("type").and_then(Value::as_str) {
            Some("assistant") => {
                let Some(content) = value
                    .get("message")
                    .and_then(|message| message.get("content"))
                    .and_then(Value::as_array)
                else {
                    continue;
                };
                for block in content {
                    if block.get("type").and_then(Value::as_str) == Some("text") {
                        if let Some(text) = block.get("text").and_then(Value::as_str) {
                            final_text = text.to_string();
                        }
                    }
                }
            }
            Some("result") => {
                is_error = value
                    .get("is_error")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if let Some(text) = value.get("result").and_then(Value::as_str) {
                    final_text = text.to_string();
                }
            }
            _ => {}
        }
    }
    AcpRunOutput {
        provider_session_id,
        text: final_text,
        is_error,
    }
}

fn preview_text(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    normalized.chars().take(max_chars).collect::<String>() + "..."
}

fn compact_error(value: &str) -> String {
    preview_text(value, 500)
}

fn help_text() -> &'static str {
    "ACP commands:\n!oc bind codex\n!oc bind claude\n!oc status\n!oc reset\n!oc unbind\n\nUnbound channels stay in normal assistant mode and keep their own chat context. After binding, plain messages switch to the linked CLI thread. `reset` clears the current mode's context, and `unbind` switches the channel back to normal assistant mode."
}

fn render_thread_status(thread: &AcpThreadRecord) -> String {
    let mut lines = vec![
        format!("ACP thread: `{}`", thread.id),
        format!("Title: {}", thread.title),
        format!("Runtime: {}", thread.runtime.label()),
        format!("Channel: {}", thread.channel_id),
        format!("CWD: {}", thread.cwd),
        format!("Status: {}", thread.status.label()),
    ];
    if let Some(session_id) = &thread.provider_session_id {
        lines.push(format!("Provider session: {}", session_id));
    }
    if let Some(error) = &thread.last_error {
        lines.push(format!("Last error: {}", error));
    }
    lines.join("\n")
}

fn discord_chunks(value: &str) -> Vec<String> {
    let text = value.trim();
    if text.is_empty() {
        return vec!["(empty response)".to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let candidate_len = current.chars().count() + line.chars().count() + 1;
        if !current.is_empty() && candidate_len > MAX_DISCORD_MESSAGE_LEN {
            chunks.push(current.trim().to_string());
            current.clear();
        }
        if line.chars().count() > MAX_DISCORD_MESSAGE_LEN {
            let mut segment = String::new();
            for ch in line.chars() {
                segment.push(ch);
                if segment.chars().count() >= MAX_DISCORD_MESSAGE_LEN {
                    chunks.push(segment.clone());
                    segment.clear();
                }
            }
            if !segment.is_empty() {
                if !current.is_empty() {
                    current.push('\n');
                }
                current.push_str(&segment);
            }
            continue;
        }
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    if chunks.is_empty() {
        chunks.push(text.chars().take(MAX_DISCORD_MESSAGE_LEN).collect());
    }
    chunks
}

fn assistant_session_id(channel_id: &str) -> String {
    format!("discord-channel-{channel_id}")
}

fn assistant_run_key(channel_id: &str) -> String {
    format!("assistant:{channel_id}")
}

fn latest_assistant_text(session: &Session) -> Option<String> {
    session
        .messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::Assistant)
        .and_then(|message| message.first_text())
        .map(ToOwned::to_owned)
}

fn encode_workspace_path(path: &Path) -> String {
    path.to_string_lossy()
        .as_bytes()
        .iter()
        .flat_map(|byte| {
            if byte.is_ascii_alphanumeric() {
                vec![char::from(*byte)]
            } else {
                format!("_{byte:02x}").chars().collect::<Vec<_>>()
            }
        })
        .collect()
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::{
        discord_chunks, encode_workspace_path, parse_args_text, parse_channel_update_requests,
        parse_claude_output, parse_codex_output, read_channel_configs,
        AcpChannelConfigUpdateRequest,
    };
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn parses_codex_jsonl_output() {
        let output = parse_codex_output(
            r#"{"type":"thread.started","thread_id":"thread-1"}
{"type":"item.completed","item":{"type":"agent_message","text":"OK"}}"#,
        );
        assert_eq!(output.provider_session_id.as_deref(), Some("thread-1"));
        assert_eq!(output.text, "OK");
    }

    #[test]
    fn parses_claude_stream_json_output() {
        let output = parse_claude_output(
            r#"{"type":"system","subtype":"init","session_id":"session-1"}
{"type":"assistant","message":{"content":[{"type":"text","text":"Hello"}]}}
{"type":"result","subtype":"success","is_error":false,"result":"Hello","session_id":"session-1"}"#,
        );
        assert_eq!(output.provider_session_id.as_deref(), Some("session-1"));
        assert_eq!(output.text, "Hello");
        assert!(!output.is_error);
    }

    #[test]
    fn parses_multiline_args() {
        let values = parse_args_text(Some("--search\n-m\ngpt-5.4"));
        assert_eq!(values, vec!["--search", "-m", "gpt-5.4"]);
    }

    #[test]
    fn discord_chunks_respect_message_limit() {
        let chunks = discord_chunks(&"a".repeat(4_500));
        assert!(chunks.len() >= 3);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 1_900));
    }

    #[test]
    fn workspace_key_is_ascii_and_stable() {
        let key = encode_workspace_path(Path::new("d:/新建程序项目/opencowork"));
        assert!(key.is_ascii());
        assert!(key.contains("_"));
    }

    #[test]
    fn legacy_discord_settings_are_exposed_as_channel_list() {
        let settings = json!({
            "discord": {
                "enabled": true,
                "token": "bot-token",
                "guildId": "guild-1",
                "pollMs": 3000
            }
        });
        let channels = read_channel_configs(settings.as_object());
        assert_eq!(channels.len(), 1);
        let view = channels[0].to_view();
        assert_eq!(view.channel_type, "discord");
        assert_eq!(view.label, "Discord");
        assert!(view.enabled);
        assert_eq!(
            view.config.get("guildId").and_then(|value| value.as_str()),
            Some("guild-1")
        );
    }

    #[test]
    fn parses_channel_update_requests_for_discord() {
        let channels = parse_channel_update_requests(&[AcpChannelConfigUpdateRequest {
            channel_type: "discord".to_string(),
            label: Some("Discord 生产".to_string()),
            config: Some(json!({
                "token": "bot-token",
                "guildId": "guild-2",
                "pollMs": 7000
            })),
        }])
        .expect("channel update");
        assert_eq!(channels.len(), 1);
        let view = channels[0].to_view();
        assert_eq!(view.label, "Discord 生产");
        assert_eq!(
            view.config.get("guildId").and_then(|value| value.as_str()),
            Some("guild-2")
        );
        assert_eq!(
            view.config.get("pollMs").and_then(|value| value.as_u64()),
            Some(7000)
        );
    }
}

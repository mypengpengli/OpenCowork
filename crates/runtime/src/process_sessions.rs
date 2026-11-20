//! Host-owned process sessions. Commands cross an atomic, workspace-scoped inbox.
use crate::{write_json_atomic, ExclusiveLease, ProcessTree};
use notify::{RecursiveMode, Watcher};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};

const LOG_LIMIT: u64 = 16 * 1024 * 1024;
pub fn output_contains(raw: &str, expected: &str) -> bool {
    if raw.contains(expected) {
        return true;
    }
    let mut text = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            text.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    text.contains(expected)
}
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSession {
    pub id: String,
    pub session: String,
    pub turn: String,
    pub cwd: PathBuf,
    pub host: u32,
    pub command: String,
    pub pty: bool,
    pub status: String,
    pub pid: Option<u32>,
    pub created_at: u64,
    pub finished_at: Option<u64>,
    pub exit_code: Option<u32>,
    pub error: Option<String>,
    pub deadline: u64,
    pub truncated: bool,
}
#[derive(Clone)]
pub struct ProcessStore {
    home: PathBuf,
}
impl ProcessStore {
    pub fn new(home: &Path) -> Self {
        Self {
            home: home.join("process-sessions"),
        }
    }
    fn path(&self, id: &str) -> Result<PathBuf, String> {
        if id.is_empty()
            || id.len() > 160
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err("Invalid process session ID".into());
        }
        Ok(self.home.join("records").join(format!("{id}.json")))
    }
    pub fn get(&self, id: &str) -> Result<ProcessSession, String> {
        serde_json::from_slice(&fs::read(self.path(id)?).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
    pub fn list(&self) -> Vec<ProcessSession> {
        fs::read_dir(self.home.join("records"))
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|e| fs::read(e.path()).ok())
            .filter_map(|b| serde_json::from_slice(&b).ok())
            .collect()
    }
    pub fn save(&self, record: &ProcessSession) -> Result<(), String> {
        write_json_atomic(&self.path(&record.id)?, record).map_err(|e| e.to_string())
    }
    pub fn submit(&self, action: &Value) -> Result<(), String> {
        write_json_atomic(
            &self
                .home
                .join("inbox")
                .join(format!("{}.json", uuid::Uuid::new_v4())),
            action,
        )
        .map_err(|e| e.to_string())
    }
    pub fn create(
        &self,
        session: &str,
        turn: &str,
        cwd: &Path,
        host: u32,
        command: &str,
        pty: bool,
        timeout_ms: u64,
    ) -> Result<ProcessSession, String> {
        if command.is_empty() || command.len() > 64000 {
            return Err("Command must contain 1-64000 bytes".into());
        }
        let record = ProcessSession {
            id: format!("process-{}", uuid::Uuid::new_v4()),
            session: session.into(),
            turn: turn.into(),
            cwd: cwd.into(),
            host,
            command: command.into(),
            pty,
            status: "queued".into(),
            pid: None,
            created_at: now_ms(),
            finished_at: None,
            exit_code: None,
            error: None,
            deadline: now_ms() + timeout_ms.clamp(1000, 86400000),
            truncated: false,
        };
        self.save(&record)?;
        self.submit(&json!({"action":"start","id":record.id}))?;
        Ok(record)
    }
    pub fn read(
        &self,
        id: &str,
        cursor: u64,
        max_bytes: usize,
        wait_ms: u64,
    ) -> Result<Value, String> {
        let until = now_ms() + wait_ms.min(30000);
        loop {
            let record = self.get(id)?;
            let mut file = OpenOptions::new()
                .read(true)
                .open(self.home.join("logs").join(format!("{id}.log")))
                .ok();
            let length = file
                .as_ref()
                .and_then(|f| f.metadata().ok())
                .map_or(0, |m| m.len());
            if cursor > length {
                return Err("Invalid log cursor".into());
            }
            if length > cursor
                || !matches!(record.status.as_str(), "queued" | "running")
                || now_ms() >= until
            {
                let mut bytes = vec![];
                if let Some(file) = file.as_mut() {
                    file.seek(SeekFrom::Start(cursor))
                        .map_err(|e| e.to_string())?;
                    file.take(max_bytes.clamp(16, 65536) as u64)
                        .read_to_end(&mut bytes)
                        .map_err(|e| e.to_string())?;
                }
                if let Err(error) = std::str::from_utf8(&bytes) {
                    if error.error_len().is_none() && record.status == "running" {
                        bytes.truncate(error.valid_up_to());
                    }
                }
                return Ok(
                    json!({"process":record,"output":String::from_utf8_lossy(&bytes),"cursor":cursor+bytes.len() as u64,"readTimedOut":length==cursor && now_ms()>=until,"exited":!matches!(record.status.as_str(),"queued"|"running"),"outputLimitBytes":LOG_LIMIT}),
                );
            }
            std::thread::sleep(Duration::from_millis(40));
        }
    }
}

enum OwnedChild {
    Pipe(Child),
    Pty(Box<dyn portable_pty::Child + Send + Sync>),
}
impl OwnedChild {
    fn try_exit(&mut self) -> Result<Option<u32>, String> {
        match self {
            Self::Pipe(c) => c
                .try_wait()
                .map(|s| s.map(|s| s.code().unwrap_or(1) as u32))
                .map_err(|e| e.to_string()),
            Self::Pty(c) => c
                .try_wait()
                .map(|s| s.map(|s| s.exit_code()))
                .map_err(|e| e.to_string()),
        }
    }
}
struct Execution {
    child: OwnedChild,
    tree: ProcessTree,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Option<Box<dyn MasterPty + Send>>,
    log: Arc<Mutex<Log>>,
    record: ProcessSession,
}
struct Log {
    file: File,
    kept: u64,
    truncated: bool,
}
fn capture(
    mut reader: Box<dyn Read + Send>,
    log: Arc<Mutex<Log>>,
    terminal: Option<Arc<Mutex<Box<dyn Write + Send>>>>,
) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        let mut control = Vec::new();
        loop {
            let n = match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            if let Some(writer) = terminal.as_ref() {
                control.extend_from_slice(&buffer[..n]);
                for _ in 0..control
                    .windows(4)
                    .filter(|bytes| *bytes == b"\x1b[6n")
                    .count()
                {
                    if let Ok(mut writer) = writer.lock() {
                        let _ = writer.write_all(b"\x1b[1;1R");
                        let _ = writer.flush();
                    }
                }
                let retain = control.len().saturating_sub(3);
                control.drain(..retain);
            }
            if let Ok(mut log) = log.lock() {
                let count = (LOG_LIMIT.saturating_sub(log.kept) as usize).min(n);
                if log.file.write_all(&buffer[..count]).is_ok() {
                    log.kept += count as u64;
                }
                log.truncated |= count < n;
                let _ = log.file.flush();
            }
        }
    });
}
fn launch(store: &ProcessStore, mut record: ProcessSession) -> Result<Execution, String> {
    fs::create_dir_all(store.home.join("logs")).map_err(|e| e.to_string())?;
    let file = File::create(store.home.join("logs").join(format!("{}.log", record.id)))
        .map_err(|e| e.to_string())?;
    let log = Arc::new(Mutex::new(Log {
        file,
        kept: 0,
        truncated: false,
    }));
    let (child, tree, writer, master, pid) = if record.pty {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 30,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;
        #[cfg(windows)]
        let mut command = CommandBuilder::new("powershell.exe");
        #[cfg(unix)]
        let mut command = CommandBuilder::new("sh");
        #[cfg(windows)]
        command.args(["-NoLogo", "-NoProfile", "-NoExit"]);
        command.cwd(&record.cwd);
        command.env("OPENCOWORK_SESSION_ID", &record.session);
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| e.to_string())?;
        drop(pair.slave);
        let pid = child
            .process_id()
            .ok_or("PTY did not return a process ID")?;
        let tree = ProcessTree::attach(pid).map_err(|e| {
            let _ = child.kill();
            e.to_string()
        })?;
        let writer = Arc::new(Mutex::new(
            pair.master.take_writer().map_err(|e| e.to_string())?,
        ));
        capture(
            pair.master.try_clone_reader().map_err(|e| e.to_string())?,
            log.clone(),
            Some(writer.clone()),
        );
        (OwnedChild::Pty(child), tree, writer, Some(pair.master), pid)
    } else {
        #[cfg(windows)]
        let mut command = {
            use std::os::windows::process::CommandExt;
            let mut c = Command::new("powershell.exe");
            c.args(["-NoLogo","-NoProfile","-NonInteractive","-Command","[Console]::InputEncoding=[Text.UTF8Encoding]::new($false);[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);$script=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String([Console]::ReadLine())); & ([scriptblock]::Create($script)); if ($null -ne $LASTEXITCODE) {exit $LASTEXITCODE}"]);
            c.creation_flags(0x08000000);
            c
        };
        #[cfg(unix)]
        let mut command = {
            use std::os::unix::process::CommandExt;
            let mut c = Command::new("sh");
            c.arg("-s").process_group(0);
            c
        };
        command
            .current_dir(&record.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let pid = child.id();
        let tree = ProcessTree::attach(pid).map_err(|e| {
            let _ = child.kill();
            e.to_string()
        })?;
        capture(Box::new(child.stdout.take().unwrap()), log.clone(), None);
        capture(Box::new(child.stderr.take().unwrap()), log.clone(), None);
        let writer: Arc<Mutex<Box<dyn Write + Send>>> =
            Arc::new(Mutex::new(Box::new(child.stdin.take().unwrap())));
        (OwnedChild::Pipe(child), tree, writer, None, pid)
    };
    #[cfg(windows)]
    let input = if record.pty {
        format!(
            "[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); {}\r",
            record.command
        )
    } else {
        use base64::Engine;
        format!(
            "{}\n",
            base64::engine::general_purpose::STANDARD.encode(record.command.as_bytes())
        )
    };
    #[cfg(unix)]
    let input = format!("{}\n", record.command);
    writer
        .lock()
        .map_err(|e| e.to_string())?
        .write_all(input.as_bytes())
        .map_err(|e| e.to_string())?;
    writer
        .lock()
        .map_err(|e| e.to_string())?
        .flush()
        .map_err(|e| e.to_string())?;
    record.pid = Some(pid);
    record.status = "running".into();
    store.save(&record)?;
    Ok(Execution {
        child,
        tree,
        writer,
        master,
        log,
        record,
    })
}

/// The host owns every process tree, including native Windows ConPTY sessions.
pub fn start_service(home: PathBuf) {
    std::thread::spawn(move || {
        let store = ProcessStore::new(&home);
        let inbox = store.home.join("inbox");
        let _ = fs::create_dir_all(&inbox);
        let pid = std::process::id();
        let lease_path = |pid| store.home.join("hosts").join(format!("{pid}.lock"));
        let Ok(_host) = ExclusiveLease::acquire(&lease_path(pid)) else {
            return;
        };
        for mut record in store.list() {
            if matches!(record.status.as_str(), "queued" | "running") && record.host != pid {
                if let Ok(_stale) = ExclusiveLease::acquire(&lease_path(record.host)) {
                    record.status = "interrupted".into();
                    record.finished_at = Some(now_ms());
                    record.error =
                        Some("Host stopped; inspect output before restarting the process".into());
                    let _ = store.save(&record);
                }
            }
        }
        let (send, recv) = std::sync::mpsc::sync_channel(32);
        let mut watcher = notify::recommended_watcher(move |_: notify::Result<notify::Event>| {
            let _ = send.try_send(());
        })
        .ok();
        if let Some(watcher) = watcher.as_mut() {
            let _ = watcher.watch(&inbox, RecursiveMode::NonRecursive);
        }
        let mut running = BTreeMap::<String, Execution>::new();
        loop {
            for path in fs::read_dir(&inbox)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|s| s == "json"))
            {
                let Some(request) = fs::read(&path)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                else {
                    continue;
                };
                let id = request["id"].as_str().unwrap_or("");
                let Ok(record) = store.get(id) else {
                    let _ = fs::remove_file(path);
                    continue;
                };
                if record.host != pid {
                    continue;
                }
                let action = request["action"].as_str().unwrap_or("");
                if action == "start" && record.status == "queued" {
                    match launch(&store, record.clone()) {
                        Ok(process) => {
                            running.insert(id.into(), process);
                        }
                        Err(error) => {
                            let mut failed = record;
                            failed.status = "failed".into();
                            failed.error = Some(error);
                            failed.finished_at = Some(now_ms());
                            let _ = store.save(&failed);
                        }
                    }
                } else if let Some(process) = running.get_mut(id) {
                    match action {
                        "input" | "interrupt" => {
                            let input = if action == "interrupt" {
                                "\u{3}"
                            } else {
                                request["text"].as_str().unwrap_or("")
                            };
                            let written =
                                process.writer.lock().map_err(|e| e.to_string()).and_then(
                                    |mut writer| {
                                        writer
                                            .write_all(input.as_bytes())
                                            .and_then(|_| writer.flush())
                                            .map_err(|e| e.to_string())
                                    },
                                );
                            if let Err(error) = written {
                                process.record.error = Some(error.to_string());
                                let _ = store.save(&process.record);
                            }
                        }
                        "resize" => {
                            if let Some(master) = process.master.as_ref() {
                                let _ = master.resize(PtySize {
                                    rows: request["rows"].as_u64().unwrap_or(30).clamp(1, 500)
                                        as u16,
                                    cols: request["cols"].as_u64().unwrap_or(120).clamp(1, 500)
                                        as u16,
                                    pixel_width: 0,
                                    pixel_height: 0,
                                });
                            }
                        }
                        "cancel" => {
                            let _ = process.tree.terminate();
                            process.record.status = "cancelled".into();
                        }
                        _ => {}
                    }
                } else if action == "cancel" && record.status == "queued" {
                    let mut cancelled = record;
                    cancelled.status = "cancelled".into();
                    cancelled.finished_at = Some(now_ms());
                    let _ = store.save(&cancelled);
                }
                let _ = fs::remove_file(path);
            }
            let mut ended = vec![];
            for (id, process) in running.iter_mut() {
                let cancelled = fs::read(
                    home.join("subagents/groups")
                        .join(format!("{}.json", process.record.turn)),
                )
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .is_some_and(|g| g["cancelled"] == true)
                    || fs::read(
                        home.join("subagents/tasks")
                            .join(format!("{}.json", process.record.session)),
                    )
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                    .is_some_and(|task| {
                        matches!(
                            task["status"].as_str(),
                            Some("cancelled" | "failed" | "interrupted" | "completed")
                        )
                    });
                if cancelled || now_ms() >= process.record.deadline {
                    let _ = process.tree.terminate();
                    process.record.status =
                        if cancelled { "cancelled" } else { "timed_out" }.into();
                }
                let exit = process.child.try_exit();
                if process.record.status != "running" || !matches!(&exit, Ok(None)) {
                    if let Ok(Some(code)) = exit {
                        process.record.exit_code = Some(code);
                        if process.record.status == "running" {
                            process.record.status = "exited".into();
                        }
                    } else if let Err(error) = exit {
                        process.record.error = Some(error);
                        process.record.status = "failed".into();
                    }
                    process.record.finished_at = Some(now_ms());
                    process.record.truncated =
                        process.log.lock().map(|l| l.truncated).unwrap_or(true);
                    let _ = store.save(&process.record);
                    ended.push(id.clone());
                }
            }
            for id in ended {
                running.remove(&id);
            }
            let _ = recv.recv_timeout(if running.is_empty() {
                Duration::from_secs(10)
            } else {
                Duration::from_millis(100)
            });
            while recv.try_recv().is_ok() {}
        }
    });
}

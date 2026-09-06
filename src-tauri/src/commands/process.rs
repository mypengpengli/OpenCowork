use super::*;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

#[derive(Clone, serde::Serialize)]
pub struct ProcessInfo {
    pub id: String,
    pub request_id: String,
    pub status: String,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub output_path: String,
}

struct Entry {
    info: ProcessInfo,
    token: CancellationToken,
}
fn registry() -> &'static TokioMutex<HashMap<String, Entry>> {
    static VALUE: OnceLock<TokioMutex<HashMap<String, Entry>>> = OnceLock::new();
    VALUE.get_or_init(Default::default)
}

/// The OS job/process group outlives the wait future and owns descendant cleanup.
pub(super) struct ProcessTree {
    #[cfg(windows)]
    job: isize,
    #[cfg(unix)]
    pid: i32,
}
impl ProcessTree {
    pub(super) fn attach(pid: u32) -> Result<Self, String> {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::{
                Foundation::CloseHandle,
                System::{JobObjects::*, Threading::*},
            };
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(io::Error::last_os_error().to_string());
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            let success = !process.is_null()
                && SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as _,
                    std::mem::size_of_val(&info) as u32,
                ) != 0
                && AssignProcessToJobObject(job, process) != 0;
            let error = io::Error::last_os_error();
            if !process.is_null() {
                CloseHandle(process);
            }
            if !success {
                CloseHandle(job);
                return Err(format!("Cannot manage process tree: {error}"));
            }
            Ok(Self { job: job as isize })
        }
        #[cfg(unix)]
        {
            Ok(Self { pid: pid as i32 })
        }
    }
}
impl Drop for ProcessTree {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job as _);
        }
        #[cfg(unix)]
        unsafe {
            libc::kill(-self.pid, libc::SIGKILL);
        }
    }
}

pub(super) fn managed_command(command: &mut TokioCommand) {
    command.kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    #[cfg(unix)]
    command.process_group(0);
}

async fn pump(
    mut source: impl AsyncRead + Unpin,
    output: Arc<TokioMutex<tokio::fs::File>>,
) -> io::Result<()> {
    let mut buffer = [0u8; 8192];
    loop {
        let n = source.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        let mut file = output.lock().await;
        file.write_all(&buffer[..n]).await?;
        file.flush().await?;
    }
    Ok(())
}

pub(super) async fn run(
    mut command: TokioCommand,
    dir: &Path,
    owner: &str,
    parent: Option<&CancellationToken>,
    background: bool,
    timeout_ms: u64,
) -> Result<String, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let id = next_background_task_id();
    let path = dir.join(format!("{id}.output"));
    let file = tokio::fs::File::create(&path)
        .await
        .map_err(|e| e.to_string())?;
    let token = parent
        .map(CancellationToken::child_token)
        .unwrap_or_default();
    managed_command(&mut command);
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let pid = child.id().ok_or("Process exited before registration")?;
    let tree = match ProcessTree::attach(pid) {
        Ok(tree) => tree,
        Err(error) => {
            let _ = child.kill().await;
            return Err(error);
        }
    };
    let stdout = child.stdout.take().ok_or("Missing stdout")?;
    let stderr = child.stderr.take().ok_or("Missing stderr")?;
    let info = ProcessInfo {
        id: id.clone(),
        request_id: owner.into(),
        status: "running".into(),
        pid: Some(pid),
        exit_code: None,
        output_path: path.display().to_string(),
    };
    registry().lock().await.insert(
        id.clone(),
        Entry {
            info,
            token: token.clone(),
        },
    );
    let job_id = id.clone();
    let worker = tokio::spawn(async move {
        let output = Arc::new(TokioMutex::new(file));
        let mut out_task = tokio::spawn(pump(stdout, output.clone()));
        let mut err_task = tokio::spawn(pump(stderr, output));
        let (status, code) = tokio::select! {
            _ = token.cancelled() => ("cancelled", None),
            _ = sleep(TokioDuration::from_millis(timeout_ms)) => ("timed_out", None),
            exit = child.wait() => match exit {
                Ok(exit) => (if exit.success() { "completed" } else { "failed" }, exit.code()),
                Err(_) => ("failed", None),
            }
        };
        drop(tree);
        let _ = child.kill().await;
        let _ = child.wait().await;
        // Closing the job/group also closes inherited pipes held by descendants.
        let pumps = async { tokio::join!(&mut out_task, &mut err_task) };
        if timeout(TokioDuration::from_secs(3), pumps).await.is_err() {
            out_task.abort();
            err_task.abort();
        }
        if let Some(entry) = registry().lock().await.get_mut(&job_id) {
            entry.info.status = status.into();
            entry.info.exit_code = code;
            entry.info.pid = None;
        }
    });
    if !background {
        worker.await.map_err(|e| e.to_string())?;
    }
    describe(&id, owner).await
}

pub(super) async fn describe(id: &str, owner: &str) -> Result<String, String> {
    let info = registry()
        .lock()
        .await
        .get(id)
        .filter(|e| e.info.request_id == owner)
        .map(|e| e.info.clone())
        .ok_or("Unknown process for this task")?;
    let mut file = tokio::fs::File::open(&info.output_path)
        .await
        .map_err(|e| e.to_string())?;
    let start = file
        .metadata()
        .await
        .map_err(|e| e.to_string())?
        .len()
        .saturating_sub(MAX_COMMAND_OUTPUT_CHARS as u64);
    file.seek(io::SeekFrom::Start(start))
        .await
        .map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    file.take(MAX_COMMAND_OUTPUT_CHARS as u64)
        .read_to_end(&mut data)
        .await
        .map_err(|e| e.to_string())?;
    let output = String::from_utf8_lossy(&data);
    Ok(format!(
        "exit_code: {}\nprocess_id: {}\nstatus: {}\noutput_path: {}\n{}{}",
        info.exit_code.unwrap_or(-1),
        info.id,
        info.status,
        info.output_path,
        if start > 0 {
            "[tail; complete output in output_path]\n"
        } else {
            ""
        },
        output
    ))
}

pub(super) async fn stop_owner(owner: &str) -> Result<(), String> {
    {
        let entries = registry().lock().await;
        for entry in entries.values().filter(|e| e.info.request_id == owner) {
            entry.token.cancel();
        }
    }
    timeout(TokioDuration::from_secs(8), async {
        loop {
            if registry()
                .lock()
                .await
                .values()
                .all(|e| e.info.request_id != owner || e.info.status != "running")
            {
                break;
            }
            sleep(TokioDuration::from_millis(25)).await;
        }
    })
    .await
    .map_err(|_| "Processes are still stopping; inspect process status".to_string())
}

pub(super) async fn control(id: &str, owner: &str, stop: bool) -> Result<String, String> {
    if stop {
        {
            let entries = registry().lock().await;
            let entry = entries
                .get(id)
                .filter(|e| e.info.request_id == owner)
                .ok_or("Unknown process for this task")?;
            entry.token.cancel();
        }
        timeout(TokioDuration::from_secs(8), async {
            while registry()
                .lock()
                .await
                .get(id)
                .is_some_and(|e| e.info.status == "running")
            {
                sleep(TokioDuration::from_millis(25)).await;
            }
        })
        .await
        .map_err(|_| "Process is still stopping".to_string())?;
    }
    describe(id, owner).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_cancellation_waits_for_process_exit() {
        let dir = std::env::temp_dir().join(next_background_task_id());
        let token = CancellationToken::new();
        let mut cmd = TokioCommand::new(if cfg!(windows) { "ping" } else { "sleep" });
        if cfg!(windows) {
            cmd.args(["-n", "30", "127.0.0.1"]);
        } else {
            cmd.arg("30");
        }
        run(cmd, &dir, "cancel-test", Some(&token), true, 60_000)
            .await
            .unwrap();
        stop_owner("cancel-test").await.unwrap();
        assert!(registry()
            .lock()
            .await
            .values()
            .filter(|e| e.info.request_id == "cancel-test")
            .all(|e| e.info.status == "cancelled" && e.info.pid.is_none()));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_stop_kills_descendants_and_timeout_is_reported() {
        use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::*};
        let dir = std::env::temp_dir().join(next_background_task_id());
        let owner = next_background_task_id();
        let mut cmd = TokioCommand::new("node");
        cmd.arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/process-tree.cjs"));
        run(cmd, &dir, &owner, None, true, 30_000).await.unwrap();
        let id = registry()
            .lock()
            .await
            .values()
            .find(|e| e.info.request_id == owner)
            .unwrap()
            .info
            .id
            .clone();
        let child_pid = timeout(TokioDuration::from_secs(5), async {
            loop {
                let output = describe(&id, &owner).await.unwrap();
                if let Some(pid) = output.lines().find_map(|line| {
                    line.strip_prefix("child_pid: ")
                        .and_then(|pid| pid.parse::<u32>().ok())
                }) {
                    break pid;
                }
                sleep(TokioDuration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
        // Hold a process handle so PID reuse cannot affect the assertion.
        let child =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | 0x00100000, 0, child_pid) };
        assert!(!child.is_null());
        assert!(control(&id, &owner, true)
            .await
            .unwrap()
            .contains("status: cancelled"));
        unsafe {
            assert_eq!(WaitForSingleObject(child, 5000), 0);
            let mut code = 259;
            assert_ne!(GetExitCodeProcess(child, &mut code), 0);
            assert_ne!(code, 259);
            CloseHandle(child);
        }
        let mut cmd = TokioCommand::new("ping");
        cmd.args(["-n", "30", "127.0.0.1"]);
        let output = run(cmd, &dir, &owner, None, false, 50).await.unwrap();
        assert!(output.contains("status: timed_out"));
    }
}

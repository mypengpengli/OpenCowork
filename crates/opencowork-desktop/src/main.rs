use anyhow::{bail, Context};
use reqwest::blocking::Client;
use std::env;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

const READY_TIMEOUT: Duration = Duration::from_secs(30);

fn main() {
    if let Err(error) = run() {
        let message = format!("OpenCowork could not start:\n{error:#}\n\nSee startup.log and shell.log in the OpenCowork logs folder.\nWebView2: https://developer.microsoft.com/microsoft-edge/webview2/");
        let _ = std::fs::write(log_directory().join("startup.log"), &message);
        eprintln!("{message}");
        #[cfg(windows)]
        unsafe {
            #[link(name = "user32")]
            extern "system" {
                fn MessageBoxW(
                    window: *mut std::ffi::c_void,
                    text: *const u16,
                    caption: *const u16,
                    flags: u32,
                ) -> i32;
            }
            let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
            let caption: Vec<u16> = "OpenCowork startup error"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10);
        }
        std::process::exit(1);
    }
}

fn log_directory() -> PathBuf {
    let root = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("OpenCowork/logs");
    let _ = std::fs::create_dir_all(&root);
    root
}

fn run() -> anyhow::Result<()> {
    wry::webview_version().context("WebView2 Runtime is missing or unavailable. Install the Evergreen Runtime before launching OpenCowork.")?;
    let cwd = env::current_dir().context("failed to resolve current directory")?;
    let port = reserve_local_port()?;
    let shell_url = format!("http://127.0.0.1:{port}/");
    let mut shell_child = spawn_shell_process(&cwd, port)?;
    if let Err(error) = wait_for_shell(&shell_url, &mut shell_child) {
        let _ = terminate_shell_process(&mut shell_child);
        return Err(error);
    }

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("OpenCowork Desktop")
        .with_inner_size(LogicalSize::new(1480.0, 980.0))
        .with_min_inner_size(LogicalSize::new(1100.0, 760.0))
        .build(&event_loop)
        .context("failed to create desktop window")?;

    let _webview = WebViewBuilder::new()
        .with_url(&shell_url)
        .with_devtools(cfg!(debug_assertions))
        .build(&window)
        .context("failed to build desktop webview")?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            let _ = terminate_shell_process(&mut shell_child);
            *control_flow = ControlFlow::Exit;
        }
    });
}

fn reserve_local_port() -> anyhow::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).context("failed to reserve local port")?;
    let port = listener
        .local_addr()
        .context("failed to inspect local port")?
        .port();
    drop(listener);
    Ok(port)
}

fn spawn_shell_process(cwd: &Path, port: u16) -> anyhow::Result<Child> {
    let shell_exe = locate_shell_executable()?;
    let mut command = Command::new(shell_exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .current_dir(cwd)
        .env("OPENCOWORK_SHELL_PORT", port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(std::fs::File::create(
            log_directory().join("shell.log"),
        )?))
        .spawn()
        .context("failed to launch opencowork-shell.exe")
}

fn locate_shell_executable() -> anyhow::Result<PathBuf> {
    let current_exe = env::current_exe().context("failed to resolve current executable path")?;
    let exe_dir = current_exe
        .parent()
        .context("desktop executable has no parent directory")?;
    let candidates = [
        exe_dir.join("opencowork-shell.exe"),
        exe_dir.join("opencowork-shell"),
        exe_dir.join("opencowork_shell.exe"),
    ];

    for candidate in candidates {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    bail!(
        "could not find opencowork-shell executable next to the desktop host in {}",
        exe_dir.display()
    )
}

fn wait_for_shell(shell_url: &str, shell_child: &mut Child) -> anyhow::Result<()> {
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .context("failed to build readiness HTTP client")?;
    let health_url = format!("{shell_url}api/health");
    let start = Instant::now();

    while start.elapsed() < READY_TIMEOUT {
        if let Some(status) = shell_child
            .try_wait()
            .context("failed to poll shell child process")?
        {
            bail!("opencowork-shell exited before readiness with status {status}");
        }

        if client
            .get(&health_url)
            .send()
            .is_ok_and(|response| response.status().is_success())
        {
            return Ok(());
        }

        thread::sleep(Duration::from_millis(250));
    }

    let _ = terminate_shell_process(shell_child);
    bail!("timed out waiting for the local shell server to become ready");
}

fn terminate_shell_process(shell_child: &mut Child) -> anyhow::Result<()> {
    if shell_child
        .try_wait()
        .context("failed to poll shell child process before shutdown")?
        .is_some()
    {
        return Ok(());
    }

    shell_child
        .kill()
        .context("failed to terminate shell child process")?;
    let _ = shell_child.wait();
    Ok(())
}

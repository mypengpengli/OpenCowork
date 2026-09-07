use opencowork_runtime::ProcessTree;
use serde_json::{json, Value};
use std::io::{self, Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const CAPTURE_LIMIT: usize = 2 * 1024 * 1024;

fn capture(mut reader: impl Read) -> io::Result<(String, bool)> {
    let mut kept = Vec::new();
    let mut truncated = false;
    let mut buffer = [0; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let retain = count.min(CAPTURE_LIMIT - kept.len());
        kept.extend_from_slice(&buffer[..retain]);
        truncated |= retain < count;
    }
    Ok((String::from_utf8_lossy(&kept).trim().to_string(), truncated))
}

pub(super) fn run_shell(script: &str, timeout: Duration) -> io::Result<Value> {
    #[cfg(windows)]
    let mut command = {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("powershell");
        // Wait for the parent to attach the job before accepting any user code.
        command.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command",
            "[Console]::InputEncoding = [Text.UTF8Encoding]::new($false); [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); $script = [Console]::In.ReadToEnd(); & ([scriptblock]::Create($script)); if (-not $?) { exit 1 }; if ($null -ne $LASTEXITCODE) { exit $LASTEXITCODE }"]);
        command.creation_flags(0x08000000);
        command
    };
    #[cfg(unix)]
    let mut command = {
        use std::os::unix::process::CommandExt;
        let mut command = Command::new("sh");
        command.arg("-s").process_group(0);
        command
    };
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let start = Instant::now();
    let mut child = command.spawn()?;
    let mut tree = match ProcessTree::attach(child.id()) {
        Ok(tree) => tree,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let mut stdin = child.stdin.take().expect("piped stdin");
    let script = script.as_bytes().to_vec();
    let writer = thread::spawn(move || stdin.write_all(&script));
    let output = thread::spawn(move || capture(stdout));
    let errors = thread::spawn(move || capture(stderr));
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= timeout {
            timed_out = true;
            tree.terminate()?;
            break child.wait()?;
        }
        thread::sleep(Duration::from_millis(20));
    };
    // A descendant can keep a pipe open after the shell exits. Bound that wait too.
    while !output.is_finished() || !errors.is_finished() || !writer.is_finished() {
        if start.elapsed() >= timeout {
            timed_out = true;
            tree.terminate()?;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let _ = writer.join();
    let (stdout, stdout_truncated) = output
        .join()
        .map_err(|_| io::Error::other("stdout reader failed"))??;
    let (stderr, stderr_truncated) = errors
        .join()
        .map_err(|_| io::Error::other("stderr reader failed"))??;
    if !timed_out {
        tree.disarm()?;
    }
    Ok(json!({
        "stdout": stdout, "stderr": stderr,
        "success": status.success() && !timed_out,
        "exit_code": status.code(), "timed_out": timed_out,
        "truncated": stdout_truncated || stderr_truncated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_command_output_and_failure() {
        let output = run_shell("echo hello", Duration::from_secs(10)).unwrap();
        assert_eq!(output["stdout"], "hello");
        assert_eq!(output["success"], true);
        let output = run_shell("echo '你好🌏'", Duration::from_secs(10)).unwrap();
        assert_eq!(output["stdout"], "你好🌏");
        let output = run_shell("exit 7", Duration::from_secs(10)).unwrap();
        assert_eq!(output["exit_code"], 7);
        assert_eq!(output["success"], false);
    }

    #[test]
    fn times_out_commands() {
        #[cfg(windows)]
        let script = "Start-Sleep -Seconds 20";
        #[cfg(unix)]
        let script = "sleep 20";
        let start = Instant::now();
        let output = run_shell(script, Duration::from_millis(800)).unwrap();
        assert_eq!(output["timed_out"], true);
        assert!(!output["success"].as_bool().unwrap());
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn output_limit_keeps_draining() {
        let input = vec![b'x'; CAPTURE_LIMIT + 100];
        let (text, truncated) = capture(input.as_slice()).unwrap();
        assert_eq!(text.len(), CAPTURE_LIMIT);
        assert!(truncated);
    }
}

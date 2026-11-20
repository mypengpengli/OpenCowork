use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    path::Path,
    time::{Duration, Instant},
};
pub(crate) struct RequestPermit(File);
impl RequestPermit {
    pub fn acquire(identity: &str, limit: usize, timeout_ms: u64) -> Result<Self, String> {
        let base = std::env::var_os("OPENCOWORK_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("opencowork-shared"));
        let root = base.join("provider-permits").join(format!(
            "{:x}",
            Sha256::digest(identity.trim_end_matches('/').as_bytes())
        ));
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let start = Instant::now();
        loop {
            for slot in 0..limit.clamp(1, 4096) {
                if let Ok(file) = lock(&root.join(format!("{slot}.lock"))) {
                    return Ok(Self(file));
                }
            }
            if start.elapsed() >= Duration::from_millis(timeout_ms) {
                return Err(
                    "provider_queue_timeout: waiting for shared provider request capacity".into(),
                );
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
fn lock(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(file)
}
impl Drop for RequestPermit {
    fn drop(&mut self) {
        let _ = self.0.metadata();
    }
}

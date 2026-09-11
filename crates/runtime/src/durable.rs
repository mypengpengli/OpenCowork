use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

/// OS-owned exclusive lease. A process crash releases it; the marker may remain.
pub struct ExclusiveLease(File);
impl ExclusiveLease {
    pub fn acquire(path: &Path) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
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
            // SAFETY: live file descriptor, no pointers retained.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(Self(file))
    }
}
impl Drop for ExclusiveLease {
    fn drop(&mut self) {
        let _ = self.0.sync_all();
    }
}

pub fn write_json_atomic(path: &Path, value: &impl serde::Serialize) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = path.with_extension(format!("pending-{}-{stamp}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(&serde_json::to_vec(value)?)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lease_is_exclusive_and_reusable() {
        let path = std::env::temp_dir().join(format!("cowork-lease-{}.lock", std::process::id()));
        let a = ExclusiveLease::acquire(&path).unwrap();
        assert!(ExclusiveLease::acquire(&path).is_err());
        drop(a);
        assert!(ExclusiveLease::acquire(&path).is_ok());
    }
}

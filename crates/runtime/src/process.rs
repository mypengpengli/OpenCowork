//! Lifetime ownership of a subprocess and its descendants, not a security sandbox.
use std::io;

pub struct ProcessTree {
    #[cfg(windows)]
    job: std::os::windows::io::OwnedHandle,
    #[cfg(unix)]
    group: libc::pid_t,
    #[cfg(unix)]
    armed: bool,
}

impl ProcessTree {
    /// The child must wait for input until attachment completes. On Unix, spawn
    /// it with `CommandExt::process_group(0)` before calling this method.
    pub fn attach(pid: u32) -> io::Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
            use windows_sys::Win32::System::JobObjects::*;
            use windows_sys::Win32::System::Threading::*;
            // SAFETY: all handles are checked, owned by RAII, and used only for
            // the child identified by its live PID. The job is unnamed/private.
            unsafe {
                let raw_job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if raw_job.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let job = OwnedHandle::from_raw_handle(raw_job);
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(
                    job.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const _,
                    std::mem::size_of_val(&limits) as u32,
                ) == 0
                {
                    return Err(io::Error::last_os_error());
                }
                let raw_process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
                if raw_process.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let process = OwnedHandle::from_raw_handle(raw_process);
                if AssignProcessToJobObject(job.as_raw_handle(), process.as_raw_handle()) == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Self { job })
            }
        }
        #[cfg(unix)]
        {
            Ok(Self {
                group: pid as libc::pid_t,
                armed: true,
            })
        }
    }

    pub fn terminate(&self) -> io::Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            // SAFETY: this live owned handle refers to our private job.
            if unsafe {
                windows_sys::Win32::System::JobObjects::TerminateJobObject(
                    self.job.as_raw_handle(),
                    1,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        #[cfg(unix)]
        {
            // Tools can create their own process groups. Include descendants
            // still parented to this worker, then terminate its original group.
            for pid in descendant_pids(self.group).into_iter().rev() {
                // SAFETY: positive PIDs were resolved from this process's descendant tree.
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
            // SAFETY: the positive group ID belongs to the child we spawned.
            if unsafe { libc::kill(-self.group, libc::SIGKILL) } != 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// Successful commands may intentionally leave a server running. Transfer
    /// that lifetime to the user rather than killing it when this guard drops.
    pub fn disarm(&mut self) -> io::Result<()> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::JobObjects::*;
            // SAFETY: the job handle remains valid; this clears only our limit flags.
            unsafe {
                let limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                if SetInformationJobObject(
                    self.job.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const _,
                    std::mem::size_of_val(&limits) as u32,
                ) == 0
                {
                    return Err(io::Error::last_os_error());
                }
            }
        }
        #[cfg(unix)]
        {
            self.armed = false;
        }
        Ok(())
    }
}

#[cfg(unix)]
fn descendant_pids(root: libc::pid_t) -> Vec<libc::pid_t> {
    let Ok(output) = std::process::Command::new("ps")
        .args(["-A", "-o", "pid=", "-o", "ppid="])
        .output()
    else {
        return vec![];
    };
    let pairs = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((
                fields.next()?.parse::<libc::pid_t>().ok()?,
                fields.next()?.parse::<libc::pid_t>().ok()?,
            ))
        })
        .collect::<Vec<_>>();
    let mut found = vec![root];
    let mut index = 0;
    while index < found.len() {
        for &(pid, parent) in &pairs {
            if parent == found[index] && pid > 0 && !found.contains(&pid) {
                found.push(pid);
            }
        }
        index += 1;
    }
    found.into_iter().skip(1).collect()
}

#[cfg(unix)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.terminate();
        }
    }
}

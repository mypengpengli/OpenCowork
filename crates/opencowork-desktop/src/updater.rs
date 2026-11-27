use anyhow::{bail, Context};
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tao::event_loop::EventLoopProxy;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const LATEST: &str = "https://api.github.com/repos/mypengpengli/OpenCowork/releases/latest";
const MAX_SIZE: u64 = 150 * 1024 * 1024;

#[derive(Clone, Debug)]
pub enum UpdateEvent {
    Status,
    Check,
    Install,
    Auto(bool),
    Changed,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    current_version: String,
    latest_version: Option<String>,
    phase: String,
    error: Option<String>,
    auto_enabled: bool,
    can_install: bool,
}

#[derive(Clone, Debug)]
struct Candidate {
    version: String,
    url: String,
    digest: String,
    size: u64,
}

pub struct Updater {
    status: Mutex<Status>,
    ready: Mutex<Option<(PathBuf, Candidate)>>,
    checking: AtomicBool,
    directory: PathBuf,
    install_dir: Option<PathBuf>,
    proxy: EventLoopProxy<UpdateEvent>,
}

fn version(text: &str) -> Option<[u64; 3]> {
    let parts: Vec<_> = text.strip_prefix('v').unwrap_or(text).split('.').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some([
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ])
}

fn candidate(release: &Value, current: &str) -> anyhow::Result<Option<Candidate>> {
    if release["draft"] != false || release["prerelease"] != false {
        bail!("Only published stable releases can be installed");
    }
    let tag = release["tag_name"]
        .as_str()
        .context("Release tag missing")?;
    let newer = version(tag).context("Unsupported release version")?;
    if newer <= version(current).context("Invalid current version")? {
        return Ok(None);
    }
    let number = tag.strip_prefix('v').unwrap_or(tag);
    let name = format!("OpenCowork-{number}-Setup-x64.exe");
    let asset = release["assets"]
        .as_array()
        .context("Release assets missing")?
        .iter()
        .find(|a| a["name"] == name)
        .context("Windows installer has not been published yet")?;
    let url = asset["browser_download_url"]
        .as_str()
        .context("Installer URL missing")?;
    if url != format!("https://github.com/mypengpengli/OpenCowork/releases/download/{tag}/{name}") {
        bail!("Unexpected installer origin");
    }
    let digest = asset["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit()))
        .context("Release has no valid SHA256 digest")?;
    let size = asset["size"]
        .as_u64()
        .filter(|s| *s > 0 && *s <= MAX_SIZE)
        .context("Invalid installer size")?;
    Ok(Some(Candidate {
        version: number.into(),
        url: url.into(),
        digest: digest.to_ascii_lowercase(),
        size,
    }))
}

fn copy_verified(
    mut source: impl Read,
    destination: &Path,
    expected: &Candidate,
) -> anyhow::Result<()> {
    let temporary = destination.with_extension(format!("{}.part", std::process::id()));
    let result = (|| {
        let mut file = File::create(&temporary)?;
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let n = source.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > expected.size || count > MAX_SIZE {
                bail!("Installer exceeded its declared size");
            }
            file.write_all(&buffer[..n])?;
            hash.update(&buffer[..n]);
        }
        file.sync_all()?;
        drop(file);
        if count != expected.size || format!("{:x}", hash.finalize()) != expected.digest {
            bail!("Installer SHA256 or size mismatch; update was not installed");
        }
        // Reuse only a previously verified identical download; never overwrite a running installer.
        if destination.exists() {
            verify_file(destination, expected)?;
            fs::remove_file(&temporary)?;
        } else {
            fs::rename(&temporary, destination)?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn verify_file(path: &Path, expected: &Candidate) -> anyhow::Result<()> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > expected.size {
            bail!("Cached installer size mismatch");
        }
        hash.update(&buffer[..n]);
    }
    if count != expected.size || format!("{:x}", hash.finalize()) != expected.digest {
        bail!("Cached installer verification failed");
    }
    Ok(())
}

impl Updater {
    pub fn new(proxy: EventLoopProxy<UpdateEvent>) -> Arc<Self> {
        let directory = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("OpenCowork/updates");
        let install_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_owned))
            .filter(|p| {
                fs::read_to_string(p.join("installation.ini"))
                    .is_ok_and(|s| s.lines().any(|l| l.trim() == "TestMode=0"))
                    && p.join("Uninstall.exe").is_file()
            });
        let auto_enabled = fs::read(directory.join("preferences.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v["autoEnabled"].as_bool())
            .unwrap_or(true);
        Arc::new(Self {
            status: Mutex::new(Status {
                current_version: VERSION.into(),
                latest_version: None,
                phase: "idle".into(),
                error: None,
                auto_enabled,
                can_install: install_dir.is_some(),
            }),
            ready: Mutex::new(None),
            checking: AtomicBool::new(false),
            directory,
            install_dir,
            proxy,
        })
    }

    pub fn snapshot(&self) -> String {
        serde_json::to_string(&*self.status.lock().unwrap()).unwrap()
    }
    fn changed(&self, phase: &str, error: Option<String>) {
        {
            let mut state = self.status.lock().unwrap();
            state.phase = phase.into();
            state.error = error;
        }
        let _ = self.proxy.send_event(UpdateEvent::Changed);
    }
    pub fn set_auto(&self, enabled: bool) -> anyhow::Result<()> {
        fs::create_dir_all(&self.directory)?;
        fs::write(
            self.directory.join("preferences.json"),
            serde_json::to_vec(&serde_json::json!({"autoEnabled":enabled}))?,
        )?;
        self.status.lock().unwrap().auto_enabled = enabled;
        let _ = self.proxy.send_event(UpdateEvent::Changed);
        Ok(())
    }
    pub fn start(self: &Arc<Self>) {
        let updater = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(5));
            loop {
                if updater.install_dir.is_some() && updater.status.lock().unwrap().auto_enabled {
                    updater.check();
                }
                std::thread::sleep(Duration::from_secs(6 * 3600));
            }
        });
    }
    pub fn check(self: &Arc<Self>) {
        if self.checking.swap(true, Ordering::SeqCst) {
            return;
        }
        let updater = self.clone();
        std::thread::spawn(move || {
            updater.changed("checking", None);
            let result = updater.fetch();
            if let Err(error) = result {
                updater.changed("error", Some(format!("{error:#}")));
            }
            updater.checking.store(false, Ordering::SeqCst);
        });
    }
    fn fetch(&self) -> anyhow::Result<()> {
        *self.ready.lock().unwrap() = None;
        self.status.lock().unwrap().latest_version = None;
        let client = Client::builder()
            .https_only(true)
            .user_agent(format!("OpenCowork/{VERSION}"))
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(300))
            .build()?;
        let response = client
            .get(LATEST)
            .header("Accept", "application/vnd.github+json")
            .send()?
            .error_for_status()?;
        let mut bytes = Vec::new();
        response.take(2 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 2 * 1024 * 1024 {
            bail!("Release metadata too large");
        }
        let release: Value = serde_json::from_slice(&bytes)?;
        let Some(asset) = candidate(&release, VERSION)? else {
            self.changed("current", None);
            return Ok(());
        };
        self.status.lock().unwrap().latest_version = Some(asset.version.clone());
        if self.install_dir.is_none() {
            self.changed("available", None);
            return Ok(());
        }
        self.changed("downloading", None);
        fs::create_dir_all(&self.directory)?;
        let destination = self
            .directory
            .join(format!("OpenCowork-{}-Setup-x64.exe", asset.version));
        let response = client.get(&asset.url).send()?.error_for_status()?;
        copy_verified(response, &destination, &asset)?;
        *self.ready.lock().unwrap() = Some((destination, asset));
        self.changed("ready", None);
        Ok(())
    }
    pub fn install(&self) -> anyhow::Result<bool> {
        if self.status.lock().unwrap().phase != "ready" || self.checking.load(Ordering::SeqCst) {
            bail!("No verified update is ready");
        }
        let dir = self
            .install_dir
            .as_ref()
            .context("Development builds must be updated with Git")?;
        let ready = self.ready.lock().unwrap();
        let (path, asset) = ready.as_ref().context("No verified update is ready")?;
        verify_file(path, asset)?;
        #[cfg(windows)]
        unsafe {
            #[link(name = "user32")]
            extern "system" {
                fn MessageBoxW(
                    window: *mut std::ffi::c_void,
                    text: *const u16,
                    title: *const u16,
                    flags: u32,
                ) -> i32;
            }
            let text: Vec<u16> = "现在退出并安装更新？当前任务将停止；会话、设置和工作区会保留。"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let title: Vec<u16> = "OpenCowork 更新".encode_utf16().chain(Some(0)).collect();
            if MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x24) != 6 {
                return Ok(false);
            }
        }
        let mut command = std::process::Command::new(path);
        command
            .args(["/S", "/UPDATE"])
            .arg(format!("/D={}", dir.display()))
            .current_dir(&self.directory);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
            .spawn()
            .context("Could not start update installer")?;
        Ok(true)
    }
    pub fn error(&self, error: anyhow::Error) {
        self.changed("error", Some(format!("{error:#}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release() -> Value {
        serde_json::json!({"draft":false,"prerelease":false,"tag_name":"v0.7.0","assets":[{"name":"OpenCowork-0.7.0-Setup-x64.exe","browser_download_url":"https://github.com/mypengpengli/OpenCowork/releases/download/v0.7.0/OpenCowork-0.7.0-Setup-x64.exe","digest":format!("sha256:{}", "a".repeat(64)),"size":10}]})
    }
    #[test]
    fn release_selection_is_stable_forward_only_and_origin_pinned() {
        assert!(candidate(&release(), "0.6.1").unwrap().is_some());
        assert!(candidate(&release(), "0.7.0").unwrap().is_none());
        assert!(candidate(&release(), "0.8.0").unwrap().is_none());
        for key in ["draft", "prerelease"] {
            let mut v = release();
            v[key] = true.into();
            assert!(candidate(&v, "0.6.1").is_err());
        }
        for (key, value) in [
            ("browser_download_url", "https://example.com/app.exe".into()),
            ("digest", "sha256:bad".into()),
            ("size", (MAX_SIZE + 1).into()),
        ] {
            let mut v = release();
            v["assets"][0][key] = value;
            assert!(candidate(&v, "0.6.1").is_err());
        }
        assert!(version("1.2.3-beta").is_none());
        assert!(version("../1.2.3").is_none());
    }
    #[test]
    fn truncated_or_tampered_downloads_never_become_installers() {
        let root = std::env::temp_dir().join(format!("oc-updater-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("setup.exe");
        let asset = Candidate {
            version: "0.7.0".into(),
            url: String::new(),
            digest: format!("{:x}", Sha256::digest(b"fixture")),
            size: 7,
        };
        assert!(copy_verified(&b"bad"[..], &path, &asset).is_err());
        assert!(!path.exists());
        assert!(!path
            .with_extension(format!("{}.part", std::process::id()))
            .exists());
        assert!(copy_verified(&b"fixture EXTRA"[..], &path, &asset).is_err());
        assert!(!path.exists());
        copy_verified(&b"fixture"[..], &path, &asset).unwrap();
        verify_file(&path, &asset).unwrap();
        fs::write(&path, b"changed").unwrap();
        assert!(verify_file(&path, &asset).is_err());
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    #[ignore = "downloads the public published installer to verify the real GitHub update channel"]
    fn live_update_channel_download() {
        let client = Client::builder()
            .https_only(true)
            .user_agent("OpenCowork-update-verification")
            .timeout(Duration::from_secs(120))
            .build()
            .unwrap();
        let metadata: Value = client
            .get(LATEST)
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .unwrap();
        let asset = candidate(&metadata, "0.0.0").unwrap().unwrap();
        let root = std::env::temp_dir().join(format!("oc-live-update-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("setup.exe");
        let response = client
            .get(&asset.url)
            .send()
            .unwrap()
            .error_for_status()
            .unwrap();
        copy_verified(response, &path, &asset).unwrap();
        verify_file(&path, &asset).unwrap();
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&root).unwrap();
    }
}

use opencowork_runtime::PermissionMode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const MANIFEST_FILE: &str = "plugin.json";
const REGISTRY_FILE: &str = "installed.json";
const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PluginHooks {
    #[serde(rename = "PreToolUse", default)]
    pub pre_tool_use: Vec<String>,
    #[serde(rename = "PostToolUse", default)]
    pub post_tool_use: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginToolManifest {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(rename = "requiredPermission", default = "default_permission")]
    pub required_permission: PluginToolPermission,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginToolPermission {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
}

fn default_permission() -> PluginToolPermission {
    PluginToolPermission::DangerFullAccess
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(rename = "defaultEnabled", default)]
    pub default_enabled: bool,
    #[serde(default)]
    pub hooks: PluginHooks,
    #[serde(default)]
    pub tools: Vec<PluginToolManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledPluginRecord {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub install_path: PathBuf,
    pub source_path: PathBuf,
    pub installed_at_unix_ms: u128,
    pub updated_at_unix_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InstalledPluginRegistry {
    #[serde(default)]
    pub plugins: BTreeMap<String, InstalledPluginRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PluginTool {
    plugin_id: String,
    plugin_root: PathBuf,
    manifest: PluginToolManifest,
}

impl PluginTool {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.manifest.name
    }

    #[must_use]
    pub fn description(&self) -> &str {
        &self.manifest.description
    }

    #[must_use]
    pub fn input_schema(&self) -> &Value {
        &self.manifest.input_schema
    }

    #[must_use]
    pub fn required_permission(&self) -> PermissionMode {
        match self.manifest.required_permission {
            PluginToolPermission::ReadOnly => PermissionMode::ReadOnly,
            PluginToolPermission::WorkspaceWrite => PermissionMode::WorkspaceWrite,
            PluginToolPermission::DangerFullAccess => PermissionMode::DangerFullAccess,
        }
    }

    pub fn execute(&self, input: &Value) -> Result<String, PluginError> {
        let input_json = input.to_string();
        let mut child = Command::new(&self.manifest.command)
            .args(&self.manifest.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(&self.plugin_root)
            .env("OPENCOWORK_PLUGIN_ID", &self.plugin_id)
            .env("OPENCOWORK_TOOL_NAME", &self.manifest.name)
            .env("OPENCOWORK_TOOL_INPUT", &input_json)
            .spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write as _;
            stdin.write_all(input_json.as_bytes())?;
        }

        let output = child.wait_with_output()?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(PluginError::CommandFailed(if stderr.is_empty() {
                format!(
                    "plugin tool `{}` failed with {}",
                    self.manifest.name, output.status
                )
            } else {
                stderr
            }))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallResult {
    pub id: String,
    pub install_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManagerConfig {
    pub config_home: PathBuf,
    pub install_root: PathBuf,
}

impl PluginManagerConfig {
    #[must_use]
    pub fn new(config_home: impl Into<PathBuf>) -> Self {
        let config_home = config_home.into();
        Self {
            install_root: config_home.join("plugins").join("installed"),
            config_home,
        }
    }
}

pub struct PluginManager {
    config: PluginManagerConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    CommandFailed(String),
    #[error("{0}")]
    InvalidManifest(String),
}

impl PluginManager {
    #[must_use]
    pub fn new(config: PluginManagerConfig) -> Self {
        Self { config }
    }

    pub fn validate_source(&self, source: impl AsRef<Path>) -> Result<PluginManifest, PluginError> {
        let path = source.as_ref().join(MANIFEST_FILE);
        let manifest: PluginManifest = serde_json::from_str(&fs::read_to_string(&path)?)?;
        if manifest.name.trim().is_empty() {
            return Err(PluginError::InvalidManifest(
                "plugin name cannot be empty".to_string(),
            ));
        }
        for tool in &manifest.tools {
            if tool.name.trim().is_empty() {
                return Err(PluginError::InvalidManifest(
                    "plugin tool name cannot be empty".to_string(),
                ));
            }
        }
        Ok(manifest)
    }

    pub fn install(&self, source: impl AsRef<Path>) -> Result<InstallResult, PluginError> {
        let source = source.as_ref().to_path_buf();
        let manifest = self.validate_source(&source)?;
        let id = format!("{}@external", manifest.name);
        let install_path = self.config.install_root.join(&id);

        if install_path.exists() {
            fs::remove_dir_all(&install_path)?;
        }
        copy_dir(&source, &install_path)?;

        let mut registry = self.load_registry()?;
        let now = now_ms();
        registry.plugins.insert(
            id.clone(),
            InstalledPluginRecord {
                id: id.clone(),
                name: manifest.name.clone(),
                version: manifest.version,
                description: manifest.description,
                install_path: install_path.clone(),
                source_path: source,
                installed_at_unix_ms: now,
                updated_at_unix_ms: now,
            },
        );
        self.store_registry(&registry)?;
        self.set_enabled(&id, manifest.default_enabled)?;

        Ok(InstallResult { id, install_path })
    }

    pub fn uninstall(&self, id: &str) -> Result<(), PluginError> {
        let mut registry = self.load_registry()?;
        if let Some(record) = registry.plugins.remove(id) {
            if record.install_path.exists() {
                fs::remove_dir_all(record.install_path)?;
            }
            self.store_registry(&registry)?;
            let mut enabled = self.load_enabled_map()?;
            enabled.remove(id);
            self.store_enabled_map(&enabled)?;
        }
        Ok(())
    }

    pub fn enable(&self, id: &str) -> Result<(), PluginError> {
        self.set_enabled(id, true)
    }

    pub fn disable(&self, id: &str) -> Result<(), PluginError> {
        self.set_enabled(id, false)
    }

    pub fn list(&self) -> Result<Vec<PluginSummary>, PluginError> {
        let registry = self.load_registry()?;
        let enabled = self.load_enabled_map()?;
        Ok(registry
            .plugins
            .values()
            .map(|record| PluginSummary {
                id: record.id.clone(),
                name: record.name.clone(),
                version: record.version.clone(),
                description: record.description.clone(),
                enabled: enabled.get(&record.id).copied().unwrap_or(true),
            })
            .collect())
    }

    pub fn aggregated_hooks(&self) -> Result<PluginHooks, PluginError> {
        let enabled = self.load_enabled_map()?;
        let registry = self.load_registry()?;
        let mut hooks = PluginHooks::default();
        for record in registry.plugins.values() {
            if !enabled.get(&record.id).copied().unwrap_or(true) {
                continue;
            }
            let manifest = self.validate_source(&record.install_path)?;
            hooks.pre_tool_use.extend(manifest.hooks.pre_tool_use);
            hooks.post_tool_use.extend(manifest.hooks.post_tool_use);
        }
        Ok(hooks)
    }

    pub fn aggregated_tools(&self) -> Result<Vec<PluginTool>, PluginError> {
        let enabled = self.load_enabled_map()?;
        let registry = self.load_registry()?;
        let mut tools = Vec::new();
        let mut seen = BTreeMap::new();

        for record in registry.plugins.values() {
            if !enabled.get(&record.id).copied().unwrap_or(true) {
                continue;
            }
            let manifest = self.validate_source(&record.install_path)?;
            for tool in manifest.tools {
                if let Some(owner) = seen.insert(tool.name.clone(), record.id.clone()) {
                    return Err(PluginError::InvalidManifest(format!(
                        "tool `{}` is defined by both `{owner}` and `{}`",
                        tool.name, record.id
                    )));
                }
                tools.push(PluginTool {
                    plugin_id: record.id.clone(),
                    plugin_root: record.install_path.clone(),
                    manifest: tool,
                });
            }
        }

        Ok(tools)
    }

    fn registry_path(&self) -> PathBuf {
        self.config.config_home.join("plugins").join(REGISTRY_FILE)
    }

    fn settings_path(&self) -> PathBuf {
        self.config.config_home.join("plugins").join(SETTINGS_FILE)
    }

    fn load_registry(&self) -> Result<InstalledPluginRegistry, PluginError> {
        let path = self.registry_path();
        if !path.exists() {
            return Ok(InstalledPluginRegistry::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    fn store_registry(&self, registry: &InstalledPluginRegistry) -> Result<(), PluginError> {
        ensure_parent(&self.registry_path())?;
        fs::write(
            self.registry_path(),
            serde_json::to_string_pretty(registry)?,
        )?;
        Ok(())
    }

    fn load_enabled_map(&self) -> Result<BTreeMap<String, bool>, PluginError> {
        let path = self.settings_path();
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        #[derive(Deserialize)]
        struct State {
            #[serde(default)]
            enabled_plugins: BTreeMap<String, bool>,
        }
        Ok(serde_json::from_str::<State>(&fs::read_to_string(path)?)?.enabled_plugins)
    }

    fn store_enabled_map(
        &self,
        enabled_plugins: &BTreeMap<String, bool>,
    ) -> Result<(), PluginError> {
        #[derive(Serialize)]
        struct State<'a> {
            enabled_plugins: &'a BTreeMap<String, bool>,
        }
        ensure_parent(&self.settings_path())?;
        fs::write(
            self.settings_path(),
            serde_json::to_string_pretty(&State { enabled_plugins })?,
        )?;
        Ok(())
    }

    fn set_enabled(&self, id: &str, value: bool) -> Result<(), PluginError> {
        let mut enabled = self.load_enabled_map()?;
        enabled.insert(id.to_string(), value);
        self.store_enabled_map(&enabled)
    }
}

fn ensure_parent(path: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_millis()
}

fn copy_dir(source: &Path, target: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(target)?;
    for entry in WalkDir::new(source) {
        let entry = entry?;
        let relative = entry.path().strip_prefix(source).expect("strip prefix");
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination)?;
        } else {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PluginManager, PluginManagerConfig};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("opencowork-{label}-{stamp}"))
    }

    fn write_demo_plugin(root: &std::path::Path) {
        fs::create_dir_all(root).expect("plugin root");
        fs::write(
            root.join("plugin.json"),
            r#"{
  "name": "demo",
  "version": "1.0.0",
  "description": "demo plugin",
  "defaultEnabled": true,
  "hooks": {
    "PreToolUse": ["Write-Output pre"]
  },
  "tools": [
    {
      "name": "plugin_echo",
      "description": "echo tool",
      "inputSchema": { "type": "object" },
      "command": "python",
      "args": ["tool.py"],
      "requiredPermission": "workspace-write"
    }
  ]
}"#,
        )
        .expect("write manifest");
        fs::write(
            root.join("tool.py"),
            "import json,sys; print(json.dumps({'echo': json.loads(sys.stdin.read() or '{}')}))",
        )
        .expect("write tool");
    }

    #[test]
    fn installs_lists_and_toggles_plugins() {
        let home = temp_dir("plugins-home");
        let source = temp_dir("plugins-source");
        write_demo_plugin(&source);

        let manager = PluginManager::new(PluginManagerConfig::new(&home));
        let install = manager.install(&source).expect("install plugin");
        assert_eq!(install.id, "demo@external");

        let listed = manager.list().expect("list plugins");
        assert_eq!(listed.len(), 1);
        assert!(listed[0].enabled);

        manager.disable("demo@external").expect("disable");
        assert!(!manager.list().expect("list after disable")[0].enabled);
        manager.enable("demo@external").expect("enable");
        assert_eq!(manager.aggregated_tools().expect("tools").len(), 1);

        manager.uninstall("demo@external").expect("uninstall");
        assert!(manager.list().expect("empty").is_empty());

        let _ = fs::remove_dir_all(home);
        let _ = fs::remove_dir_all(source);
    }
}

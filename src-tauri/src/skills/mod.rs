mod parser;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::Emitter;
use serde::{Deserialize, Serialize};
use crate::storage::StorageManager;

pub use parser::SkillParser;

const DEFAULT_SCRIPT_PS1: &str = r#"# PowerShell placeholder for this skill.
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts/run.ps1 -InputPath "input" -OutputPath "output"
param(
  [string]$InputPath = "",
  [string]$OutputPath = ""
)

Write-Output "TODO: implement skill automation."
if ($InputPath) { Write-Output "Input: $InputPath" }
if ($OutputPath) { Write-Output "Output: $OutputPath" }
"#;

const DEFAULT_REFERENCE_MD: &str = r#"# Reference
Add domain-specific notes, APIs, examples, and constraints here.
"#;

const DEFAULT_ASSET_TEMPLATE_MD: &str = r#"# Template
Replace this file with the real template or data needed by the skill.
"#;
const DEFAULT_SKILL_MD_FILE: &str = "SKILL.md";
const LOWERCASE_SKILL_MD_FILE: &str = "skill.md";

struct BuiltinFile {
    rel_path: &'static str,
    contents: &'static str,
}

struct BuiltinSkill {
    name: &'static str,
    files: &'static [BuiltinFile],
}

const BUILTIN_SKILLS: &[BuiltinSkill] = &[
    BuiltinSkill {
        name: "skill-creator",
        files: &[
            BuiltinFile {
                rel_path: "SKILL.md",
                contents: include_str!("../../resources/skills/skill-creator/SKILL.md"),
            },
            BuiltinFile {
                rel_path: "references/workflows.md",
                contents: include_str!("../../resources/skills/skill-creator/references/workflows.md"),
            },
            BuiltinFile {
                rel_path: "references/output-patterns.md",
                contents: include_str!("../../resources/skills/skill-creator/references/output-patterns.md"),
            },
            BuiltinFile {
                rel_path: "assets/template.md",
                contents: include_str!("../../resources/skills/skill-creator/assets/template.md"),
            },
            BuiltinFile {
                rel_path: "scripts/init_skill.py",
                contents: include_str!("../../resources/skills/skill-creator/scripts/init_skill.py"),
            },
            BuiltinFile {
                rel_path: "scripts/package_skill.py",
                contents: include_str!("../../resources/skills/skill-creator/scripts/package_skill.py"),
            },
            BuiltinFile {
                rel_path: "scripts/quick_validate.py",
                contents: include_str!("../../resources/skills/skill-creator/scripts/quick_validate.py"),
            },
        ],
    },
];

/// Skill 元数据（启动时加载）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_invocable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Default)]
pub struct SkillFrontmatterOverrides {
    pub allowed_tools: Option<Vec<String>>,
    pub model: Option<String>,
    pub context: Option<String>,
    pub user_invocable: Option<bool>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

/// 完整的 Skill（激活时加载）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    #[serde(flatten)]
    pub metadata: SkillMetadata,
    pub instructions: String,
    pub path: String,
}

/// Skill 管理器
pub struct SkillManager {
    skills_dir: PathBuf,
}

impl SkillManager {
    pub fn new() -> Self {
        let storage = StorageManager::new();
        let skills_dir = storage.get_data_dir().join("skills");

        // 确保 skills 目录存在
        if !skills_dir.exists() {
            std::fs::create_dir_all(&skills_dir).ok();
        }
        ensure_builtin_skills(&skills_dir);

        Self { skills_dir }
    }

    /// 获取 skills 目录路径
    pub fn get_skills_dir(&self) -> &PathBuf {
        &self.skills_dir
    }

    /// 发现所有可用的 skills（只加载元数据）
    pub fn discover_skills(&self) -> Result<Vec<SkillMetadata>, String> {
        let mut skills = Vec::new();

        if !self.skills_dir.exists() {
            return Ok(skills);
        }

        let entries = std::fs::read_dir(&self.skills_dir)
            .map_err(|e| format!("无法读取 skills 目录: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(skill_md) = Self::resolve_skill_md_path(&path) {
                    match SkillParser::parse_metadata(&skill_md) {
                        Ok(metadata) => {
                            // 验证 name 与目录名匹配
                            let dir_name = path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");
                            if metadata.name == dir_name {
                                skills.push(metadata);
                            } else {
                                eprintln!(
                                    "Skill name '{}' 与目录名 '{}' 不匹配，跳过",
                                    metadata.name, dir_name
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("解析 skill {:?} 失败: {}", path, e);
                        }
                    }
                }
            }
        }

        Ok(skills)
    }

    /// 加载完整的 skill（包括指令）
    pub fn load_skill(&self, name: &str) -> Result<Skill, String> {
        // 验证 name 格式
        Self::validate_skill_name(name)?;

        let skill_dir = self.skills_dir.join(name);
        let skill_md = Self::resolve_skill_md_path(&skill_dir)
            .ok_or_else(|| format!("Skill '{}' not found (missing SKILL.md or skill.md)", name))?;

        if !skill_md.exists() {
            return Err(format!("Skill '{}' 不存在", name));
        }

        SkillParser::parse_full(&skill_md)
    }

    /// 创建新的 skill

    pub fn create_skill(
        &self,
        name: &str,
        description: &str,
        instructions: &str,
    ) -> Result<(), String> {
        self.create_skill_with_meta(name, description, instructions, SkillFrontmatterOverrides::default())
    }

    /// ??????? skill
    pub fn create_skill_with_meta(
        &self,
        name: &str,
        description: &str,
        instructions: &str,
        overrides: SkillFrontmatterOverrides,
    ) -> Result<(), String> {
        // ?? name ??
        Self::validate_skill_name(name)?;

        let skill_dir = self.skills_dir.join(name);
        if skill_dir.exists() {
            return Err(format!("Skill '{}' ???", name));
        }

        std::fs::create_dir_all(&skill_dir)
            .map_err(|e| format!("?? skill ????: {}", e))?;

        let skill_md = skill_dir.join(DEFAULT_SKILL_MD_FILE);
        let instructions = ensure_resource_section(instructions);
        let frontmatter = build_skill_frontmatter(name, description, None, &overrides);
        let content = format!("---\n{}\n---\n\n{}", frontmatter, instructions);

        std::fs::write(&skill_md, content)
            .map_err(|e| format!("?? SKILL.md ??: {}", e))?;

        for dir in ["scripts", "references", "assets"] {
            std::fs::create_dir_all(skill_dir.join(dir))
                .map_err(|e| format!("?? {} ????: {}", dir, e))?;
        }
        ensure_scaffold_files(&skill_dir)?;

        Ok(())
    }

    /// ?????? skill
    pub fn update_skill(
        &self,
        name: &str,
        description: &str,
        instructions: &str,
    ) -> Result<(), String> {
        self.update_skill_with_meta(name, description, instructions, SkillFrontmatterOverrides::default())
    }

    /// ??????? skill
    pub fn update_skill_with_meta(
        &self,
        name: &str,
        description: &str,
        instructions: &str,
        overrides: SkillFrontmatterOverrides,
    ) -> Result<(), String> {
        Self::validate_skill_name(name)?;

        let skill_dir = self.skills_dir.join(name);
        if !skill_dir.exists() {
            return Err(format!("Skill '{}' ???", name));
        }

        let skill_md = Self::skill_md_path_for_write(&skill_dir);
        let existing = SkillParser::parse_metadata(&skill_md).ok();
        let instructions = ensure_resource_section(instructions);
        let frontmatter = build_skill_frontmatter(name, description, existing.as_ref(), &overrides);
        let content = format!("---\n{}\n---\n\n{}", frontmatter, instructions);

        std::fs::write(&skill_md, content)
            .map_err(|e| format!("?? SKILL.md ??: {}", e))?;

        for dir in ["scripts", "references", "assets"] {
            std::fs::create_dir_all(skill_dir.join(dir))
                .map_err(|e| format!("?? {} ????: {}", dir, e))?;
        }
        ensure_scaffold_files(&skill_dir)?;

        Ok(())
    }

    pub fn delete_skill(&self, name: &str) -> Result<(), String> {
        Self::validate_skill_name(name)?;

        let skill_dir = self.skills_dir.join(name);
        if !skill_dir.exists() {
            return Err(format!("Skill '{}' 不存在", name));
        }

        std::fs::remove_dir_all(&skill_dir)
            .map_err(|e| format!("删除 skill 失败: {}", e))?;

        Ok(())
    }

    /// 验证 skill name 格式
    fn resolve_skill_md_path(skill_dir: &Path) -> Option<PathBuf> {
        let default_path = skill_dir.join(DEFAULT_SKILL_MD_FILE);
        if default_path.exists() {
            return Some(default_path);
        }

        let lowercase_path = skill_dir.join(LOWERCASE_SKILL_MD_FILE);
        if lowercase_path.exists() {
            return Some(lowercase_path);
        }

        None
    }

    fn skill_md_path_for_write(skill_dir: &Path) -> PathBuf {
        Self::resolve_skill_md_path(skill_dir)
            .unwrap_or_else(|| skill_dir.join(DEFAULT_SKILL_MD_FILE))
    }

    fn validate_skill_name(name: &str) -> Result<(), String> {
        if name.is_empty() || name.len() > 64 {
            return Err("Skill name 必须在 1-64 字符之间".to_string());
        }

        if name.starts_with('-') || name.ends_with('-') {
            return Err("Skill name 不能以连字符开头或结尾".to_string());
        }

        if name.contains("--") {
            return Err("Skill name 不能包含连续连字符".to_string());
        }

        for c in name.chars() {
            if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' {
                return Err("Skill name 只能包含小写字母、数字和连字符".to_string());
            }
        }

        Ok(())
    }
}

fn ensure_resource_section(instructions: &str) -> String {
    let lower = instructions.to_lowercase();
    let has_scripts = lower.contains("scripts/");
    let has_references = lower.contains("references/");
    let has_assets = lower.contains("assets/");
    if has_scripts && has_references && has_assets {
        return instructions.to_string();
    }

    let mut result = instructions.trim().to_string();
    if !result.is_empty() {
        result.push_str("\n\n");
    }
    result.push_str(
        "## Resources\n\
- scripts/: executable scripts (default: scripts/run.ps1)\n\
- references/: reference docs (default: references/REFERENCE.md)\n\
- assets/: templates or data (default: assets/template.md)\n\n\
## Script usage\n\
Run scripts/run.ps1 via Bash/run_command and set cwd to the skill directory.",
    );
    result
}

fn yaml_quote(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}


fn build_skill_frontmatter(
    name: &str,
    description: &str,
    existing: Option<&SkillMetadata>,
    overrides: &SkillFrontmatterOverrides,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!("name: {}", yaml_quote(name)));
    lines.push(format!("description: {}", yaml_quote(description)));

    let allowed_tools = overrides.allowed_tools.clone().or_else(|| existing.and_then(|m| m.allowed_tools.clone()));
    if let Some(list) = allowed_tools {
        let cleaned: Vec<String> = list
            .into_iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();
        if !cleaned.is_empty() {
            lines.push(format!("allowed-tools: {}", yaml_quote(&cleaned.join(", "))));
        }
    }

    let model = overrides.model.clone().or_else(|| existing.and_then(|m| m.model.clone()));
    if let Some(model) = model {
        let model = model.trim();
        if !model.is_empty() {
            lines.push(format!("model: {}", yaml_quote(model)));
        }
    }

    let context = overrides.context.clone().or_else(|| existing.and_then(|m| m.context.clone()));
    if let Some(context) = context {
        let context = context.trim();
        if !context.is_empty() {
            lines.push(format!("context: {}", yaml_quote(context)));
        }
    }

    let user_invocable = overrides.user_invocable.or_else(|| existing.and_then(|m| m.user_invocable));
    if let Some(value) = user_invocable {
        lines.push(format!("user-invocable: {}", value));
    }

    let metadata = overrides.metadata.clone().or_else(|| existing.and_then(|m| m.metadata.clone()));
    if let Some(meta) = metadata {
        if !meta.is_empty() {
            lines.push("metadata:".to_string());
            let mut keys: Vec<String> = meta.keys().cloned().collect();
            keys.sort();
            for key in keys {
                if let Some(value) = meta.get(&key) {
                    lines.push(format!("  {}: {}", yaml_quote(&key), yaml_quote(value)));
                }
            }
        }
    }

    lines.join("\n")
}

fn ensure_scaffold_files(skill_dir: &Path) -> Result<(), String> {
    let scripts_dir = skill_dir.join("scripts");
    let references_dir = skill_dir.join("references");
    let assets_dir = skill_dir.join("assets");

    let scaffolds = [
        (scripts_dir.join("run.ps1"), DEFAULT_SCRIPT_PS1),
        (references_dir.join("REFERENCE.md"), DEFAULT_REFERENCE_MD),
        (assets_dir.join("template.md"), DEFAULT_ASSET_TEMPLATE_MD),
    ];

    for (path, content) in scaffolds {
        if path.exists() {
            continue;
        }
        std::fs::write(&path, content)
            .map_err(|e| format!("写入默认文件失败 {}: {}", path.display(), e))?;
    }

    Ok(())
}

pub type SkillsWatcher = RecommendedWatcher;

pub fn start_skills_watcher(app_handle: &tauri::AppHandle) -> Result<SkillsWatcher, String> {
    let skills_dir = SkillManager::new().get_skills_dir().clone();
    let app_handle = app_handle.clone();
    let last_emit = Arc::new(Mutex::new(Instant::now().checked_sub(Duration::from_secs(5)).unwrap_or_else(Instant::now)));
    let last_emit_guard = Arc::clone(&last_emit);

    let mut watcher = notify::recommended_watcher(move |res| {
        let event: notify::Event = match res {
            Ok(event) => event,
            Err(err) => {
                eprintln!("Skills watcher error: {}", err);
                return;
            }
        };
        if !matches!(
            event.kind,
            EventKind::Create(_)
                | EventKind::Modify(_)
                | EventKind::Remove(_)
                | EventKind::Any
        ) {
            return;
        }

        let now = Instant::now();
        let mut last_emit_at = last_emit_guard.lock().unwrap();
        if now.duration_since(*last_emit_at) < Duration::from_millis(250) {
            return;
        }
        *last_emit_at = now;

        let _ = app_handle.emit("skills-changed", ());
    })
    .map_err(|e| format!("Create skills watcher failed: {}", e))?;

    watcher
        .watch(&skills_dir, RecursiveMode::Recursive)
        .map_err(|e| format!("Watch skills dir failed: {}", e))?;

    Ok(watcher)
}

impl Default for SkillManager {
    fn default() -> Self {
        Self::new()
    }
}

fn ensure_builtin_skills(skills_dir: &Path) {
    for skill in BUILTIN_SKILLS {
        if let Err(err) = ensure_builtin_skill(skills_dir, skill) {
            eprintln!("Failed to init builtin skill {}: {}", skill.name, err);
        }
    }
}

fn ensure_builtin_skill(skills_dir: &Path, skill: &BuiltinSkill) -> Result<(), String> {
    let skill_dir = skills_dir.join(skill.name);
    if skill_dir.exists() && !skill_dir.is_dir() {
        return Err(format!(
            "Skill path is not a directory: {}",
            skill_dir.display()
        ));
    }
    if !skill_dir.exists() {
        std::fs::create_dir_all(&skill_dir)
            .map_err(|e| format!("Create skill dir failed: {}", e))?;
    }

    for file in skill.files {
        let path = skill_dir.join(file.rel_path);
        if path.exists() {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Create skill subdir failed: {}", e))?;
        }
        std::fs::write(&path, file.contents)
            .map_err(|e| format!("Write builtin skill file failed: {}", e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_skill_name() {
        assert!(SkillManager::validate_skill_name("export").is_ok());
        assert!(SkillManager::validate_skill_name("my-skill").is_ok());
        assert!(SkillManager::validate_skill_name("skill123").is_ok());

        assert!(SkillManager::validate_skill_name("").is_err());
        assert!(SkillManager::validate_skill_name("-skill").is_err());
        assert!(SkillManager::validate_skill_name("skill-").is_err());
        assert!(SkillManager::validate_skill_name("my--skill").is_err());
        assert!(SkillManager::validate_skill_name("MySkill").is_err());
        assert!(SkillManager::validate_skill_name("my_skill").is_err());
    }
}

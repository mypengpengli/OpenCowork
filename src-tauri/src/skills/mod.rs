mod parser;

use std::path::{Path, PathBuf};
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
        name: "skill-authoring",
        files: &[
            BuiltinFile {
                rel_path: "SKILL.md",
                contents: include_str!("../../resources/skills/skill-authoring/SKILL.md"),
            },
            BuiltinFile {
                rel_path: "references/REFERENCE.md",
                contents: include_str!("../../resources/skills/skill-authoring/references/REFERENCE.md"),
            },
            BuiltinFile {
                rel_path: "assets/template.md",
                contents: include_str!("../../resources/skills/skill-authoring/assets/template.md"),
            },
            BuiltinFile {
                rel_path: "scripts/run.ps1",
                contents: include_str!("../../resources/skills/skill-authoring/scripts/run.ps1"),
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
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() {
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
        let skill_md = skill_dir.join("SKILL.md");

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
        // 验证 name 格式
        Self::validate_skill_name(name)?;

        let skill_dir = self.skills_dir.join(name);
        if skill_dir.exists() {
            return Err(format!("Skill '{}' 已存在", name));
        }

        std::fs::create_dir_all(&skill_dir)
            .map_err(|e| format!("创建 skill 目录失败: {}", e))?;

        let skill_md = skill_dir.join("SKILL.md");
        let instructions = ensure_resource_section(instructions);
        let content = format!(
            "---\nname: {}\ndescription: {}\n---\n\n{}",
            name, description, instructions
        );

        std::fs::write(&skill_md, content)
            .map_err(|e| format!("写入 SKILL.md 失败: {}", e))?;

        for dir in ["scripts", "references", "assets"] {
            std::fs::create_dir_all(skill_dir.join(dir))
                .map_err(|e| format!("创建 {} 目录失败: {}", dir, e))?;
        }
        ensure_scaffold_files(&skill_dir)?;

        Ok(())
    }

    /// 更新已存在的 skill
    pub fn update_skill(
        &self,
        name: &str,
        description: &str,
        instructions: &str,
    ) -> Result<(), String> {
        Self::validate_skill_name(name)?;

        let skill_dir = self.skills_dir.join(name);
        if !skill_dir.exists() {
            return Err(format!("Skill '{}' 不存在", name));
        }

        let skill_md = skill_dir.join("SKILL.md");
        let instructions = ensure_resource_section(instructions);
        let content = format!(
            "---\nname: {}\ndescription: {}\n---\n\n{}",
            name, description, instructions
        );

        std::fs::write(&skill_md, content)
            .map_err(|e| format!("更新 SKILL.md 失败: {}", e))?;

        for dir in ["scripts", "references", "assets"] {
            std::fs::create_dir_all(skill_dir.join(dir))
                .map_err(|e| format!("创建 {} 目录失败: {}", dir, e))?;
        }
        ensure_scaffold_files(&skill_dir)?;

        Ok(())
    }

    /// 删除 skill
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

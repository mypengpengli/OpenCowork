mod parser;

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::storage::StorageManager;

pub use parser::SkillParser;

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
        let content = format!(
            "---\nname: {}\ndescription: {}\n---\n\n{}",
            name, description, instructions
        );

        std::fs::write(&skill_md, content)
            .map_err(|e| format!("写入 SKILL.md 失败: {}", e))?;

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
        let content = format!(
            "---\nname: {}\ndescription: {}\n---\n\n{}",
            name, description, instructions
        );

        std::fs::write(&skill_md, content)
            .map_err(|e| format!("更新 SKILL.md 失败: {}", e))?;

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

impl Default for SkillManager {
    fn default() -> Self {
        Self::new()
    }
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

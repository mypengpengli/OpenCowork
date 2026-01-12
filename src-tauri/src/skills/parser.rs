use std::path::Path;
use std::collections::HashMap;
use serde::Deserialize;
use super::{Skill, SkillMetadata};

/// YAML frontmatter 结构
#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(rename = "allowed-tools")]
    allowed_tools: Option<String>,
    model: Option<String>,
    context: Option<String>,
    #[serde(rename = "user-invocable")]
    user_invocable: Option<bool>,
    metadata: Option<HashMap<String, String>>,
}

pub struct SkillParser;

impl SkillParser {
    /// 解析 SKILL.md 文件，只提取元数据
    pub fn parse_metadata(path: &Path) -> Result<SkillMetadata, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("读取文件失败: {}", e))?;

        let frontmatter = Self::extract_frontmatter(&content)?;

        Ok(SkillMetadata {
            name: frontmatter.name,
            description: frontmatter.description,
            allowed_tools: frontmatter.allowed_tools.map(|s| {
                s.split([',', ' '])
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }),
            model: frontmatter.model,
            context: frontmatter.context,
            user_invocable: frontmatter.user_invocable,
            metadata: frontmatter.metadata,
        })
    }

    /// 解析完整的 SKILL.md 文件（包括指令）
    pub fn parse_full(path: &Path) -> Result<Skill, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("读取文件失败: {}", e))?;

        let frontmatter = Self::extract_frontmatter(&content)?;
        let instructions = Self::extract_instructions(&content)?;

        Ok(Skill {
            metadata: SkillMetadata {
                name: frontmatter.name,
                description: frontmatter.description,
                allowed_tools: frontmatter.allowed_tools.map(|s| {
                    s.split([',', ' '])
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                }),
                model: frontmatter.model,
                context: frontmatter.context,
                user_invocable: frontmatter.user_invocable,
                metadata: frontmatter.metadata,
            },
            instructions,
            path: path.to_string_lossy().to_string(),
        })
    }

    /// 从内容中提取 YAML frontmatter
    fn extract_frontmatter(content: &str) -> Result<SkillFrontmatter, String> {
        let content = content.trim();

        // 检查是否以 --- 开头
        if !content.starts_with("---") {
            return Err("SKILL.md 必须以 YAML frontmatter 开头 (---)".to_string());
        }

        // 找到第二个 ---
        let rest = &content[3..];
        let end_pos = rest.find("\n---")
            .ok_or("找不到 frontmatter 结束标记 (---)")?;

        let yaml_content = &rest[..end_pos].trim();

        serde_yaml::from_str(yaml_content)
            .map_err(|e| format!("解析 YAML frontmatter 失败: {}", e))
    }

    /// 从内容中提取 Markdown 指令（frontmatter 之后的部分）
    fn extract_instructions(content: &str) -> Result<String, String> {
        let content = content.trim();

        if !content.starts_with("---") {
            return Err("SKILL.md 必须以 YAML frontmatter 开头 (---)".to_string());
        }

        let rest = &content[3..];
        let end_pos = rest.find("\n---")
            .ok_or("找不到 frontmatter 结束标记 (---)")?;

        // 跳过 frontmatter 和结束标记
        let instructions_start = 3 + end_pos + 4; // "---" + yaml + "\n---"
        if instructions_start >= content.len() {
            return Ok(String::new());
        }

        Ok(content[instructions_start..].trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_frontmatter() {
        let content = r#"---
name: test-skill
description: A test skill for testing purposes
---

# Test Skill

This is the instruction content.
"#;

        let frontmatter = SkillParser::extract_frontmatter(content).unwrap();
        assert_eq!(frontmatter.name, "test-skill");
        assert_eq!(frontmatter.description, "A test skill for testing purposes");
    }

    #[test]
    fn test_extract_instructions() {
        let content = r#"---
name: test-skill
description: A test skill
---

# Test Skill

This is the instruction content.
"#;

        let instructions = SkillParser::extract_instructions(content).unwrap();
        assert!(instructions.contains("# Test Skill"));
        assert!(instructions.contains("This is the instruction content."));
    }

    #[test]
    fn test_allowed_tools_parsing() {
        let content = r#"---
name: test-skill
description: A test skill
allowed-tools: Read, Grep, Glob
---

Instructions here.
"#;

        let frontmatter = SkillParser::extract_frontmatter(content).unwrap();
        let tools = frontmatter.allowed_tools.unwrap();
        assert!(tools.contains("Read"));
    }
}

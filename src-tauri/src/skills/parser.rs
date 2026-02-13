use super::{Skill, SkillMetadata};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(rename = "allowed-tools")]
    allowed_tools: Option<AllowedToolsField>,
    model: Option<String>,
    context: Option<String>,
    #[serde(rename = "user-invocable")]
    user_invocable: Option<bool>,
    #[serde(rename = "disable-model-invocation")]
    disable_model_invocation: Option<bool>,
    metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AllowedToolsField {
    Text(String),
    List(Vec<String>),
}

pub struct SkillParser;

impl SkillParser {
    pub fn parse_metadata(path: &Path) -> Result<SkillMetadata, String> {
        let content = std::fs::read_to_string(path).map_err(|e| format!("read file failed: {}", e))?;
        let frontmatter = Self::extract_frontmatter(&content)?;
        let name = Self::resolve_name(path, frontmatter.name)?;
        let description = Self::resolve_description(frontmatter.description, &name);

        Ok(SkillMetadata {
            name,
            description,
            allowed_tools: Self::parse_allowed_tools(frontmatter.allowed_tools),
            model: frontmatter.model,
            context: frontmatter.context,
            user_invocable: frontmatter.user_invocable,
            disable_model_invocation: frontmatter.disable_model_invocation,
            metadata: frontmatter.metadata,
        })
    }

    pub fn parse_full(path: &Path) -> Result<Skill, String> {
        let content = std::fs::read_to_string(path).map_err(|e| format!("read file failed: {}", e))?;
        let frontmatter = Self::extract_frontmatter(&content)?;
        let instructions = Self::extract_instructions(&content)?;
        let name = Self::resolve_name(path, frontmatter.name)?;
        let description = Self::resolve_description(frontmatter.description, &name);

        Ok(Skill {
            metadata: SkillMetadata {
                name,
                description,
                allowed_tools: Self::parse_allowed_tools(frontmatter.allowed_tools),
                model: frontmatter.model,
                context: frontmatter.context,
                user_invocable: frontmatter.user_invocable,
                disable_model_invocation: frontmatter.disable_model_invocation,
                metadata: frontmatter.metadata,
            },
            instructions,
            path: path.to_string_lossy().to_string(),
        })
    }

    fn extract_frontmatter(content: &str) -> Result<SkillFrontmatter, String> {
        let content = content.trim();
        if !content.starts_with("---") {
            return Err("SKILL.md must start with YAML frontmatter (---)".to_string());
        }

        let rest = &content[3..];
        let end_pos = rest
            .find("\n---")
            .ok_or_else(|| "cannot find frontmatter end marker (---)".to_string())?;
        let yaml_content = rest[..end_pos].trim();

        serde_yaml::from_str(yaml_content).map_err(|e| format!("parse YAML frontmatter failed: {}", e))
    }

    fn extract_instructions(content: &str) -> Result<String, String> {
        let content = content.trim();
        if !content.starts_with("---") {
            return Err("SKILL.md must start with YAML frontmatter (---)".to_string());
        }

        let rest = &content[3..];
        let end_pos = rest
            .find("\n---")
            .ok_or_else(|| "cannot find frontmatter end marker (---)".to_string())?;
        let instructions_start = 3 + end_pos + 4;
        if instructions_start >= content.len() {
            return Ok(String::new());
        }

        Ok(content[instructions_start..].trim().to_string())
    }

    fn parse_allowed_tools(value: Option<AllowedToolsField>) -> Option<Vec<String>> {
        let mut tools = Vec::new();
        match value {
            None => return None,
            Some(AllowedToolsField::Text(text)) => {
                for token in text.split([',', ' ', '\n', '\t']) {
                    let token = token.trim();
                    if !token.is_empty() {
                        tools.push(token.to_string());
                    }
                }
            }
            Some(AllowedToolsField::List(list)) => {
                for item in list {
                    let item = item.trim();
                    if !item.is_empty() {
                        tools.push(item.to_string());
                    }
                }
            }
        }
        if tools.is_empty() {
            None
        } else {
            Some(tools)
        }
    }

    fn resolve_name(path: &Path, frontmatter_name: Option<String>) -> Result<String, String> {
        if let Some(name) = frontmatter_name {
            let name = name.trim();
            if !name.is_empty() {
                return Ok(name.to_string());
            }
        }

        let fallback = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        fallback.ok_or_else(|| "frontmatter is missing name and directory fallback failed".to_string())
    }

    fn resolve_description(frontmatter_description: Option<String>, name: &str) -> String {
        if let Some(description) = frontmatter_description {
            let description = description.trim();
            if !description.is_empty() {
                return description.to_string();
            }
        }
        format!("Skill {}", name)
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
"#;

        let frontmatter = SkillParser::extract_frontmatter(content).unwrap();
        assert_eq!(frontmatter.name.as_deref(), Some("test-skill"));
        assert_eq!(
            frontmatter.description.as_deref(),
            Some("A test skill for testing purposes")
        );
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
    fn test_allowed_tools_string_parsing() {
        let content = r#"---
name: test-skill
description: A test skill
allowed-tools: Read, Grep, Glob
---
"#;

        let frontmatter = SkillParser::extract_frontmatter(content).unwrap();
        let tools = SkillParser::parse_allowed_tools(frontmatter.allowed_tools).unwrap();
        assert!(tools.contains(&"Read".to_string()));
        assert!(tools.contains(&"Grep".to_string()));
        assert!(tools.contains(&"Glob".to_string()));
    }

    #[test]
    fn test_allowed_tools_list_parsing() {
        let content = r#"---
name: test-skill
description: A test skill
allowed-tools:
  - Read
  - Bash
---
"#;

        let frontmatter = SkillParser::extract_frontmatter(content).unwrap();
        let tools = SkillParser::parse_allowed_tools(frontmatter.allowed_tools).unwrap();
        assert_eq!(tools, vec!["Read".to_string(), "Bash".to_string()]);
    }
}

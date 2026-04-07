use crate::{CompactResult, ContentBlock, PermissionMode, Session};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

const MAX_INSTRUCTION_FILE_CHARS: usize = 4_000;
const DISCOVERED_INSTRUCTION_FILES: &[&str] = &[
    "CLAUDE.md",
    "CLAUDE.local.md",
    ".claude/CLAUDE.md",
    ".opencowork/CLAUDE.md",
    ".opencowork/instructions.md",
    ".codex/CLAUDE.md",
    ".codex/instructions.md",
    "CLAW.md",
    "CLAW.local.md",
    ".claw/CLAW.md",
    ".claw/instructions.md",
];
const DISCOVERED_RULE_DIRECTORIES: &[&str] = &[".claude/rules"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectContext {
    pub cwd: PathBuf,
    pub current_date: String,
    pub model: Option<String>,
    pub permission_mode: PermissionMode,
    pub instruction_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionSource {
    pub label: String,
    pub content: String,
    pub priority: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptLayer {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptBundle {
    pub system_prompt: Vec<String>,
    pub layers: Vec<PromptLayer>,
    pub estimated_tokens: usize,
    pub compacted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptComposer;

impl PromptComposer {
    #[must_use]
    pub fn compose(
        &self,
        project: &ProjectContext,
        instructions: &[InstructionSource],
        compact_result: Option<&CompactResult>,
        recent_summary: Option<&str>,
    ) -> PromptBundle {
        let mut layers = vec![
            PromptLayer {
                name: "identity".to_string(),
                content: "You are OpenCoWork, a local-first coding runtime.".to_string(),
            },
            PromptLayer {
                name: "system".to_string(),
                content: "Read the workspace carefully, stay within the requested scope, and report verification honestly.".to_string(),
            },
            PromptLayer {
                name: "environment".to_string(),
                content: format!(
                    "Working directory: {}\nCurrent date: {}\nPermission mode: {:?}",
                    project.cwd.display(),
                    project.current_date,
                    project.permission_mode
                ),
            },
        ];

        if let Some(model) = &project.model {
            layers.push(PromptLayer {
                name: "model".to_string(),
                content: format!("Requested model: {model}"),
            });
        }

        let mut sorted = instructions.to_vec();
        sorted.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.label.cmp(&right.label))
        });
        for instruction in sorted {
            layers.push(PromptLayer {
                name: format!("instruction:{}", instruction.label),
                content: instruction.content,
            });
        }

        if let Some(compact_result) = compact_result.filter(|value| !value.summary.is_empty()) {
            layers.push(PromptLayer {
                name: "compacted-summary".to_string(),
                content: if compact_result.formatted_summary.trim().is_empty() {
                    compact_result.summary.clone()
                } else {
                    compact_result.formatted_summary.clone()
                },
            });
        }

        if let Some(summary) = recent_summary.filter(|value| !value.trim().is_empty()) {
            layers.push(PromptLayer {
                name: "context-summary".to_string(),
                content: summary.to_string(),
            });
        }

        let system_prompt = layers
            .iter()
            .map(|layer| layer.content.clone())
            .collect::<Vec<_>>();
        let estimated_tokens = system_prompt
            .iter()
            .map(|content| content.len() / 4 + 1)
            .sum();

        PromptBundle {
            system_prompt,
            layers,
            estimated_tokens,
            compacted: compact_result.is_some_and(|value| !value.summary.is_empty()),
        }
    }
}

pub fn discover_instruction_sources(
    cwd: &Path,
    configured_files: &[String],
    max_instruction_tokens: usize,
) -> Result<Vec<InstructionSource>, std::io::Error> {
    if max_instruction_tokens == 0 {
        return Ok(Vec::new());
    }

    let mut candidates = Vec::new();
    let ancestor_dirs = ancestor_directories(cwd);
    let ancestor_count = ancestor_dirs.len();
    for (depth, dir) in ancestor_dirs.iter().enumerate() {
        for (index, relative_path) in configured_files.iter().enumerate() {
            let path = dir.join(relative_path);
            if let Some(content) = read_optional_text(&path)? {
                candidates.push(InstructionSource {
                    label: path.display().to_string(),
                    content,
                    priority: 20_000
                        + depth.saturating_mul(100)
                        + (ancestor_count.saturating_sub(depth)).saturating_mul(10)
                        + configured_files.len().saturating_sub(index),
                });
            }
        }
    }

    for (depth, dir) in ancestor_dirs.iter().enumerate() {
        for (index, relative_path) in DISCOVERED_INSTRUCTION_FILES.iter().enumerate() {
            let path = dir.join(relative_path);
            if let Some(content) = read_optional_text(&path)? {
                candidates.push(InstructionSource {
                    label: path.display().to_string(),
                    content,
                    priority: 10_000
                        + depth.saturating_mul(100)
                        + (ancestor_count.saturating_sub(depth)).saturating_mul(10)
                        + (DISCOVERED_INSTRUCTION_FILES.len().saturating_sub(index)),
                });
            }
        }
        for rule_dir in DISCOVERED_RULE_DIRECTORIES {
            candidates.extend(discover_rule_instruction_sources(
                &dir.join(rule_dir),
                10_500
                    + depth.saturating_mul(100)
                    + (ancestor_count.saturating_sub(depth)).saturating_mul(10),
            )?);
        }
    }

    Ok(normalize_and_budget_instruction_sources(
        candidates,
        max_instruction_tokens,
    ))
}

pub fn discover_nested_instruction_sources(
    cwd: &Path,
    touched_paths: &[PathBuf],
    max_instruction_tokens: usize,
) -> Result<Vec<InstructionSource>, std::io::Error> {
    if max_instruction_tokens == 0 || touched_paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut candidates = Vec::new();
    let mut seen_dirs = BTreeSet::new();
    for touched_path in touched_paths {
        let directory = touched_instruction_directory(cwd, touched_path);
        if directory == cwd || !directory.starts_with(cwd) {
            continue;
        }
        let nested_dirs = descendant_directories(cwd, &directory);
        let nested_count = nested_dirs.len();
        for (depth, dir) in nested_dirs.iter().enumerate() {
            if !seen_dirs.insert(dir.clone()) {
                continue;
            }
            for (index, relative_path) in DISCOVERED_INSTRUCTION_FILES.iter().enumerate() {
                let path = dir.join(relative_path);
                if let Some(content) = read_optional_text(&path)? {
                    candidates.push(InstructionSource {
                        label: path.display().to_string(),
                        content,
                        priority: 22_000
                            + depth.saturating_mul(100)
                            + (nested_count.saturating_sub(depth)).saturating_mul(10)
                            + (DISCOVERED_INSTRUCTION_FILES.len().saturating_sub(index)),
                    });
                }
            }
            for rule_dir in DISCOVERED_RULE_DIRECTORIES {
                candidates.extend(discover_rule_instruction_sources(
                    &dir.join(rule_dir),
                    22_500
                        + depth.saturating_mul(100)
                        + (nested_count.saturating_sub(depth)).saturating_mul(10),
                )?);
            }
        }
    }

    Ok(normalize_and_budget_instruction_sources(
        candidates,
        max_instruction_tokens,
    ))
}

#[must_use]
pub fn skill_instruction_sources_from_session(session: &Session) -> Vec<InstructionSource> {
    let mut skills = Vec::new();
    let mut seen = BTreeSet::new();

    for (index, block) in session
        .messages
        .iter()
        .rev()
        .flat_map(|message| message.blocks.iter())
        .enumerate()
    {
        let ContentBlock::ToolResult {
            tool_name,
            output,
            is_error,
            ..
        } = block
        else {
            continue;
        };
        if *is_error || tool_name != "Skill" {
            continue;
        }
        let Some(source) = skill_instruction_source_from_output(output, index) else {
            continue;
        };
        if seen.insert(source.label.clone()) {
            skills.push(source);
        }
    }

    skills.reverse();
    skills
}

#[must_use]
pub fn skill_instruction_source_from_output(
    output: &str,
    recency_index: usize,
) -> Option<InstructionSource> {
    let payload = serde_json::from_str::<Value>(output).ok()?;
    let skill_name = payload.get("name")?.as_str()?.trim();
    let instruction = payload.get("instruction")?.as_str()?.trim();
    if skill_name.is_empty() || instruction.is_empty() {
        return None;
    }

    let mut details = Vec::new();
    if let Some(description) = payload
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        details.push(format!("Description: {description}"));
    }
    if let Some(when_to_use) = payload
        .get("when_to_use")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        details.push(format!("When to use: {when_to_use}"));
    }
    if let Some(argument_hint) = payload
        .get("argument_hint")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        details.push(format!("Argument hint: {argument_hint}"));
    }
    if let Some(allowed_tools) = payload
        .get("allowed_tools")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
    {
        details.push(format!("Allowed tools: {}", allowed_tools.join(", ")));
    }
    if let Some(paths) = payload
        .get("paths")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
    {
        details.push(format!("Activation paths: {}", paths.join(", ")));
    }
    if let Some(execution_context) = payload
        .get("execution_context")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        details.push(format!("Execution context: {execution_context}"));
    }
    if let Some(version) = payload
        .get("version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        details.push(format!("Version: {version}"));
    }
    if let Some(agent) = payload
        .get("agent")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        details.push(format!("Suggested agent: {agent}"));
    }
    if let Some(model) = payload
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        details.push(format!("Preferred model: {model}"));
    }
    if let Some(effort) = payload
        .get("effort")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        details.push(format!("Suggested effort: {effort}"));
    }

    let content = if let Some(args) = payload.get("args").and_then(Value::as_str).map(str::trim) {
        if !args.is_empty() {
            if details.is_empty() {
                format!(
                    "Active skill `{skill_name}`.\nRequested skill arguments: {args}\nApply these instructions when relevant.\n\n{instruction}"
                )
            } else {
                format!(
                    "Active skill `{skill_name}`.\nRequested skill arguments: {args}\n{}\nApply these instructions when relevant.\n\n{instruction}",
                    details.join("\n")
                )
            }
        } else if details.is_empty() {
            format!(
                "Active skill `{skill_name}`.\nApply these instructions when relevant.\n\n{instruction}"
            )
        } else {
            format!(
                "Active skill `{skill_name}`.\n{}\nApply these instructions when relevant.\n\n{instruction}",
                details.join("\n")
            )
        }
    } else if details.is_empty() {
        format!("Active skill `{skill_name}`.\nApply these instructions when relevant.\n\n{instruction}")
    } else {
        format!(
            "Active skill `{skill_name}`.\n{}\nApply these instructions when relevant.\n\n{instruction}",
            details.join("\n")
        )
    };

    Some(InstructionSource {
        label: format!("skill:{skill_name}"),
        content: truncate_instruction_content(&content, MAX_INSTRUCTION_FILE_CHARS),
        priority: 30_000usize.saturating_sub(recency_index),
    })
}

fn normalize_and_budget_instruction_sources(
    mut sources: Vec<InstructionSource>,
    max_instruction_tokens: usize,
) -> Vec<InstructionSource> {
    sources.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.label.cmp(&right.label))
    });

    let mut normalized = Vec::new();
    let mut seen_hashes = BTreeSet::new();
    let mut remaining_chars = max_instruction_tokens.saturating_mul(4);

    for mut source in sources {
        if remaining_chars == 0 {
            break;
        }
        let normalized_content = normalize_instruction_content(&source.content);
        let hash = stable_content_hash(&normalized_content);
        if !seen_hashes.insert(hash) {
            continue;
        }

        let max_chars = remaining_chars.min(MAX_INSTRUCTION_FILE_CHARS);
        source.content = truncate_instruction_content(&normalized_content, max_chars);
        remaining_chars = remaining_chars.saturating_sub(source.content.chars().count());
        normalized.push(source);
    }

    normalized
}

fn read_optional_text(path: &Path) -> Result<Option<String>, std::io::Error> {
    match fs::read_to_string(path) {
        Ok(content) if !content.trim().is_empty() => Ok(Some(content)),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn ancestor_directories(cwd: &Path) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    let mut cursor = Some(cwd);
    while let Some(current) = cursor {
        directories.push(current.to_path_buf());
        cursor = current.parent();
    }
    directories.reverse();
    directories
}

fn descendant_directories(cwd: &Path, target: &Path) -> Vec<PathBuf> {
    ancestor_directories(target)
        .into_iter()
        .filter(|dir| dir.starts_with(cwd) && dir != cwd)
        .collect()
}

fn touched_instruction_directory(cwd: &Path, touched_path: &Path) -> PathBuf {
    if touched_path == cwd {
        return cwd.to_path_buf();
    }
    if touched_path.is_dir() {
        return touched_path.to_path_buf();
    }
    touched_path
        .parent()
        .filter(|parent| parent.starts_with(cwd))
        .map_or_else(|| cwd.to_path_buf(), Path::to_path_buf)
}

fn discover_rule_instruction_sources(
    rule_dir: &Path,
    priority_base: usize,
) -> Result<Vec<InstructionSource>, std::io::Error> {
    let mut files = Vec::new();
    collect_markdown_files(rule_dir, &mut files)?;
    files.sort();

    let mut sources = Vec::new();
    for (index, path) in files.into_iter().enumerate() {
        if let Some(content) = read_optional_text(&path)? {
            sources.push(InstructionSource {
                label: path.display().to_string(),
                content,
                priority: priority_base + index,
            });
        }
    }

    Ok(sources)
}

fn collect_markdown_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_markdown_files(&path, files)?;
            continue;
        }
        if file_type.is_file() && path.extension().and_then(|value| value.to_str()) == Some("md") {
            files.push(path);
        }
    }

    Ok(())
}

fn normalize_instruction_content(content: &str) -> String {
    let mut result = String::new();
    let mut previous_blank = false;

    for line in content.lines() {
        let trimmed = line.trim_end();
        let is_blank = trimmed.trim().is_empty();
        if is_blank && previous_blank {
            continue;
        }
        result.push_str(trimmed);
        result.push('\n');
        previous_blank = is_blank;
    }

    result.trim().to_string()
}

fn stable_content_hash(content: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

fn truncate_instruction_content(content: &str, max_chars: usize) -> String {
    if content.chars().count() <= max_chars {
        return content.to_string();
    }

    let mut rendered = content.chars().take(max_chars).collect::<String>();
    rendered.push_str("\n\n[truncated]");
    rendered
}

#[cfg(test)]
mod tests {
    use super::{
        discover_instruction_sources, discover_nested_instruction_sources,
        normalize_instruction_content, skill_instruction_sources_from_session, InstructionSource,
        ProjectContext, PromptComposer,
    };
    use crate::{CompactResult, ConversationMessage, PermissionMode, Session};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("opencowork-prompt-{stamp}"))
    }

    #[test]
    fn composes_prompt_layers_in_priority_order() {
        let composer = PromptComposer;
        let bundle = composer.compose(
            &ProjectContext {
                cwd: PathBuf::from("/tmp/project"),
                current_date: "2026-04-02".to_string(),
                model: Some("gpt-5.4".to_string()),
                permission_mode: PermissionMode::WorkspaceWrite,
                instruction_files: Vec::new(),
            },
            &[
                InstructionSource {
                    label: "low".to_string(),
                    content: "low".to_string(),
                    priority: 1,
                },
                InstructionSource {
                    label: "high".to_string(),
                    content: "high".to_string(),
                    priority: 10,
                },
            ],
            Some(&CompactResult {
                summary: "<summary>Older work</summary>".to_string(),
                formatted_summary: "Summary:\nOlder work".to_string(),
                compacted_session: Session::new(),
                removed_message_count: 3,
            }),
            Some("recent summary"),
        );

        assert!(bundle
            .system_prompt
            .iter()
            .any(|value| value.contains("gpt-5.4")));
        let high_index = bundle
            .layers
            .iter()
            .position(|layer| layer.name == "instruction:high")
            .expect("high layer");
        let low_index = bundle
            .layers
            .iter()
            .position(|layer| layer.name == "instruction:low")
            .expect("low layer");
        assert!(high_index < low_index);
        assert!(bundle
            .layers
            .iter()
            .any(|layer| layer.name == "compacted-summary"
                && layer.content.contains("Summary:\nOlder work")));
    }

    #[test]
    fn discovers_instruction_files_from_ancestor_chain_and_dedupes() {
        let root = temp_dir();
        let nested = root.join("apps").join("api");
        fs::create_dir_all(nested.join(".opencowork")).expect("nested dir");
        fs::write(root.join("CLAUDE.md"), "root instructions").expect("root instruction");
        fs::write(root.join("README.md"), "workspace readme").expect("readme");
        fs::write(
            nested.join(".opencowork").join("instructions.md"),
            "nested instructions",
        )
        .expect("nested instruction");
        fs::write(nested.join("CLAUDE.md"), "root instructions").expect("duplicate content");

        let sources = discover_instruction_sources(&nested, &["README.md".to_string()], 2_000)
            .expect("discover instructions");

        assert!(sources
            .iter()
            .any(|source| source.content.contains("workspace readme")));
        assert!(sources
            .iter()
            .any(|source| source.content.contains("nested instructions")));
        assert_eq!(
            sources
                .iter()
                .filter(|source| source.content == "root instructions")
                .count(),
            1
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn extracts_active_skill_instructions_from_session() {
        let session = Session::from_messages(vec![ConversationMessage::tool_result(
            "tool-1",
            "Skill",
            r#"{"name":"planner","description":"Planning guidance","instruction":"Use a clear plan.","args":"mode=fast"}"#,
            false,
        )]);

        let sources = skill_instruction_sources_from_session(&session);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].label, "skill:planner");
        assert!(sources[0].content.contains("Use a clear plan."));
        assert!(sources[0].content.contains("mode=fast"));
    }

    #[test]
    fn discovers_nested_instruction_sources_for_touched_paths() {
        let root = temp_dir();
        let feature_dir = root.join("packages").join("app");
        fs::create_dir_all(feature_dir.join(".claude").join("rules")).expect("nested rules dir");
        fs::write(feature_dir.join("CLAUDE.md"), "package instructions").expect("claude");
        fs::write(
            feature_dir.join(".claude").join("rules").join("ui.md"),
            "ui rule",
        )
        .expect("rule");

        let touched = vec![feature_dir.join("src").join("main.tsx")];
        fs::create_dir_all(feature_dir.join("src")).expect("src dir");
        fs::write(
            feature_dir.join("src").join("main.tsx"),
            "console.log('hi');",
        )
        .expect("file");

        let sources =
            discover_nested_instruction_sources(&root, &touched, 2_000).expect("nested sources");
        assert!(sources
            .iter()
            .any(|source| source.content.contains("package instructions")));
        assert!(sources
            .iter()
            .any(|source| source.content.contains("ui rule")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn normalizes_blank_lines_in_instruction_content() {
        assert_eq!(
            normalize_instruction_content("line one\n\n\nline two\n"),
            "line one\n\nline two"
        );
    }
}

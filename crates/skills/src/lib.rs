use glob::Pattern;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const SKILL_BUDGET_CONTEXT_PERCENT: usize = 1;
const SKILL_BUDGET_CHARS_PER_TOKEN: usize = 4;
const DEFAULT_SKILL_CHAR_BUDGET: usize = 8_000;
const MAX_LISTING_DESC_CHARS: usize = 250;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillOrigin {
    SkillsDir,
    LegacyCommandsDir,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillExecutionContext {
    Current,
    Fork,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSummary {
    pub name: String,
    pub description: Option<String>,
    pub path: PathBuf,
    pub source_root: PathBuf,
    pub origin: SkillOrigin,
    pub when_to_use: Option<String>,
    pub argument_hint: Option<String>,
    pub allowed_tools: Vec<String>,
    pub paths: Vec<String>,
    pub execution_context: Option<SkillExecutionContext>,
    pub version: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDefinition {
    pub summary: SkillSummary,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SkillCatalog {
    skills: Vec<SkillSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ParsedSkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
    when_to_use: Option<String>,
    argument_hint: Option<String>,
    allowed_tools: Vec<String>,
    paths: Vec<String>,
    execution_context: Option<SkillExecutionContext>,
    version: Option<String>,
    agent: Option<String>,
    model: Option<String>,
    effort: Option<String>,
}

impl SkillCatalog {
    #[must_use]
    pub fn discover(roots: &[PathBuf]) -> Self {
        let mut skills = Vec::new();
        for root in roots {
            if !root.exists() {
                continue;
            }
            let origin = skill_origin_for_root(root);
            match origin {
                SkillOrigin::SkillsDir => discover_skills_dir(root, &mut skills),
                SkillOrigin::LegacyCommandsDir => discover_commands_dir(root, &mut skills),
            }
        }
        skills.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.path.cmp(&right.path))
        });
        skills.dedup_by(|left, right| left.name == right.name && left.path == right.path);
        Self { skills }
    }

    #[must_use]
    pub fn skills(&self) -> &[SkillSummary] {
        &self.skills
    }

    #[must_use]
    pub fn resolve(&self, requested: &str) -> Option<&SkillSummary> {
        let requested = requested.trim();
        if requested.is_empty() {
            return None;
        }
        self.skills.iter().find(|skill| {
            skill.name.eq_ignore_ascii_case(requested)
                || skill
                    .name
                    .replace('-', "_")
                    .eq_ignore_ascii_case(&requested.replace('-', "_"))
        })
    }

    pub fn load(&self, requested: &str) -> Result<SkillDefinition, String> {
        let summary = self
            .resolve(requested)
            .ok_or_else(|| format!("unknown skill `{requested}`"))?
            .clone();
        let content = fs::read_to_string(&summary.path)
            .map_err(|error| format!("failed to read {}: {error}", summary.path.display()))?;
        Ok(SkillDefinition { summary, content })
    }

    #[must_use]
    pub fn conditional_matches<'a>(
        &'a self,
        cwd: &Path,
        touched_paths: &[PathBuf],
    ) -> Vec<&'a SkillSummary> {
        self.skills
            .iter()
            .filter(|skill| {
                !skill.paths.is_empty()
                    && touched_paths
                        .iter()
                        .any(|path| path_matches_skill(cwd, path, skill))
            })
            .collect()
    }

    #[must_use]
    pub fn render_listing(&self, context_window_tokens: Option<usize>) -> String {
        render_skill_listing(&self.skills, context_window_tokens)
    }
}

#[must_use]
pub fn render_skill_listing(
    skills: &[SkillSummary],
    context_window_tokens: Option<usize>,
) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let budget = context_window_tokens.map_or(DEFAULT_SKILL_CHAR_BUDGET, |tokens| {
        tokens
            .saturating_mul(SKILL_BUDGET_CHARS_PER_TOKEN)
            .saturating_mul(SKILL_BUDGET_CONTEXT_PERCENT)
            / 100
    });

    let mut remaining = budget;
    let mut lines = Vec::new();
    for skill in skills {
        let description = skill_listing_description(skill);
        let line = format!("- {}: {}", skill.name, description);
        let width = line.chars().count() + usize::from(!lines.is_empty());
        if width > remaining {
            break;
        }
        remaining = remaining.saturating_sub(width);
        lines.push(line);
    }

    if lines.is_empty() {
        return String::new();
    }

    format!(
        "The following skills are available for use with the Skill tool:\n\n{}\n\nInvoke via Skill(\"<name>\") for complete instructions.",
        lines.join("\n")
    )
}

fn skill_listing_description(skill: &SkillSummary) -> String {
    let description = match (&skill.description, &skill.when_to_use) {
        (Some(description), Some(when_to_use)) => format!("{description} - {when_to_use}"),
        (Some(description), None) => description.clone(),
        (None, Some(when_to_use)) => when_to_use.clone(),
        (None, None) => "No description provided".to_string(),
    };
    truncate_for_listing(&description, MAX_LISTING_DESC_CHARS)
}

fn truncate_for_listing(content: &str, max_chars: usize) -> String {
    let count = content.chars().count();
    if count <= max_chars {
        return content.to_string();
    }
    let truncated = content
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}…")
}

fn discover_skills_dir(root: &Path, skills: &mut Vec<SkillSummary>) {
    for entry in WalkDir::new(root).min_depth(1).max_depth(2) {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_file() || entry.file_name() != "SKILL.md" {
            continue;
        }
        let Some(parent) = entry.path().parent() else {
            continue;
        };
        let Some(fallback_name) = parent.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if let Some(summary) = build_skill_summary(
            entry.path(),
            root.to_path_buf(),
            SkillOrigin::SkillsDir,
            fallback_name,
        ) {
            skills.push(summary);
        }
    }
}

fn discover_commands_dir(root: &Path, skills: &mut Vec<SkillSummary>) {
    let mut markdown_files = Vec::new();
    let mut skill_directories = BTreeSet::new();

    for entry in WalkDir::new(root).min_depth(1) {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_file()
            || !entry
                .path()
                .extension()
                .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("md"))
        {
            continue;
        }
        let path = entry.into_path();
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("SKILL.md"))
        {
            if let Some(parent) = path.parent() {
                skill_directories.insert(parent.to_path_buf());
            }
        }
        markdown_files.push(path);
    }

    markdown_files.sort();
    for markdown_path in markdown_files {
        let Some(parent) = markdown_path.parent() else {
            continue;
        };
        let is_skill_file = markdown_path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("SKILL.md"));
        if skill_directories.contains(parent) && !is_skill_file {
            continue;
        }

        let fallback_name = legacy_command_name(root, &markdown_path);
        if let Some(summary) = build_skill_summary(
            &markdown_path,
            root.to_path_buf(),
            SkillOrigin::LegacyCommandsDir,
            &fallback_name,
        ) {
            skills.push(summary);
        }
    }
}

fn build_skill_summary(
    path: &Path,
    source_root: PathBuf,
    origin: SkillOrigin,
    fallback_name: &str,
) -> Option<SkillSummary> {
    let contents = fs::read_to_string(path).ok()?;
    let frontmatter = parse_skill_frontmatter(&contents);
    Some(SkillSummary {
        name: frontmatter
            .name
            .unwrap_or_else(|| fallback_name.to_string()),
        description: frontmatter
            .description
            .or_else(|| parse_description_without_frontmatter(&contents)),
        path: path.to_path_buf(),
        source_root,
        origin,
        when_to_use: frontmatter.when_to_use,
        argument_hint: frontmatter.argument_hint,
        allowed_tools: frontmatter.allowed_tools,
        paths: frontmatter.paths,
        execution_context: frontmatter.execution_context,
        version: frontmatter.version,
        agent: frontmatter.agent,
        model: frontmatter.model,
        effort: frontmatter.effort,
    })
}

fn skill_origin_for_root(root: &Path) -> SkillOrigin {
    if root
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("commands"))
    {
        SkillOrigin::LegacyCommandsDir
    } else {
        SkillOrigin::SkillsDir
    }
}

fn parse_skill_frontmatter(contents: &str) -> ParsedSkillFrontmatter {
    let mut lines = contents.lines();
    if lines.next().map(str::trim) != Some("---") {
        return ParsedSkillFrontmatter::default();
    }

    let mut parsed = ParsedSkillFrontmatter::default();
    let mut active_multiline_key: Option<String> = None;
    let mut active_list_values = Vec::new();

    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            flush_multiline_frontmatter(
                &mut parsed,
                &active_multiline_key,
                &mut active_list_values,
            );
            break;
        }

        if let Some(key) = &active_multiline_key {
            if trimmed.starts_with('-') {
                active_list_values.push(unquote_frontmatter_value(
                    trimmed.trim_start_matches('-').trim(),
                ));
                continue;
            }
            flush_multiline_frontmatter(&mut parsed, &Some(key.clone()), &mut active_list_values);
            active_multiline_key = None;
        }

        let Some((raw_key, raw_value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = normalize_frontmatter_key(raw_key);
        let value = raw_value.trim();

        if value.is_empty() && (key == "allowed_tools" || key == "paths") {
            active_multiline_key = Some(key);
            continue;
        }

        apply_frontmatter_value(&mut parsed, &key, value);
    }

    parsed
}

fn flush_multiline_frontmatter(
    parsed: &mut ParsedSkillFrontmatter,
    key: &Option<String>,
    values: &mut Vec<String>,
) {
    let Some(key) = key.as_deref() else {
        values.clear();
        return;
    };
    match key {
        "allowed_tools" if !values.is_empty() => parsed.allowed_tools = values.clone(),
        "paths" if !values.is_empty() => parsed.paths = normalize_path_patterns(values.clone()),
        _ => {}
    }
    values.clear();
}

fn apply_frontmatter_value(parsed: &mut ParsedSkillFrontmatter, key: &str, raw_value: &str) {
    match key {
        "name" => assign_if_non_empty(&mut parsed.name, raw_value),
        "description" => assign_if_non_empty(&mut parsed.description, raw_value),
        "when_to_use" => assign_if_non_empty(&mut parsed.when_to_use, raw_value),
        "argument_hint" => assign_if_non_empty(&mut parsed.argument_hint, raw_value),
        "allowed_tools" => {
            parsed.allowed_tools = parse_list_value(raw_value);
        }
        "paths" => {
            parsed.paths = normalize_path_patterns(parse_list_value(raw_value));
        }
        "context" | "execution_context" => {
            parsed.execution_context = match unquote_frontmatter_value(raw_value)
                .to_ascii_lowercase()
                .as_str()
            {
                "fork" => Some(SkillExecutionContext::Fork),
                "current" | "inline" => Some(SkillExecutionContext::Current),
                _ => None,
            };
        }
        "version" => assign_if_non_empty(&mut parsed.version, raw_value),
        "agent" => assign_if_non_empty(&mut parsed.agent, raw_value),
        "model" => assign_if_non_empty(&mut parsed.model, raw_value),
        "effort" => assign_if_non_empty(&mut parsed.effort, raw_value),
        _ => {}
    }
}

fn assign_if_non_empty(target: &mut Option<String>, raw_value: &str) {
    let value = unquote_frontmatter_value(raw_value);
    if !value.is_empty() {
        *target = Some(value);
    }
}

fn normalize_frontmatter_key(key: &str) -> String {
    key.trim().replace(['-', ' '], "_").to_ascii_lowercase()
}

fn parse_list_value(raw_value: &str) -> Vec<String> {
    let trimmed = raw_value.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(trimmed);

    inner
        .split(',')
        .map(unquote_frontmatter_value)
        .filter(|value| !value.is_empty())
        .collect()
}

fn normalize_path_patterns(patterns: Vec<String>) -> Vec<String> {
    patterns
        .into_iter()
        .map(|pattern| {
            let normalized = pattern.trim().replace('\\', "/");
            normalized
                .strip_suffix("/**")
                .unwrap_or(&normalized)
                .to_string()
        })
        .filter(|pattern| !pattern.is_empty() && pattern != "**")
        .collect()
}

fn unquote_frontmatter_value(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|trimmed| trimmed.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|trimmed| trimmed.strip_suffix('\''))
        })
        .unwrap_or(value)
        .trim()
        .to_string()
}

fn parse_description_without_frontmatter(contents: &str) -> Option<String> {
    let mut lines = contents.lines();
    if lines.next().map(str::trim) == Some("---") {
        for line in &mut lines {
            if line.trim() == "---" {
                break;
            }
        }
    } else {
        lines = contents.lines();
    }

    lines
        .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .map(|line| line.trim().to_string())
}

fn path_matches_skill(cwd: &Path, touched_path: &Path, skill: &SkillSummary) -> bool {
    let Ok(relative_path) = touched_path.strip_prefix(cwd) else {
        return false;
    };
    let relative = relative_path.to_string_lossy().replace('\\', "/");
    skill
        .paths
        .iter()
        .any(|pattern| pattern_matches_path(pattern, &relative))
}

fn pattern_matches_path(pattern: &str, relative_path: &str) -> bool {
    if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
        return Pattern::new(pattern)
            .ok()
            .is_some_and(|compiled| compiled.matches(relative_path));
    }

    relative_path == pattern
        || relative_path
            .strip_prefix(pattern)
            .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with('/'))
}

fn legacy_command_name(root: &Path, markdown_path: &Path) -> String {
    let relative = markdown_path
        .strip_prefix(root)
        .ok()
        .unwrap_or(markdown_path);

    let is_skill_file = markdown_path
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("SKILL.md"));

    let parts = if is_skill_file {
        relative
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
            .components()
            .filter_map(|component| component.as_os_str().to_str())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>()
    } else {
        let mut parts = relative
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
            .components()
            .filter_map(|component| component.as_os_str().to_str())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if let Some(stem) = markdown_path.file_stem().and_then(|value| value.to_str()) {
            parts.push(stem.to_string());
        }
        parts
    };

    if parts.is_empty() {
        markdown_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown")
            .to_string()
    } else {
        parts.join(":")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        render_skill_listing, SkillCatalog, SkillExecutionContext, SkillOrigin, SkillSummary,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("opencowork-skills-{stamp}"))
    }

    #[test]
    fn discovers_skill_files_from_roots() {
        let root = temp_dir();
        let skill_dir = root.join("planner");
        fs::create_dir_all(&skill_dir).expect("skill dir");
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: \"Planning guidance\"\nwhen_to_use: \"Need a plan\"\nargument-hint: \"goal\"\nallowed-tools: [read_file, grep_search]\ncontext: fork\nmodel: gpt-5.4\neffort: high\n---\nUse a plan.\n",
        )
        .expect("skill file");

        let catalog = SkillCatalog::discover(std::slice::from_ref(&root));
        assert_eq!(catalog.skills().len(), 1);
        assert_eq!(catalog.skills()[0].name, "planner");
        assert_eq!(
            catalog.skills()[0].description.as_deref(),
            Some("Planning guidance")
        );
        assert_eq!(
            catalog.skills()[0].when_to_use.as_deref(),
            Some("Need a plan")
        );
        assert_eq!(catalog.skills()[0].argument_hint.as_deref(), Some("goal"));
        assert_eq!(
            catalog.skills()[0].allowed_tools,
            vec!["read_file".to_string(), "grep_search".to_string()]
        );
        assert!(catalog.skills()[0].paths.is_empty());
        assert_eq!(
            catalog.skills()[0].execution_context,
            Some(SkillExecutionContext::Fork)
        );
        assert_eq!(catalog.skills()[0].version.as_deref(), None);
        assert_eq!(catalog.skills()[0].agent.as_deref(), None);
        assert_eq!(catalog.skills()[0].model.as_deref(), Some("gpt-5.4"));
        assert_eq!(catalog.skills()[0].effort.as_deref(), Some("high"));
        let loaded = catalog.load("planner").expect("load skill");
        assert!(loaded.content.contains("Use a plan."));
        assert_eq!(catalog.resolve("Planner").expect("resolve").name, "planner");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discovers_legacy_command_markdown_files() {
        let root = temp_dir();
        let commands_root = root.join("commands");
        fs::create_dir_all(&commands_root).expect("commands dir");
        fs::write(
            commands_root.join("review.md"),
            "---\ndescription: \"Review workflow\"\nversion: \"1.0\"\nagent: reviewer\nallowed-tools:\n  - read_file\n  - grep_search\n---\nInspect the diff and report risks.\n",
        )
        .expect("command skill");
        let nested_dir = commands_root.join("security");
        fs::create_dir_all(&nested_dir).expect("nested command dir");
        fs::write(
            nested_dir.join("audit.md"),
            "Audit security-sensitive code paths.\n",
        )
        .expect("nested command");
        let scoped_dir = commands_root.join("ops").join("deploy");
        fs::create_dir_all(&scoped_dir).expect("scoped command dir");
        fs::write(
            scoped_dir.join("SKILL.md"),
            "---\ndescription: \"Deployment workflow\"\n---\nHandle deployments carefully.\n",
        )
        .expect("scoped skill");
        fs::write(
            scoped_dir.join("ignored.md"),
            "This file should be ignored because SKILL.md exists.\n",
        )
        .expect("ignored markdown");

        let catalog = SkillCatalog::discover(std::slice::from_ref(&commands_root));
        assert_eq!(catalog.skills().len(), 3);
        assert_eq!(catalog.skills()[0].origin, SkillOrigin::LegacyCommandsDir);
        assert_eq!(catalog.skills()[0].name, "ops:deploy");
        assert_eq!(catalog.skills()[1].name, "review");
        assert_eq!(catalog.skills()[2].name, "security:audit");
        assert_eq!(
            catalog.skills()[1].allowed_tools,
            vec!["read_file".to_string(), "grep_search".to_string()]
        );
        assert_eq!(catalog.skills()[1].version.as_deref(), Some("1.0"));
        assert_eq!(catalog.skills()[1].agent.as_deref(), Some("reviewer"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn matches_conditional_skills_for_touched_paths() {
        let root = temp_dir();
        let skill_dir = root.join("refactor");
        fs::create_dir_all(&skill_dir).expect("skill dir");
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: \"Refactor guidance\"\npaths:\n  - src/**\n  - Cargo.toml\n---\nApply refactor rules.\n",
        )
        .expect("skill file");
        let catalog = SkillCatalog::discover(std::slice::from_ref(&root));
        let cwd = PathBuf::from("D:/workspace/demo");
        let matched = catalog.conditional_matches(
            &cwd,
            &[
                cwd.join("src").join("main.rs"),
                cwd.join("README.md"),
                cwd.join("Cargo.toml"),
            ],
        );
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].name, "refactor");
        assert_eq!(
            matched[0].paths,
            vec!["src".to_string(), "Cargo.toml".to_string()]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn renders_skill_listing_within_budget() {
        let listing = render_skill_listing(
            &[
                SkillSummary {
                    name: "review".to_string(),
                    description: Some("Review code changes".to_string()),
                    path: PathBuf::from("review/SKILL.md"),
                    source_root: PathBuf::from("skills"),
                    origin: SkillOrigin::SkillsDir,
                    when_to_use: Some("When you need a focused review".to_string()),
                    argument_hint: None,
                    allowed_tools: Vec::new(),
                    paths: Vec::new(),
                    execution_context: None,
                    version: None,
                    agent: None,
                    model: None,
                    effort: None,
                },
                SkillSummary {
                    name: "deploy".to_string(),
                    description: Some("Deploy the current application".to_string()),
                    path: PathBuf::from("deploy/SKILL.md"),
                    source_root: PathBuf::from("skills"),
                    origin: SkillOrigin::SkillsDir,
                    when_to_use: None,
                    argument_hint: None,
                    allowed_tools: Vec::new(),
                    paths: Vec::new(),
                    execution_context: None,
                    version: None,
                    agent: None,
                    model: None,
                    effort: None,
                },
            ],
            Some(200_000),
        );

        assert!(listing.contains("The following skills are available"));
        assert!(listing.contains("review"));
        assert!(listing.contains("Invoke via Skill(\"<name>\")"));
    }
}

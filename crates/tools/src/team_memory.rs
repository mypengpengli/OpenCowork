use opencowork_runtime::{default_config_home, project_team_memory_root};
use regex::Regex;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

struct SecretRule {
    id: &'static str,
    label: &'static str,
    pattern: &'static str,
}

const SECRET_RULES: &[SecretRule] = &[
    SecretRule {
        id: "aws-access-token",
        label: "AWS Access Token",
        pattern: r"\b((?:A3T[A-Z0-9]|AKIA|ASIA|ABIA|ACCA)[A-Z2-7]{16})\b",
    },
    SecretRule {
        id: "gcp-api-key",
        label: "GCP API Key",
        pattern: r#"\b(AIza[\w-]{35})(?:[\x60'"\\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "azure-ad-client-secret",
        label: "Azure AD Client Secret",
        pattern: r#"(?:^|[\\'"`\s>=:(,)])([a-zA-Z0-9_~.]{3}\dQ~[a-zA-Z0-9_~.-]{31,34})(?:$|[\\'"`\s<),])"#,
    },
    SecretRule {
        id: "digitalocean-pat",
        label: "DigitalOcean PAT",
        pattern: r#"\b(dop_v1_[a-f0-9]{64})(?:[\x60'"\\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "digitalocean-access-token",
        label: "DigitalOcean Access Token",
        pattern: r#"\b(doo_v1_[a-f0-9]{64})(?:[\x60'"\\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "anthropic-api-key",
        label: "Anthropic API Key",
        pattern: r#"\b(sk-ant-api03-[a-zA-Z0-9_\-]{93}AA)(?:[\x60'"\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "anthropic-admin-api-key",
        label: "Anthropic Admin API Key",
        pattern: r#"\b(sk-ant-admin01-[a-zA-Z0-9_\-]{93}AA)(?:[\x60'"\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "openai-api-key",
        label: "OpenAI API Key",
        pattern: r#"\b(sk-(?:proj|svcacct|admin)-(?:[A-Za-z0-9_-]{74}|[A-Za-z0-9_-]{58})T3BlbkFJ(?:[A-Za-z0-9_-]{74}|[A-Za-z0-9_-]{58})\b|sk-[a-zA-Z0-9]{20}T3BlbkFJ[a-zA-Z0-9]{20})(?:[\x60'"\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "huggingface-access-token",
        label: "HuggingFace Access Token",
        pattern: r#"\b(hf_[a-zA-Z]{34})(?:[\x60'"\\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "github-pat",
        label: "GitHub PAT",
        pattern: r"ghp_[0-9a-zA-Z]{36}",
    },
    SecretRule {
        id: "github-fine-grained-pat",
        label: "GitHub Fine Grained PAT",
        pattern: r"github_pat_\w{82}",
    },
    SecretRule {
        id: "github-app-token",
        label: "GitHub App Token",
        pattern: r"(?:ghu|ghs)_[0-9a-zA-Z]{36}",
    },
    SecretRule {
        id: "github-oauth",
        label: "GitHub OAuth",
        pattern: r"gho_[0-9a-zA-Z]{36}",
    },
    SecretRule {
        id: "github-refresh-token",
        label: "GitHub Refresh Token",
        pattern: r"ghr_[0-9a-zA-Z]{36}",
    },
    SecretRule {
        id: "gitlab-pat",
        label: "GitLab PAT",
        pattern: r"glpat-[\w-]{20}",
    },
    SecretRule {
        id: "gitlab-deploy-token",
        label: "GitLab Deploy Token",
        pattern: r"gldt-[0-9a-zA-Z_\-]{20}",
    },
    SecretRule {
        id: "slack-bot-token",
        label: "Slack Bot Token",
        pattern: r"xoxb-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*",
    },
    SecretRule {
        id: "slack-user-token",
        label: "Slack User Token",
        pattern: r"xox[pe](?:-[0-9]{10,13}){3}-[a-zA-Z0-9-]{28,34}",
    },
    SecretRule {
        id: "slack-app-token",
        label: "Slack App Token",
        pattern: r"(?i)xapp-\d-[A-Z0-9]+-\d+-[a-z0-9]+",
    },
    SecretRule {
        id: "twilio-api-key",
        label: "Twilio API Key",
        pattern: r"SK[0-9a-fA-F]{32}",
    },
    SecretRule {
        id: "sendgrid-api-token",
        label: "SendGrid API Token",
        pattern: r#"\b(SG\.[a-zA-Z0-9=_\-.]{66})(?:[\x60'"\\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "npm-access-token",
        label: "NPM Access Token",
        pattern: r#"\b(npm_[a-zA-Z0-9]{36})(?:[\x60'"\\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "pypi-upload-token",
        label: "PyPI Upload Token",
        pattern: r"pypi-AgEIcHlwaS5vcmc[\w-]{50,200}",
    },
    SecretRule {
        id: "databricks-api-token",
        label: "Databricks API Token",
        pattern: r#"\b(dapi[a-f0-9]{32}(?:-\d)?)(?:[\x60'"\\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "hashicorp-tf-api-token",
        label: "HashiCorp TF API Token",
        pattern: r"[a-zA-Z0-9]{14}\.atlasv1\.[a-zA-Z0-9\-_=:]{60,70}",
    },
    SecretRule {
        id: "pulumi-api-token",
        label: "Pulumi API Token",
        pattern: r#"\b(pul-[a-f0-9]{40})(?:[\x60'"\\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "postman-api-token",
        label: "Postman API Token",
        pattern: r#"\b(PMAK-[a-fA-F0-9]{24}-[a-fA-F0-9]{34})(?:[\x60'"\\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "grafana-api-key",
        label: "Grafana API Key",
        pattern: r#"\b(eyJrIjoi[A-Za-z0-9+/]{70,400}={0,3})(?:[\x60'"\\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "grafana-cloud-api-token",
        label: "Grafana Cloud API Token",
        pattern: r#"\b(glc_[A-Za-z0-9+/]{32,400}={0,3})(?:[\x60'"\\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "grafana-service-account-token",
        label: "Grafana Service Account Token",
        pattern: r#"\b(glsa_[A-Za-z0-9]{32}_[A-Fa-f0-9]{8})(?:[\x60'"\\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "sentry-user-token",
        label: "Sentry User Token",
        pattern: r#"\b(sntryu_[a-f0-9]{64})(?:[\x60'"\\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "stripe-access-token",
        label: "Stripe Access Token",
        pattern: r#"\b((?:sk|rk)_(?:test|live|prod)_[a-zA-Z0-9]{10,99})(?:[\x60'"\\s;]|\\[nr]|$)"#,
    },
    SecretRule {
        id: "shopify-access-token",
        label: "Shopify Access Token",
        pattern: r"shpat_[a-fA-F0-9]{32}",
    },
    SecretRule {
        id: "shopify-shared-secret",
        label: "Shopify Shared Secret",
        pattern: r"shpss_[a-fA-F0-9]{32}",
    },
    SecretRule {
        id: "private-key",
        label: "Private Key",
        pattern: r"(?is)-----BEGIN[ A-Z0-9_-]{0,100}PRIVATE KEY(?: BLOCK)?-----[\s\S-]{64,}?-----END[ A-Z0-9_-]{0,100}PRIVATE KEY(?: BLOCK)?-----",
    },
];

static COMPILED_RULES: OnceLock<Vec<(&'static SecretRule, Regex)>> = OnceLock::new();

pub fn guard_team_memory_write(file_path: &str, content: &str) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let config_home = default_config_home();
    guard_team_memory_write_at(file_path, content, &cwd, &config_home)
}

pub fn is_team_memory_path(file_path: &str) -> Result<bool, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let config_home = default_config_home();
    is_team_memory_path_at(file_path, &cwd, &config_home)
}

fn guard_team_memory_write_at(
    file_path: &str,
    content: &str,
    cwd: &Path,
    config_home: &Path,
) -> Result<(), String> {
    if !is_team_memory_path_at(file_path, cwd, config_home)? {
        return Ok(());
    }

    let matches = scan_for_secrets(content);
    if matches.is_empty() {
        return Ok(());
    }

    let labels = matches.into_iter().collect::<Vec<_>>().join(", ");
    Err(format!(
        "Content contains potential secrets ({labels}) and cannot be written to team memory. Team memory is shared with all repository collaborators. Remove the sensitive content and try again."
    ))
}

fn scan_for_secrets(content: &str) -> Vec<&'static str> {
    let mut matches = Vec::new();
    let mut seen = BTreeSet::new();
    for (rule, regex) in compiled_rules() {
        if seen.contains(rule.id) {
            continue;
        }
        if regex.is_match(content) {
            seen.insert(rule.id);
            matches.push(rule.label);
        }
    }
    matches
}

fn compiled_rules() -> &'static [(&'static SecretRule, Regex)] {
    COMPILED_RULES.get_or_init(|| {
        SECRET_RULES
            .iter()
            .map(|rule| {
                (
                    rule,
                    Regex::new(rule.pattern)
                        .unwrap_or_else(|error| panic!("invalid secret rule {}: {error}", rule.id)),
                )
            })
            .collect()
    })
}

fn is_team_memory_path_at(file_path: &str, cwd: &Path, config_home: &Path) -> Result<bool, String> {
    let root = normalize_path(&project_team_memory_root(config_home, cwd));
    let candidate = normalize_path(&resolve_path(file_path, cwd));
    if !candidate.starts_with(&root) {
        return Ok(false);
    }

    let real_root = match fs::canonicalize(&root) {
        Ok(real_root) => real_root,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(error.to_string()),
    };
    let real_candidate = canonicalize_deepest_existing(&candidate)?;
    if real_candidate == real_root || real_candidate.starts_with(&real_root) {
        Ok(true)
    } else {
        Err(format!(
            "Path escapes team memory directory via symlink: `{}`",
            candidate.display()
        ))
    }
}

fn resolve_path(file_path: &str, cwd: &Path) -> PathBuf {
    let path = PathBuf::from(file_path);
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

fn canonicalize_deepest_existing(path: &Path) -> Result<PathBuf, String> {
    let mut current = path.to_path_buf();
    let mut tail = Vec::<OsString>::new();

    loop {
        match fs::canonicalize(&current) {
            Ok(real) => {
                let mut joined = real;
                for component in tail.iter().rev() {
                    joined.push(component);
                }
                return Ok(joined);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::InvalidInput
                ) =>
            {
                let Some(parent) = current.parent() else {
                    return Ok(path.to_path_buf());
                };
                let Some(name) = current.file_name() else {
                    return Ok(path.to_path_buf());
                };
                tail.push(name.to_os_string());
                if parent == current {
                    return Ok(path.to_path_buf());
                }
                current = parent.to_path_buf();
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(std::path::MAIN_SEPARATOR_STR),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(segment) => normalized.push(segment),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::{guard_team_memory_write_at, project_team_memory_root};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("opencowork-tools-{prefix}-{stamp}"))
    }

    #[test]
    fn allows_non_secret_team_memory_write() {
        let cwd = temp_dir("cwd");
        let config_home = temp_dir("config");
        let team_root = project_team_memory_root(&config_home, &cwd);
        fs::create_dir_all(&team_root).expect("team root");
        let path = team_root.join("notes.md");

        let result = guard_team_memory_write_at(
            &path.display().to_string(),
            "Shared deployment notes only.",
            &cwd,
            &config_home,
        );

        assert!(result.is_ok());
        let _ = fs::remove_dir_all(cwd);
        let _ = fs::remove_dir_all(config_home);
    }

    #[test]
    fn blocks_team_memory_write_with_secret() {
        let cwd = temp_dir("cwd");
        let config_home = temp_dir("config");
        let team_root = project_team_memory_root(&config_home, &cwd);
        fs::create_dir_all(&team_root).expect("team root");
        let path = team_root.join("notes.md");

        let result = guard_team_memory_write_at(
            &path.display().to_string(),
            "Token: ghp_1234567890abcdefghijklmnopqrstuvwxyz",
            &cwd,
            &config_home,
        );

        assert!(result
            .err()
            .is_some_and(|error| error.contains("GitHub PAT")));
        let _ = fs::remove_dir_all(cwd);
        let _ = fs::remove_dir_all(config_home);
    }

    #[test]
    fn ignores_non_team_memory_path() {
        let cwd = temp_dir("cwd");
        let config_home = temp_dir("config");
        let path = cwd.join("notes.md");

        let result = guard_team_memory_write_at(
            &path.display().to_string(),
            "Token: ghp_1234567890abcdefghijklmnopqrstuvwxyz",
            &cwd,
            &config_home,
        );

        assert!(result.is_ok());
        let _ = fs::remove_dir_all(cwd);
        let _ = fs::remove_dir_all(config_home);
    }
}

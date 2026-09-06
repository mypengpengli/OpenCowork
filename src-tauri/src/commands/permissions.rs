use super::*;

/// Whitelist mode accepts plain argv only. Shell programs, expansion and composition
/// need full access; a first executable match never authorizes another command.
pub(super) fn plain_argv(command: &str) -> Result<Vec<String>, String> {
    if command.chars().any(|c| ";&|<>`$%^!\n\r\0".contains(c)) {
        return Err("Shell composition/expansion is not permitted by command prefix rules".into());
    }
    let mut quote = None;
    let mut word = String::new();
    let mut words = Vec::new();
    let mut active = false;
    for c in command.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => word.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                active = true;
            }
            None if c.is_whitespace() => {
                if active {
                    words.push(std::mem::take(&mut word));
                    active = false;
                }
            }
            None => {
                word.push(c);
                active = true;
            }
        }
    }
    if quote.is_some() {
        return Err("Unclosed command quote".into());
    }
    if active {
        words.push(word);
    }
    if words.is_empty() {
        return Err("Empty command".into());
    }
    Ok(words)
}

pub(super) fn permitted(command: &str, rules: &[String]) -> bool {
    let Ok(argv) = plain_argv(command) else {
        return false;
    };
    let executable = argv[0]
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let executable = executable.trim_end_matches(".exe");
    if executable.ends_with(".cmd") || executable.ends_with(".bat") {
        return false;
    }
    if [
        "cmd",
        "powershell",
        "pwsh",
        "bash",
        "sh",
        "zsh",
        "wscript",
        "cscript",
        "start",
    ]
    .contains(&executable)
    {
        return false;
    }
    if argv.iter().skip(1).any(|a| {
        ["-c", "-e", "--eval", "-command", "-encodedcommand"]
            .contains(&a.to_ascii_lowercase().as_str())
    }) {
        return false;
    }
    rules.iter().any(|rule| {
        let Ok(prefix) = plain_argv(rule) else {
            return false;
        };
        if prefix.iter().any(|s| s.contains('*') || s.contains('?')) || prefix.len() > argv.len() {
            return false;
        }
        // Interpreters require at least an explicitly authorized script argument.
        if ["python", "python3", "node", "ruby", "perl"].contains(&executable) && prefix.len() < 2 {
            return false;
        }
        prefix.iter().zip(&argv).enumerate().all(|(i, (a, b))| {
            if i == 0 && cfg!(windows) {
                a.eq_ignore_ascii_case(b)
            } else {
                a == b
            }
        })
    })
}

pub(super) fn canonical_target(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return path.canonicalize().map_err(|e| e.to_string());
    }
    let parent = path.parent().ok_or("Path has no parent")?;
    let name = path.file_name().ok_or("Path has no name")?;
    Ok(canonical_target(parent)?.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_compound_command_cannot_use_first_word_authorization() {
        assert!(!permitted("git status && whoami", &["git status".into()]));
        assert!(!permitted("git status\nwhoami", &["git".into()]));
        assert!(!permitted("git $(whoami)", &["git".into()]));
    }
    #[test]
    fn test_prefix_checks_arguments_and_quotes() {
        assert!(permitted("git status --short", &["git status".into()]));
        assert!(!permitted("git push", &["git status".into()]));
        assert!(permitted(
            "python \"scripts/my task.py\"",
            &["python \"scripts/my task.py\"".into()]
        ));
        assert!(!permitted("python -c 'print(1)'", &["python".into()]));
        assert!(!permitted("cmd /c whoami", &["cmd".into()]));
    }
}

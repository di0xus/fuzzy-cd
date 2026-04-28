pub fn script_for(shell: &str) -> Option<String> {
    match shell {
        "bash" => Some(BASH.to_string()),
        "zsh" => Some(ZSH.to_string()),
        "fish" => Some(fish_script()),
        _ => None,
    }
}

/// Returns the fish version as a tuple (major, minor), or None if unavailable.
fn fish_version() -> Option<(u32, u32)> {
    let output = std::process::Command::new("fish")
        .arg("--version")
        .output()
        .ok()?;
    let version_str = String::from_utf8_lossy(&output.stdout);
    // fish --version outputs e.g. "fish, version 3.6.0"
    let version_str = version_str.trim();
    // Find the version number after "version "
    let version_part = version_str
        .rsplit(',')
        .next()?
        .trim_start_matches("version ");
    let parts: Vec<&str> = version_part.split('.').collect();
    let major = parts.first()?.parse().ok()?;
    let minor = parts
        .get(1)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    Some((major, minor))
}

fn fish_script() -> String {
    let use_abbr = fish_version()
        .map(|(major, minor)| major > 2 || (major == 2 && minor >= 9))
        .unwrap_or(false);

    if use_abbr {
        r#"# hop fish integration
function __hop_chpwd --on-variable PWD
    command hop add -- "$PWD" >/dev/null 2>&1
end

abbr --add h=hop
"#
        .to_string()
    } else {
        r#"# hop fish integration
function __hop_chpwd --on-variable PWD
    command hop add -- "$PWD" >/dev/null 2>&1
end

alias h=hop
"#
        .to_string()
    }
}

/// Detect the user's shell from `$SHELL`.
pub fn detect_shell() -> Option<&'static str> {
    let shell = std::env::var("SHELL").ok()?;
    if shell.ends_with("zsh") {
        Some("zsh")
    } else if shell.ends_with("bash") {
        Some("bash")
    } else if shell.ends_with("fish") {
        Some("fish")
    } else {
        None
    }
}

pub struct VerifyReport {
    pub ok: bool,
    pub lines: Vec<String>,
}

/// Sanity-check the user's shell integration without actually loading it.
/// Detects the shell, confirms a script exists, and prints how to wire it up.
pub fn verify() -> VerifyReport {
    let mut lines = Vec::new();
    let Some(shell) = detect_shell() else {
        return VerifyReport {
            ok: false,
            lines: vec![
                "✗ could not detect shell from $SHELL".into(),
                "  pick one manually: hop init bash|zsh|fish".into(),
            ],
        };
    };
    lines.push(format!("✓ detected shell: {}", shell));

    if script_for(shell).is_none() {
        lines.push(format!("✗ no init script for {}", shell));
        return VerifyReport { ok: false, lines };
    }
    lines.push(format!("✓ init script available for {}", shell));

    let hint = match shell {
        "bash" => "add to ~/.bashrc:    eval \"$(hop init bash)\"",
        "zsh" => "add to ~/.zshrc:     eval \"$(hop init zsh)\"",
        "fish" => "add to ~/.config/fish/config.fish:   hop init fish | source",
        _ => unreachable!(),
    };
    lines.push(format!("→ {}", hint));
    lines.push("  then open a new shell and run: hop doctor".into());

    VerifyReport { ok: true, lines }
}

const BASH: &str = r#"# hop bash integration
__hop_chpwd() { command hop add -- "$PWD" >/dev/null 2>&1; }
case ":${PROMPT_COMMAND:-}:" in
  *:__hop_chpwd:*) ;;
  *) PROMPT_COMMAND="__hop_chpwd${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;;
esac

alias h=hop
"#;

const ZSH: &str = r#"# hop zsh integration
autoload -U add-zsh-hook
__hop_chpwd() { command hop add -- "$PWD" >/dev/null 2>&1 }
add-zsh-hook chpwd __hop_chpwd

alias h=hop
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_for_known_shells() {
        assert!(script_for("bash").is_some());
        assert!(script_for("zsh").is_some());
        assert!(script_for("fish").is_some());
        assert!(script_for("nushell").is_none());
    }

    #[test]
    fn each_script_calls_hop_binary() {
        for shell in ["bash", "zsh", "fish"] {
            let s = script_for(shell).unwrap();
            assert!(s.contains("command hop"), "{shell} missing binary call");
            assert!(s.contains("alias h=hop"), "{shell} missing h alias");
            assert!(!s.contains("fuzzy-cd"), "{shell} still references old name");
        }
    }

    #[test]
    fn verify_reports_something() {
        let r = verify();
        assert!(!r.lines.is_empty());
    }
}

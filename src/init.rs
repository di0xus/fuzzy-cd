pub fn script_for(shell: &str) -> Option<String> {
    match shell {
        "bash" => Some(BASH.to_string()),
        "zsh" => Some(ZSH.to_string()),
        "fish" => Some(fish_script()),
        _ => None,
    }
}

fn fish_script() -> String {
    // Note: deliberately no `abbr h=hop` here. An abbreviation would expand
    // `h` to `hop` before the function is looked up, so the function (which
    // performs the actual `cd`) would never run — and fish >= 4 rejects the
    // `h=hop` form outright with a startup error.
    r#"# hop fish integration
# Session start — dirs visited after this get a recency bonus in scoring.
set -gx HOP_SESSION (command date +%s)

function __hop_chpwd --on-variable PWD
    command hop add -- "$PWD" >/dev/null 2>&1
end

functions -e h 2>/dev/null; function h
    set -l dir (command hop $argv)
    [ -n "$dir" ] && cd -- "$dir"
end
"#
    .to_string()
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
# Session start — dirs visited after this get a recency bonus in scoring.
export HOP_SESSION="$(command date +%s)"
# bash has no chpwd hook, so PROMPT_COMMAND is used; record only when the
# directory actually changed, otherwise visit counts inflate per prompt.
__hop_chpwd() {
    if [[ "$PWD" != "${__hop_last:-}" ]]; then
        __hop_last=$PWD
        command hop add -- "$PWD" >/dev/null 2>&1
    fi
}
case ":${PROMPT_COMMAND:-}:" in
  *:__hop_chpwd:*) ;;
  *) PROMPT_COMMAND="__hop_chpwd${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;;
esac

unalias h 2>/dev/null || true

h() {
    local dir
    dir=$(command hop "$@")
    [[ -n "$dir" ]] && builtin cd -- "$dir"
}
"#;

const ZSH: &str = r#"# hop zsh integration
# Session start — dirs visited after this get a recency bonus in scoring.
export HOP_SESSION="$(command date +%s)"
autoload -U add-zsh-hook
__hop_chpwd() { command hop add -- "$PWD" >/dev/null 2>&1 }
add-zsh-hook chpwd __hop_chpwd

unalias h 2>/dev/null || true

h() {
    local dir
    dir=$(command hop "$@")
    [[ -n "$dir" ]] && builtin cd -- "$dir"
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_for_known_shells() {
        assert!(script_for("bash").is_some());
        assert!(script_for("zsh").is_some());
        assert!(script_for("fish").is_some());
        assert!(script_for("unknown-shell").is_none());
    }

    #[test]
    fn each_script_calls_hop_binary() {
        for shell in ["bash", "zsh", "fish"] {
            let s = script_for(shell).unwrap();
            assert!(s.contains("hop"), "{shell} missing hop call");
            let has_h = if shell == "fish" {
                s.contains("function h")
            } else {
                s.contains("h()")
            };
            assert!(has_h, "{shell} missing h function definition");
            assert!(!s.contains("fuzzy-cd"), "{shell} still references old name");
        }
    }

    #[test]
    fn verify_reports_something() {
        let r = verify();
        assert!(!r.lines.is_empty());
    }
}

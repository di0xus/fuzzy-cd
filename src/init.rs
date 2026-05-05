pub fn script_for(shell: &str) -> Option<String> {
    match shell {
        "bash" => Some(BASH.to_string()),
        "zsh" => Some(ZSH.to_string()),
        "fish" => Some(fish_script()),
        "nushell" | "nu" => Some(nushell_script()),
        "elvish" => Some(elvish_script()),
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

    let script = r#"# hop fish integration
function __hop_chpwd --on-variable PWD
    command hop add -- "$PWD" >/dev/null 2>&1
end

functions -e h 2>/dev/null; function h
    set -l dir (command hop $argv)
    [ -n "$dir" ] && cd -- "$dir"
end
"#;
    if use_abbr {
        format!(
            r#"abbr --add h=hop

{script}"#
        )
    } else {
        script.to_string()
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
    } else if shell.ends_with("nushell") || shell.ends_with("nu") {
        Some("nushell")
    } else if shell.ends_with("elvish") {
        Some("elvish")
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
        "nushell" => "add to ~/.config/nushell/config.nu:   mkdir ~/.local/bin; hop init nushell | save ~/.local/bin/hop.nu",
        "elvish" => "add to ~/.config/elvish/rc.elv:   use ~/.local/share/hop/hop.elv",
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

unalias h 2>/dev/null || true

h() {
    local dir
    dir=$(command hop "$@")
    [[ -n "$dir" ]] && builtin cd -- "$dir"
}
"#;

const ZSH: &str = r#"# hop zsh integration
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

/// Nushell script — saves to ~/.local/bin/hop.nu and must be sourced from config.nu.
fn nushell_script() -> String {
    r#"# hop nushell integration
# Save this to ~/.local/bin/hop.nu, then add to your config.nu:
#   mkdir ~/.local/bin
#   hop init nushell | save ~/.local/bin/hop.nu
# And in ~/.config/nushell/config.nu:
#   use ~/.local/bin/hop.nu

export def --env h [query?: string] {
    let dir = (hop $query | str trim)
    if ($dir != "") {
        cd $dir
    }
}

# Record visits automatically on cd
export env --env HOP_AUTO_ADD {
    cd $in
    ^hop add -- $in
}
"#.to_string()
}

/// Elvish script — save to ~/.local/share/hop/hop.elv and load from rc.elv.
/// In ~/.config/elvish/rc.elv:
///   mkdir -p ~/.local/share/hop
///   hop init elvish > ~/.local/share/hop/hop.elv
///   use ~/.local/share/hop/hop.elv
fn elvish_script() -> String {
    r#"# hop elvish integration
# Save this to ~/.local/share/hop/hop.elv, then in your rc.elv add:
#   use ~/.local/share/hop/hop.elv

set @hop-kept-aliases = []

fn h [@args] {
    var dir = (command-external hop $@args | str trim)
    if (not-eq $dir "") {
        cd $dir
    }
}

# Auto-record visits on directory change
set edit:prompt:hook = $@edit:prompt:hook [...prev;
    var pwd = (to-string $pwd)
    if (and (not-eq $pwd "") (path:is-dir $pwd)) {
       触#触call-external "hop" "add" "--" $pwd > "/dev/null" "2>" "&1"
    }
    $prev
]
"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_for_known_shells() {
        assert!(script_for("bash").is_some());
        assert!(script_for("zsh").is_some());
        assert!(script_for("fish").is_some());
        assert!(script_for("nushell").is_some());
        assert!(script_for("nu").is_some());
        assert!(script_for("elvish").is_some());
        assert!(script_for("nushell").is_some());
        assert!(script_for("unknown-shell").is_none());
    }

    #[test]
    fn each_script_calls_hop_binary() {
        for shell in ["bash", "zsh", "fish", "nushell", "elvish"] {
            let s = script_for(shell).unwrap();
            assert!(s.contains("hop"), "{shell} missing hop call");
            // zsh/bash use h(), fish uses "function h", nushell uses "def --env h"
            let has_h = if shell == "fish" {
                s.contains("function h")
            } else if shell == "nushell" {
                s.contains("def --env h")
            } else if shell == "elvish" {
                s.contains("fn h")
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

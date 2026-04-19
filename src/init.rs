pub fn script_for(shell: &str) -> Option<&'static str> {
    match shell {
        "bash" => Some(BASH),
        "zsh" => Some(ZSH),
        "fish" => Some(FISH),
        _ => None,
    }
}

const BASH: &str = r#"# hop bash integration
__hop_chpwd() { command hop add -- "$PWD" >/dev/null 2>&1; }
case ":${PROMPT_COMMAND:-}:" in
  *:__hop_chpwd:*) ;;
  *) PROMPT_COMMAND="__hop_chpwd${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;;
esac

__hop_cd() {
    if [ $# -eq 0 ]; then
        local dir
        dir=$(command hop pick)
        [ -n "$dir" ] && builtin cd -- "$dir"
        return
    fi
    case "$1" in
        -|..|.|~|~/) builtin cd -- "$@"; return ;;
    esac
    if [ -d "$1" ]; then
        builtin cd -- "$@"
        return
    fi
    local dir
    dir=$(command hop p -- "$@")
    if [ -n "$dir" ]; then
        builtin cd -- "$dir"
    else
        builtin cd -- "$@"
    fi
}
alias cd='__hop_cd'
"#;

const ZSH: &str = r#"# hop zsh integration
autoload -U add-zsh-hook
__hop_chpwd() { command hop add -- "$PWD" >/dev/null 2>&1 }
add-zsh-hook chpwd __hop_chpwd

__hop_cd() {
    if (( $# == 0 )); then
        local dir
        dir=$(command hop pick)
        [[ -n "$dir" ]] && builtin cd -- "$dir"
        return
    fi
    case "$1" in
        -|..|.|~|~/) builtin cd -- "$@"; return ;;
    esac
    if [[ -d "$1" ]]; then
        builtin cd -- "$@"
        return
    fi
    local dir
    dir=$(command hop p -- "$@")
    if [[ -n "$dir" ]]; then
        builtin cd -- "$dir"
    else
        builtin cd -- "$@"
    fi
}
alias cd='__hop_cd'
"#;

const FISH: &str = r#"# hop fish integration
function __hop_chpwd --on-variable PWD
    command hop add -- "$PWD" >/dev/null 2>&1
end

function __hop_cd
    if test (count $argv) -eq 0
        set -l dir (command hop pick)
        if test -n "$dir"
            builtin cd -- "$dir"
        end
        return
    end
    switch $argv[1]
        case '-' '..' '.' '~' '~/'
            builtin cd -- $argv
            return
    end
    if test -d "$argv[1]"
        builtin cd -- $argv
        return
    end
    set -l dir (command hop p -- $argv)
    if test -n "$dir"
        builtin cd -- "$dir"
    else
        builtin cd -- $argv
    end
end
alias cd=__hop_cd
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
            assert!(s.contains("__hop_cd"), "{shell} missing cd wrapper");
            assert!(!s.contains("fuzzy-cd"), "{shell} still references old name");
        }
    }
}

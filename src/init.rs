pub fn script_for(shell: &str) -> Option<&'static str> {
    match shell {
        "bash" => Some(BASH),
        "zsh" => Some(ZSH),
        "fish" => Some(FISH),
        _ => None,
    }
}

const BASH: &str = r#"# fuzzy-cd bash integration
_fuzzy_cd_chpwd() { command fuzzy-cd add -- "$PWD" >/dev/null 2>&1; }
case ":${PROMPT_COMMAND:-}:" in
  *:_fuzzy_cd_chpwd:*) ;;
  *) PROMPT_COMMAND="_fuzzy_cd_chpwd${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;;
esac

fcd() {
    if [ $# -eq 0 ]; then
        local dir
        dir=$(command fuzzy-cd pick)
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
    dir=$(command fuzzy-cd p -- "$@")
    if [ -n "$dir" ]; then
        builtin cd -- "$dir"
    else
        builtin cd -- "$@"
    fi
}
alias cd='fcd'
"#;

const ZSH: &str = r#"# fuzzy-cd zsh integration
autoload -U add-zsh-hook
_fuzzy_cd_chpwd() { command fuzzy-cd add -- "$PWD" >/dev/null 2>&1 }
add-zsh-hook chpwd _fuzzy_cd_chpwd

fcd() {
    if (( $# == 0 )); then
        local dir
        dir=$(command fuzzy-cd pick)
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
    dir=$(command fuzzy-cd p -- "$@")
    if [[ -n "$dir" ]]; then
        builtin cd -- "$dir"
    else
        builtin cd -- "$@"
    fi
}
alias cd='fcd'
"#;

const FISH: &str = r#"# fuzzy-cd fish integration
function __fuzzy_cd_chpwd --on-variable PWD
    command fuzzy-cd add -- "$PWD" >/dev/null 2>&1
end

function fcd
    if test (count $argv) -eq 0
        set -l dir (command fuzzy-cd pick)
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
    set -l dir (command fuzzy-cd p -- $argv)
    if test -n "$dir"
        builtin cd -- "$dir"
    else
        builtin cd -- $argv
    end
end
alias cd=fcd
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
    fn each_script_defines_fcd() {
        for shell in ["bash", "zsh", "fish"] {
            assert!(script_for(shell).unwrap().contains("fcd"));
        }
    }
}

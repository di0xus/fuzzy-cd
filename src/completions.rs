//! Tab-completion scripts for the `hop` CLI.
//!
//! Emitted by `hop completions <bash|zsh|fish>` and consumed by the user's
//! shell (usually via their rc file or a completions directory).

pub fn script_for(shell: &str) -> Option<&'static str> {
    match shell {
        "bash" => Some(BASH),
        "zsh" => Some(ZSH),
        "fish" => Some(FISH),
        _ => None,
    }
}

const BASH: &str = r#"# hop bash completion
_hop() {
    local cur prev words cword
    _init_completion || return

    local subcommands="p pick add rm forget zap book bookmark history recent top import prune clear stats reindex doctor init completions help"
    local shells="bash zsh fish"

    if [[ $cword -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "$subcommands --help -h" -- "$cur") )
        return
    fi

    case "${words[1]}" in
        init|completions)
            if [[ $cword -eq 2 ]]; then
                COMPREPLY=( $(compgen -W "$shells" -- "$cur") )
            fi
            ;;
        import)
            if [[ $cword -eq 2 ]]; then
                COMPREPLY=( $(compgen -W "fasd zsh" -- "$cur") )
            else
                COMPREPLY=( $(compgen -f -- "$cur") )
            fi
            ;;
        book|bookmark)
            if [[ $cword -eq 2 ]]; then
                local aliases
                aliases=$(command hop book list 2>/dev/null | awk '{print $1}')
                COMPREPLY=( $(compgen -W "list rm $aliases" -- "$cur") )
            elif [[ "${words[2]}" == "rm" ]]; then
                local aliases
                aliases=$(command hop book list 2>/dev/null | awk '{print $1}')
                COMPREPLY=( $(compgen -W "$aliases" -- "$cur") )
            else
                COMPREPLY=( $(compgen -d -- "$cur") )
            fi
            ;;
        add|rm)
            COMPREPLY=( $(compgen -d -- "$cur") )
            ;;
        prune)
            COMPREPLY=( $(compgen -W "--dry-run" -- "$cur") )
            ;;
        clear)
            COMPREPLY=( $(compgen -W "--force" -- "$cur") )
            ;;
    esac
}
complete -F _hop hop
"#;

const ZSH: &str = r#"#compdef hop
# hop zsh completion

_hop() {
    local -a subcommands shells
    subcommands=(
        'p:pick best match'
        'pick:pick best match'
        'add:record a visit'
        'rm:remove path from history'
        'forget:fuzzy-find and remove'
        'zap:alias of forget'
        'book:manage bookmarks'
        'bookmark:manage bookmarks'
        'history:top by visits'
        'recent:last visited'
        'top:top 10'
        'import:import from fasd or zsh'
        'prune:remove stale paths'
        'clear:wipe history'
        'stats:DB stats'
        'reindex:rebuild filesystem index'
        'doctor:diagnose setup'
        'init:emit shell integration'
        'completions:emit completion script'
        'help:show help'
    )
    shells=(bash zsh fish)

    _arguments -C \
        '1: :->cmd' \
        '*:: :->args'

    case $state in
        cmd)
            _describe 'hop command' subcommands
            ;;
        args)
            case $words[1] in
                init|completions)
                    _describe 'shell' shells
                    ;;
                import)
                    if (( CURRENT == 2 )); then
                        _values 'source' fasd zsh
                    else
                        _files
                    fi
                    ;;
                book|bookmark)
                    if (( CURRENT == 2 )); then
                        local -a aliases
                        aliases=(${(f)"$(command hop book list 2>/dev/null | awk '{print $1}')"})
                        _values 'action' list rm $aliases
                    elif [[ $words[2] == rm ]]; then
                        local -a aliases
                        aliases=(${(f)"$(command hop book list 2>/dev/null | awk '{print $1}')"})
                        _values 'alias' $aliases
                    else
                        _path_files -/
                    fi
                    ;;
                add|rm)
                    _path_files -/
                    ;;
                prune)
                    _values 'flag' --dry-run
                    ;;
                clear)
                    _values 'flag' --force
                    ;;
            esac
            ;;
    esac
}
compdef _hop hop
"#;

const FISH: &str = r#"# hop fish completion
complete -c hop -f

function __hop_needs_command
    set -l cmd (commandline -opc)
    test (count $cmd) -eq 1
end

function __hop_using_command
    set -l cmd (commandline -opc)
    test (count $cmd) -ge 2; and test "$cmd[2]" = "$argv[1]"
end

function __hop_bookmark_aliases
    command hop book list 2>/dev/null | awk '{print $1}'
end

# subcommands
complete -c hop -n __hop_needs_command -a p         -d 'pick best match'
complete -c hop -n __hop_needs_command -a pick      -d 'pick best match'
complete -c hop -n __hop_needs_command -a add       -d 'record a visit'
complete -c hop -n __hop_needs_command -a rm        -d 'remove path from history'
complete -c hop -n __hop_needs_command -a forget    -d 'fuzzy-find and remove'
complete -c hop -n __hop_needs_command -a zap       -d 'alias of forget'
complete -c hop -n __hop_needs_command -a book      -d 'manage bookmarks'
complete -c hop -n __hop_needs_command -a bookmark  -d 'manage bookmarks'
complete -c hop -n __hop_needs_command -a history   -d 'top by visits'
complete -c hop -n __hop_needs_command -a recent    -d 'last visited'
complete -c hop -n __hop_needs_command -a top       -d 'top 10'
complete -c hop -n __hop_needs_command -a import    -d 'import from fasd/zsh'
complete -c hop -n __hop_needs_command -a prune     -d 'remove stale paths'
complete -c hop -n __hop_needs_command -a clear     -d 'wipe history'
complete -c hop -n __hop_needs_command -a stats     -d 'DB stats'
complete -c hop -n __hop_needs_command -a reindex   -d 'rebuild filesystem index'
complete -c hop -n __hop_needs_command -a doctor    -d 'diagnose setup'
complete -c hop -n __hop_needs_command -a init      -d 'emit shell integration'
complete -c hop -n __hop_needs_command -a completions -d 'emit completion script'
complete -c hop -n __hop_needs_command -a help      -d 'show help'
complete -c hop -n __hop_needs_command -l help      -d 'show help'

# init / completions → shell
complete -c hop -n '__hop_using_command init'        -a 'bash zsh fish'
complete -c hop -n '__hop_using_command completions' -a 'bash zsh fish'

# import source
complete -c hop -n '__hop_using_command import' -a 'fasd zsh'

# book subcommand
complete -c hop -n '__hop_using_command book'     -a 'list rm'
complete -c hop -n '__hop_using_command book'     -a '(__hop_bookmark_aliases)'
complete -c hop -n '__hop_using_command bookmark' -a 'list rm'
complete -c hop -n '__hop_using_command bookmark' -a '(__hop_bookmark_aliases)'

# add / rm → directories
complete -c hop -n '__hop_using_command add' -fa '(__fish_complete_directories)'
complete -c hop -n '__hop_using_command rm'  -fa '(__fish_complete_directories)'

# flags
complete -c hop -n '__hop_using_command prune' -l dry-run -d 'preview stale paths'
complete -c hop -n '__hop_using_command clear' -l force   -d 'skip confirmation'
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
    fn each_script_mentions_hop() {
        for shell in ["bash", "zsh", "fish"] {
            let s = script_for(shell).unwrap();
            assert!(
                s.contains("hop"),
                "{shell} completion missing hop references"
            );
        }
    }

    #[test]
    fn bash_registers_completion() {
        let s = script_for("bash").unwrap();
        assert!(s.contains("complete -F _hop hop"));
    }

    #[test]
    fn zsh_has_compdef() {
        let s = script_for("zsh").unwrap();
        assert!(s.contains("#compdef hop"));
    }

    #[test]
    fn fish_registers_complete() {
        let s = script_for("fish").unwrap();
        assert!(s.contains("complete -c hop"));
    }
}

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

    local subcommands="p pick add rm forget zap book bookmark history recent top score list export import prune clear stats reindex doctor explain update init completions help"
    local shells="bash zsh fish"

    if [[ $cword -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "$subcommands --help -h" -- "$cur") )
        return
    fi

    case "${words[1]}" in
        init|completions)
            if [[ $cword -eq 2 ]]; then
                COMPREPLY=( $(compgen -W "$shells --shell --verify" -- "$cur") )
            fi
            ;;
        import)
            if [[ $cword -eq 2 ]]; then
                COMPREPLY=( $(compgen -W "fasd zsh autojump zoxide thefuck --dry-run" -- "$cur") )
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
        add)
            if [[ "$cur" == --* ]]; then
                COMPREPLY=( $(compgen -W "--dry-run" -- "$cur") )
            else
                COMPREPLY=( $(compgen -d -- "$cur") )
            fi
            ;;
        rm)
            COMPREPLY=( $(compgen -d -- "$cur") )
            ;;
        score)
            if [[ "$cur" == --* ]]; then
                COMPREPLY=( $(compgen -W "--json" -- "$cur") )
            fi
            ;;
        list)
            if [[ "$cur" == --* ]]; then
                COMPREPLY=( $(compgen -W "--limit --json" -- "$cur") )
            fi
            ;;
        export)
            if [[ "$cur" == --* ]]; then
                COMPREPLY=( $(compgen -W "--format" -- "$cur") )
            elif [[ $cword -eq 3 ]]; then
                COMPREPLY=( $(compgen -W "json csv tsv" -- "$cur") )
            fi
            ;;
        update)
            if [[ "$cur" == --* ]]; then
                COMPREPLY=( $(compgen -W "--dry-run" -- "$cur") )
            fi
            ;;
        prune)
            COMPREPLY=( $(compgen -W "--dry-run --quiet" -- "$cur") )
            ;;
        clear)
            COMPREPLY=( $(compgen -W "--force" -- "$cur") )
            ;;
        stats)
            COMPREPLY=( $(compgen -W "--verbose -V" -- "$cur") )
            ;;
        history|recent)
            if [[ "$cur" != -* ]]; then
                COMPREPLY=( $(compgen -W "20 10 50 100" -- "$cur") )
            fi
            ;;
        explain)
            COMPREPLY=( $(compgen -f -- "$cur") )
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
        'add:record a visit (use --dry-run to preview)'
        'rm:remove path from history'
        'forget:fuzzy-find and remove'
        'zap:alias of forget'
        'book:manage bookmarks'
        'bookmark:manage bookmarks'
        'history:top by visits (default 20, use 10/50/100)'
        'recent:last visited (default 20)'
        'top:top 10'
        'score:show per-component score breakdown'
        'list:list all scored matches'
        'export:dump history/bookmarks in json/csv/tsv format'
        'import:import from fasd/zsh/autojump/zoxide/thefuck'
        'prune:remove stale paths'
        'clear:wipe history'
        'stats:DB stats'
        'reindex:rebuild filesystem index'
        'doctor:diagnose setup'
        'explain:show score breakdown for query'
        'update:self-update to latest release'
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
                init)
                    _arguments \
                        '1:shell:_values "shell" bash zsh fish' \
                        '--shell[specify shell explicitly]' \
                        '--verify[check shell integration]'
                    ;;
                completions)
                    _arguments \
                        '1:shell:_values "shell" bash zsh fish' \
                        '--shell[specify shell explicitly]'
                    ;;
                import)
                    if (( CURRENT == 2 )); then
                        _values 'source' fasd zsh autojump zoxide thefuck
                    elif (( CURRENT == 3 )); then
                        _values 'flag' --dry-run
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
                add)
                    _arguments \
                        '--dry-run[preview what would be added]' \
                        '1:path:_path_files -/'
                    ;;
                rm)
                    _path_files -/
                    ;;
                score)
                    _arguments \
                        '--json[output JSON]' \
                        '1:query: '
                    ;;
                list)
                    _arguments \
                        '--json[output JSON]' \
                        '--limit[limit results]:number' \
                        '1:query: '
                    ;;
                export)
                    _arguments \
                        '--format[specify format]:format:_values "format" json csv tsv' \
                        '1: :'
                    ;;
                update)
                    _arguments \
                        '--dry-run[preview what would be installed]'
                    ;;
                prune)
                    _values 'flag' --dry-run --quiet
                    ;;
                clear)
                    _values 'flag' --force
                    ;;
                stats)
                    _values 'flag' --verbose -V
                    ;;
                history|recent)
                    _arguments '1:limit:_values "limit" 10 20 50 100'
                    ;;
                explain)
                    _arguments '1:query: '
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
complete -c hop -n __hop_needs_command -a p           -d 'pick best match'
complete -c hop -n __hop_needs_command -a pick        -d 'pick best match'
complete -c hop -n __hop_needs_command -a add          -d 'record a visit'
complete -c hop -n __hop_needs_command -a rm          -d 'remove path from history'
complete -c hop -n __hop_needs_command -a forget      -d 'fuzzy-find and remove'
complete -c hop -n __hop_needs_command -a zap         -d 'alias of forget'
complete -c hop -n __hop_needs_command -a book        -d 'manage bookmarks'
complete -c hop -n __hop_needs_command -a bookmark    -d 'manage bookmarks'
complete -c hop -n __hop_needs_command -a history     -d 'top by visits (default 20)'
complete -c hop -n __hop_needs_command -a recent      -d 'last visited (default 20)'
complete -c hop -n __hop_needs_command -a top         -d 'top 10'
complete -c hop -n __hop_needs_command -a score        -d 'show per-component score breakdown'
complete -c hop -n __hop_needs_command -a list        -d 'list all scored matches'
complete -c hop -n __hop_needs_command -a export      -d 'dump history/bookmarks in json/csv/tsv'
complete -c hop -n __hop_needs_command -a import      -d 'import from fasd/zsh/autojump/zoxide/thefuck'
complete -c hop -n __hop_needs_command -a prune       -d 'remove stale paths'
complete -c hop -n __hop_needs_command -a clear       -d 'wipe history'
complete -c hop -n __hop_needs_command -a stats       -d 'DB stats'
complete -c hop -n __hop_needs_command -a reindex     -d 'rebuild filesystem index'
complete -c hop -n __hop_needs_command -a doctor       -d 'diagnose setup'
complete -c hop -n __hop_needs_command -a explain      -d 'show score breakdown for query'
complete -c hop -n __hop_needs_command -a update      -d 'self-update to latest release'
complete -c hop -n __hop_needs_command -a init        -d 'emit shell integration'
complete -c hop -n __hop_needs_command -a completions  -d 'emit completion script'
complete -c hop -n __hop_needs_command -a help         -d 'show help'
complete -c hop -n __hop_needs_command -l help         -d 'show help'

# init / completions → shell with flags
complete -c hop -n '__hop_using_command init'        -a 'bash zsh fish' -d 'shell name'
complete -c hop -n '__hop_using_command init'        -l shell          -d 'specify shell explicitly'
complete -c hop -n '__hop_using_command init'        -l verify         -d 'check shell integration'
complete -c hop -n '__hop_using_command completions' -a 'bash zsh fish' -d 'shell name'
complete -c hop -n '__hop_using_command completions' -l shell          -d 'specify shell explicitly'

# import source with --dry-run
complete -c hop -n '__hop_using_command import' -a 'fasd zsh autojump zoxide thefuck' -d 'import source'
complete -c hop -n '__hop_using_command import' -l dry-run -d 'preview import without writing'

# book subcommand
complete -c hop -n '__hop_using_command book'     -a 'list rm' -d 'bookmark action'
complete -c hop -n '__hop_using_command book'     -l json -s j -d 'output JSON'
complete -c hop -n '__hop_using_command book rm'  -a '(__hop_bookmark_aliases)'
complete -c hop -n '__hop_using_command bookmark' -a 'list rm' -d 'bookmark action'
complete -c hop -n '__hop_using_command bookmark' -l json -s j -d 'output JSON'
complete -c hop -n '__hop_using_command bookmark rm' -a '(__hop_bookmark_aliases)'

# add with --dry-run
complete -c hop -n '__hop_using_command add' -l dry-run -d 'preview what would be added'
complete -c hop -n '__hop_using_command add' -fa '(__fish_complete_directories)'

# rm → directories
complete -c hop -n '__hop_using_command rm'  -fa '(__fish_complete_directories)'

# score with --json
complete -c hop -n '__hop_using_command score' -l json -d 'output JSON'

# list with --json and --limit
complete -c hop -n '__hop_using_command list' -l json   -d 'output JSON'
complete -c hop -n '__hop_using_command list' -l limit -s l -d 'limit results'

# export with --format
complete -c hop -n '__hop_using_command export' -l format -d 'specify format' -a 'json csv tsv'

# update with --dry-run
complete -c hop -n '__hop_using_command update' -l dry-run -d 'preview what would be installed'

# prune with flags
complete -c hop -n '__hop_using_command prune' -l dry-run -d 'preview stale paths'
complete -c hop -n '__hop_using_command prune' -l quiet   -d 'suppress progress output'

# clear with --force
complete -c hop -n '__hop_using_command clear' -l force -d 'skip confirmation'

# stats with --verbose/-V
complete -c hop -n '__hop_using_command stats' -l verbose -d 'show verbose stats'
complete -c hop -n '__hop_using_command stats' -s V        -d 'show verbose stats'

# history/recent with count suggestions
complete -c hop -n '__hop_using_command history' -a '10 20 50 100' -d 'limit count'
complete -c hop -n '__hop_using_command recent'   -a '10 20 50 100' -d 'limit count'

# explain → query
complete -c hop -n '__hop_using_command explain' -f -d 'query string'
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

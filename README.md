# hop

**Fuzzy directory jumper for your terminal.**

Type a fragment, hit enter, go anywhere. `hop` learns your habits and gets smarter over time.

```
~ $ h dl
~/Downloads

~/Downloads $ h proj
~/code/work/project
```

No more `cd ../../../long/path`. No more memorizing aliases.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/doriangironde/hop/main/install.sh | bash
```

Then add shell integration:

| Shell | Add this to your config file |
|-------|------------------------------|
| Bash  | `eval "$(hop init bash)"`    |
| Zsh   | `eval "$(hop init zsh)"`     |
| Fish  | `hop init fish \| source`    |

Restart your shell. That's it — `h` is now a function that jumps to directories.

## Quick usage

```
h proj          → jump to best match from history
hop /tmp        → literal path works too
hop ~/code      → literal paths with ~ work like cd
hop ..          → parent directory
h src doc       → all words must match (AND)
h alpha /       → subdirectory of cwd starting with "alpha"
hop             → open the interactive picker
hop book w ~/code/work   → bookmark a directory
h w             → jump to bookmark
hop score proj  → see why a match won
h -             → jump to the previous directory (like cd -)
hop forget      → pick from history to delete, interactively
```

## Command reference

```
hop <query>                  Jump to best match (prints path)
hop -                        Previous directory (like cd -)
hop p|pick [query]           Same; empty query opens picker
hop add <path>               Record a visit
hop rm <path>                Remove from history (exact path)
hop forget [query]           Remove from history; empty query opens picker
hop book [alias] [path]      Set or resolve a bookmark
hop book list [--json]       List all bookmarks
hop book rm <alias>          Delete a bookmark
hop book edit <alias>        Edit alias, path, or description
hop history [n]              Top n by visits (default 20)
hop recent [n]               Last n visited (default 20)
hop score <query> [--json]   Show per-component score breakdown
hop list <query> [--limit N] [--json]  List all scored matches
hop export [--format json|csv|tsv]  Dump history/bookmarks
hop import fasd|autojump|zoxide|zsh <file>  Import from another tool
hop prune [--dry-run]        Remove stale (deleted) paths
hop clear [--force]          Wipe history (prompts by default)
hop stats                    DB stats
hop doctor                   Diagnose setup
hop init <bash|zsh|fish>     Emit shell integration
hop completions <bash|zsh|fish>  Emit tab-completion script
```

Queries support fzf-style modifiers: `/regex/` matches a pattern, `!pattern` excludes it. Directories visited in your current shell session (the init script sets `$HOP_SESSION`) get a scoring bonus, so what you're working on right now wins over old favorites.

## Documentation

All the details are in the wiki:

- [Installation](https://github.com/doriangironde/hop/wiki/Installation) — one-liner, from source, uninstall
- [Shell Setup](https://github.com/doriangironde/hop/wiki/Shell-Setup) — bash, zsh, fish
- [Usage](https://github.com/doriangironde/hop/wiki/Usage) — all commands with examples
- [Configuration](https://github.com/doriangironde/hop/wiki/Configuration) — config.toml reference
- [Importing](https://github.com/doriangironde/hop/wiki/Importing) — migrate from zsh/fasd/autojump/zoxide
- [Troubleshooting](https://github.com/doriangironde/hop/wiki/Troubleshooting) — common issues and fixes
- [Changelog](https://github.com/doriangironde/hop/wiki/Changelog) — release notes

## Verify

```bash
hop doctor
```

## Data

- **macOS**: `~/Library/Application Support/hop/hop.db`
- **Linux**: `~/.local/share/hop/hop.db`

## License

MIT

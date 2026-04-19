# fuzzy-cd

Smart directory jump — fuzzy search your directory history, bookmarks, and filesystem.

## Features

- **Fuzzy matching** — `fuzzy-cd clov` finds `~/cloven`
- **Auto-learning** — records every `cd` through the shell function, improves over time
- **Bookmarks** — `fuzzy-cd book proj ~/projects` then `fuzzy-cd proj` jumps instantly
- **Smart scoring** — visit frequency, recency, git repo detection, basename matching
- **Import** — pull in history from `fasd` or `zsh`
- **SQLite storage** — fast, local, no cloud

## Install

```bash
cargo install fuzzy-cd
```

## Shell Setup

**Fish** — add to `~/.config/fish/config.fish`:

```fish
function fcd
    if test (count $argv) -eq 1
        if test "$argv[1]" = ".." -o "$argv[1]" = "." -o "$argv[1]" = "~" -o "$argv[1]" = "~/" -o "$argv[1]" = "-"
            builtin cd $argv[1]
            return
        end
    end
    if test (count $argv) -ge 1
        and string match -qr "^/" -- "$argv[1]"
        builtin cd $argv
        return
    end
    set -l dir (fuzzy-cd p $argv)
    if test -n "$dir"
        builtin cd $dir
        return
    end
    if test (count $argv) -ge 1
        builtin cd $argv
    end
end
abbr --add cd fcd
```

**Zsh** — add to `~/.zshrc`:

```bash
fcd() {
    local dir
    dir=$(fuzzy-cd p "$@")
    [ -n "$dir" ] && cd "$dir"
}
alias cd='fcd'
```

Then `source ~/.config/fish/config.fish` or restart your shell.

## Usage

```bash
# Jump to best match
fuzzy-cd clov              # → /Users/you/cloven
fuzzy-cd doc               # → /Users/you/Documents

# Bookmarks (highest priority)
fuzzy-cd book proj ~/projects
fuzzy-cd book list
fuzzy-cd book rm proj

# History
fuzzy-cd history           # top 20 by visits
fuzzy-cd history 5        # top 5
fuzzy-cd recent           # last 20 visited

# Management
fuzzy-cd add ~/some/path  # manually add to history
fuzzy-cd rm ~/some/path   # remove from history
fuzzy-cd clear            # clear all history

# Import from existing tools
fuzzy-cd import fasd ~/.fasd
fuzzy-cd import zsh ~/.zsh_history

# Stats
fuzzy-cd stats

# Indexing
fuzzy-cd --reindex        # rebuild filesystem index (skips ~/Library, node_modules, etc.)
```

## How scoring works

Score = fuzzy_match × frequency × recency × bonuses

- **Visits** — more visits = higher rank
- **Recency** — visited today ranks higher than 30 days ago
- **Git repos** — detected and boosted
- **Basename match** — matching the folder name directly scores much higher
- **Bookmarks** — 3× weight, exact alias match wins immediately
- **Short paths** — `~/a` beats `~/very/deep/nested/path`

## Database

SQLite at `~/.fuzzy-cd/fuzzy-cd.db`. WAL mode enabled for safety.

## Build

```bash
cargo build --release
./target/release/fuzzy-cd
```

## License

MIT

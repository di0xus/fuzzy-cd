# fuzzy-cd

Smart directory navigation with fuzzy search and visit history.

`fuzzy-cd` maintains a SQLite database of your directory visits and filesystem index, then lets you jump to any directory with a short fuzzy query.

## Features

- **Fuzzy matching** — `fcd proj` finds `~/Documents/projects/myproj`
- **Visit frequency** — popular directories rank higher
- **Interactive picker** — arrow keys + enter when multiple matches
- **Lightweight** — single Rust binary, SQLite storage in `~/.fuzzy-cd/`
- **Shell integration** — drop-in `cd` replacement

## Install

```bash
cargo install fuzzy-cd
```

Or download a binary from the Releases page.

## Shell Setup

```bash
# Add to ~/.zshrc (or ~/.bashrc)
source <(fuzzy-cd init)

# Or manually:
fcd() {
    local dir
    dir=$(fuzzy-cd pick "$1")
    [ -n "$dir" ] && cd "$dir"
}
alias cd='fcd'
```

Then restart your shell or `source ~/.zshrc`.

## Usage

```
fuzzy-cd [query]        Interactive fuzzy search, prints selected path
fuzzy-cd pick <query>  Non-interactive: prints best match or empty
fuzzy-cd add <path>    Manually add a path to history
fuzzy-cd history       Show most visited directories
fuzzy-cd --clear-history   Clear visit history
fuzzy-cd --reindex     Rebuild directory index
fuzzy-cd --help        Show this help
```

## How it works

- Visit history stored in `~/.fuzzy-cd/history.db`
- On first run, indexes all directories under `~` (skips hidden dirs, `node_modules`, `target`, `.git`)
- Match score = fuzzy_match_score × visit_count × recency_weight
- History is updated automatically when you `cd` through the shell function

## Build

```bash
cargo build --release
./target/release/fuzzy-cd
```

## License

MIT

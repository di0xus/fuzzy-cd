# fuzzy-cd — SPEC.md

## Concept & Vision

`fuzzy-cd` is a lightweight CLI tool that makes navigating directories feel like searching with spotlight. Instead of typing full paths or remembering where things live, you type fragments and it fuzzy-matches your history and filesystem. It maintains a visit frequency database so your most-used directories bubble to the top. The shell integration feels native — `fcd myproj` reads like `cd` but with intelligence behind it.

**Feel:** Fast (sub-50ms response), quiet (minimal output), reliable. The kind of tool a developer reaches for every day without thinking.

## Design Language

- **CLI output:** One-line matches with bold highlighted match segments, no tables or ASCII art
- **Color:** Terminal-native colors — bold for the matched substring, dim for the rest of the path
- **Sound:** None
- **Completion:** Interactive fuzzy search via fzf-style filtering as user types
- **Error messages:** Short, human-readable: "No matches for: foo"

## Core Features

### 1. Interactive Fuzzy Search (`fcd`)

- Runs interactively in the terminal using readline/fzf-style filtering
- User types a query, matches appear in real-time
- Shows top 10 matches, sorted by visit frequency + recency
- Enter selects and prints the chosen path to stdout (for shell integration)
- Esc or Ctrl+C cancels and prints nothing

**Matching logic:**
- Fuzzy match: `sr` matches `~/src`, `~/Site Web Perso`, `~/Shutter Encoder`
- Score = frequency × recency × fuzzy match score
- Case-insensitive by default

### 2. Visit History

- SQLite database at `~/.fuzzy-cd/history.db`
- Schema: `paths(id INTEGER PRIMARY KEY, path TEXT UNIQUE, visits INTEGER, last_visited REAL)`
- Every successful `cd` integration increments visit count and updates timestamp
- `fcd --clear-history` wipes the database

### 3. Shell Integration

Adds to `.zshrc` (or `.bashrc`):
```bash
fcd() {
  local dir
  dir=$(fuzzy-cd pick "$1")
  [ -n "$dir" ] && cd "$dir"
}
alias cd='fcd'
```

Or a lighter touch — just use `fuzzy-cd pick <query>` directly and capture its output.

### 4. Auto-jump on Single Match

If the query matches exactly one directory with high confidence, auto-selects it without interactive mode.

### 5. Directory Indexing

- Indexes `~` by default (configurable)
- Runs in background on first run to build cache
- `fuzzy-cd --reindex` forces rebuild
- Index is a SQLite table: `index(path TEXT, basename TEXT, parent TEXT)`

## User Interactions

| Command | Behavior |
|---|---|
| `fuzzy-cd` (no args) | Opens interactive picker |
| `fuzzy-cd pick <query>` | Non-interactive: prints best match path or empty |
| `fuzzy-cd add <path>` | Manually add a path to history |
| `fuzzy-cd history` | List top 20 visited directories |
| `fuzzy-cd --clear-history` | Clear visit history |
| `fuzzy-cd --reindex` | Rebuild directory index |
| `fuzzy-cd --help` | Show help |

## Component Inventory

### `fuzzy-cd` binary (Rust)

Single binary, no dependencies beyond standard library + SQLite (via rusqlite or similar).

**States:**
- No history DB → creates it silently on first run
- Empty index → triggers background indexing, shows "Indexing ~..."
- Query with matches → shows interactive picker
- Query with one high-confidence match → auto-returns
- Query with no matches → prints nothing, exits 1

### Interactive Picker

- Renders using terminal capabilities (cursor movement, clear line)
- Each match shown as: `[score] /full/path/with/matched/bold`
- Up/Down arrows navigate
- Enter confirms selection
- Typing filters in real-time
- Esc/Ctrl+C exits with no output

## Technical Approach

**Language:** Rust (single binary, fast startup, no runtime needed)

**Key crates:**
- `rusqlite` for SQLite
- `fuzzy-matcher` for fuzzy matching logic
- `readline` or direct termios for interactive input
- Standard library `std::process::Command` for shell integration

**Database:** `~/.fuzzy-cd/history.db`

**Files created:**
- `~/.fuzzy-cd/history.db` — visit history
- `~/.fuzzy-cd/index.db` — directory index
- `~/.fuzzy-cd/config.toml` — optional config (index paths, max results)

**Build:** `cargo build --release` → `fuzzy-cd` binary

## Installation

```bash
cargo install fuzzy-cd
# Then add to shell:
echo 'source <(fuzzy-cd init)' >> ~/.zshrc
```

Or a simple install script:
```bash
curl -fsSL https://raw.githubusercontent.com/user/fuzzy-cd/main/install.sh | bash
```

## Out of Scope

- Auto-completing in the middle of a typed command (requires shell plugin, not a CLI)
- Cloud sync of history
- Per-repo directory shortcuts
- Tmux/terminal session awareness

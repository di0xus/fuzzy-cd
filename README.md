# hop

**A smarter way to navigate your terminal.**

Type a fragment of a directory name — `h work` — and hop jumps you to the best match from your history. No more `cd ../../../long/path`. It learns where you go and gets better over time.

```
~ $ h dl
~/Downloads $

~/Downloads $ h proj
~/code/work/projects $
```

---

## Features

- **Fuzzy matching** — `h work`, `h 444`, `h abc` all work. Matches against visit frequency, recency, and bookmark priority.
- **Bookmarks** — name your frequent directories with short aliases.
- **Smart ranking** — the more you visit a folder, the higher it scores. Recent visits beat old ones.
- **Import existing history** — pull in your zsh, fasd, autojump, zoxide, or thefuck history on first run.
- **Interactive picker** — `h` or `hop` with no args opens a searchable picker with arrow keys.
- **Self-updating** — `hop update` fetches the latest release from GitHub.
- **Shell-agnostic** — works with Fish, Zsh, and Bash.
- **Private** — all data stays on your machine. SQLite only.

---

## Install

### One-liner (recommended)

```bash
curl -fsSL https://codeberg.org/dioxus/hop/raw/branch/main/install.sh | bash
```

Downloads the latest binary for your platform, places it in `~/.local/bin`, and prints instructions if you need to add that to your `PATH`.

### From source (requires Rust)

```bash
git clone https://codeberg.org/dioxus/hop
cd hop
cargo install --path .
```

---

## Shell setup

Restart your shell after installing, then run the init command for your shell:

**Fish** (`~/.config/fish/config.fish`):
```fish
hop init fish | source
```

**Zsh** (`~/.zshrc`):
```zsh
eval "$(hop init zsh)"
```

**Bash** (`~/.bashrc`):
```bash
eval "$(hop init bash)"
```

That's it — `h` is now an alias for `hop`. Every directory you visit is recorded automatically.

---

## Usage

### Navigate

```bash
h proj          # fuzzy match against your history
hop /tmp        # real paths work too
h ..            # relative paths still work
h -             # previous directory
hop              # open the interactive picker
hop pick         # same as above (shortcut: hop p)
h                # same as above (shorter: h)
```

### Add and remove

```bash
hop add ~/code/project      # manually record a visit
hop rm ~/old/project        # remove from history
hop forget ~/old/project     # alias for rm
hop clear                    # wipe all history (asks for confirm)
hop clear --force            # wipe without asking
```

### Bookmarks

```bash
hop book work ~/code/work    # create a bookmark
hop book dot ~/.config       # short alias for a config dir

h work                      # bookmark short-circuits fuzzy matching
hop book list               # see all bookmarks
hop book rm work            # delete a bookmark
```

### History and search

```bash
hop recent                  # last 20 visited directories
hop history                 # top 20 by visit count
hop list proj               # list all entries matching "proj"
hop score proj              # show why a query scored the way it did
hop explain proj            # fuzzy breakdown + scores
```

### Import

Seed hop with your existing shell history on first install:

```bash
hop import zsh ~/.zsh_history
hop import fasd ~/.fasd
hop import autojump ~/.local/share/autojump/autojump.txt
hop import zoxide ~/.local/share/zoxide/data.zzd
hop import thefuck ~/.thefuck_config
```

### Housekeeping

```bash
hop prune                   # remove entries pointing to deleted directories
hop prune --dry-run         # preview what would be removed (no changes made)
hop prune --quiet           # suppress output on success
hop update                  # check GitHub for a new release and upgrade
hop doctor                  # sanity-check your install
```

### Export

```bash
hop export                  # dump everything as JSON
hop export --format csv    # or CSV
hop export --format tsv    # or TSV
```

### Shell completions

```bash
hop completions fish > ~/.config/fish/completions/hop.fish
hop completions zsh > ~/.zfunc/_hop
hop completions bash > /usr/local/etc/bash_completion.d/hop
```

---

## Configuration

Create `~/Library/Application Support/hop/config.toml` (macOS) or `~/.config/hop/config.toml` (Linux) to customize:

```toml
# Directories scanned during `hop reindex`
index_roots = ["~/code", "~/work"]

# Skip these during indexing (not during normal h)
skip_dirs = ["node_modules", "target", ".venv", ".git"]

# How deep to walk subdirectories during reindex
max_depth = 6

# Reject fuzzy matches below this score (higher = stricter)
min_score = 20

# Prune stale entries on startup automatically (0 = disabled)
auto_prune_on_startup = 0
```

Then run `hop reindex` to build the filesystem index.

---

## All commands

| Command | Description |
|---|---|
| `hop [query]` / `h [query]` | Interactive picker; with query, fuzzy jumps directly |
| `hop add <path>` | Manually record a visit |
| `hop rm <path>` | Remove a path from history |
| `hop book [alias] [path]` | Set or resolve a bookmark |
| `hop book list` | List all bookmarks |
| `hop book rm <alias>` | Delete a bookmark |
| `hop history` | Top 20 by visit count |
| `hop recent` | Last 20 visited |
| `hop list [query]` | List all entries matching query |
| `hop score <query>` | Show score breakdown |
| `hop explain <query>` | Explain fuzzy match reasoning + scores |
| `hop top` | Alias for `hop history` |
| `hop prune [--dry-run] [--quiet]` | Remove stale entries; `--dry-run` previews, `--quiet` silences output |
| `hop clear [--force]` | Wipe all history and bookmarks |
| `hop stats [--verbose]` | Show database statistics |
| `hop import <type> <file>` | Import from zsh/fasd/autojump/zoxide/thefuck |
| `hop export [--format json\|csv\|tsv]` | Dump history and bookmarks |
| `hop update` | Check for and install a new release |
| `hop reindex` | Rebuild the filesystem index |
| `hop doctor` | Verify installation |
| `hop init <shell>` | Print shell integration snippet |
| `hop completions <shell>` | Print completions for the shell |

---

## Data storage

All data lives in a single SQLite database:

- **macOS**: `~/Library/Application Support/hop/hop.db`
- **Linux**: `~/.local/share/hop/hop.db`

The database has four tables: your visit history, bookmarks, an optional filesystem index, and schema metadata. That's it.

To back up or migrate: copy the `.db` file. Everything comes with you.

---

## Upgrading from `fuzzy-cd`

If you previously used the `fuzzy-cd` binary: hop automatically imports your old database from `~/Library/Application Support/fuzzy-cd/` on first run. Your history and bookmarks are preserved.

Update your shell config from `fuzzy-cd init ...` to `hop init <shell> | source` (or `eval`), then restart the shell.

---

## Troubleshooting

**"It doesn't find the folder I want."**
`h` into it the normal way a few times. Hop learns from visits. Or bookmark it directly: `hop book myproj ~/code/myproject`.

**"Wrong folder keeps winning."**
Remove it from history:
```bash
hop rm /path/to/wrong/folder
```

**"Is it working?"**
```bash
hop doctor
```
Should print `✓` for each check.

**"I broke it."**
No problem. Reset:
```bash
hop clear --force
```
Or delete everything and start fresh:
```bash
rm -rf ~/Library/Application\ Support/hop
```

---

## License

MIT

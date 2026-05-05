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

## Install

### One-liner (recommended)

```bash
curl -fsSL https://codeberg.org/dioxus/hop/raw/branch/main/install.sh | bash
```

Downloads the latest binary for your platform, places it in `~/.local/bin`, and prints instructions if you need to add that to your `PATH`.

### From source (requires Rust 1.75+)

```bash
git clone https://codeberg.org/dioxus/hop
cd hop
cargo install --path .
```

---

## Shell setup

Restart your shell after installing, then run the init command for your shell:

**Bash** (`~/.bashrc`):
```bash
eval "$(hop init bash)"
```

**Zsh** (`~/.zshrc`):
```zsh
eval "$(hop init zsh)"
```

**Fish** (`~/.config/fish/config.fish`):
```fish
hop init fish | source
```

**Nushell** (`$env.config.env_conversations` or `env.nu`):
```nu
hop init nushell | save ~/.config/hop/hop.nu
use ~/.config/hop/hop.nu
```

**Elvish** (`~/.config/elvish/rc.elv`):
```elvish
hop init elvish > ~/.config/hop/hop.elv
use (
  e:path-to-fs
  ~/.config/hop/hop.elv
)
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
h                # same as above (shorthand)
```

### Bookmarks

```bash
hop book work ~/code/work    # create a bookmark
hop book dot ~/.config       # name a frequently-used directory
h work                      # bookmark short-circuits fuzzy matching

hop book list               # see all bookmarks
hop book list --json        # machine-readable output
hop book edit work --alias proj           # rename a bookmark
hop book edit proj --path ~/code/project  # point it elsewhere
hop book edit proj --description "Main codebase"  # add a note
hop book edit proj          # with no flags, prints current values
hop book rm work            # delete a bookmark
```

### Add and remove

```bash
hop add ~/code/project      # manually record a visit
hop rm ~/old/project        # remove from history
hop forget ~/old/project    # alias for rm
hop clear                   # wipe all history (asks for confirm)
hop clear --force           # wipe without asking
```

### History and search

```bash
hop recent                  # last 20 visited directories
hop history                 # top 20 by visit count
hop list proj               # list all entries matching "proj"
hop score proj              # show why a query scored the way it did
hop explain proj            # fuzzy breakdown + individual scores
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
hop reindex                 # rebuild the filesystem index from index_roots
hop reindex --dry-run       # preview what would be indexed
hop doctor                  # sanity-check your install
hop update                  # check for a new release and upgrade
```

### Export

```bash
hop export                  # dump everything as JSON
hop export --format csv    # or CSV (alias, path, description)
hop export --format tsv    # or TSV
```

### Shell completions

```bash
hop completions fish > ~/.config/fish/completions/hop.fish
hop completions zsh > ~/.zfunc/_hop
hop completions bash > /usr/local/etc/bash_completion.d/hop
hop completions nushell > ~/.config/nushell/completions/hop.nu
hop completions elvish > ~/.config/elvish/completions/hop.elv
```

---

## Configuration

Create `~/.config/hop/config.toml` to customize:

```toml
# Directories scanned during `hop reindex`
index_roots = ["~/code", "~/work"]

# Skip these during reindex (not during normal h)
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
| `hop book list [--json]` | List all bookmarks |
| `hop book edit <alias> [--alias] [--path] [--description]` | Edit bookmark metadata |
| `hop book rm <alias>` | Delete a bookmark |
| `hop history` | Top 20 by visit count |
| `hop recent` | Last 20 visited |
| `hop list [query]` | List all entries matching query |
| `hop score <query>` | Show score breakdown |
| `hop explain <query>` | Explain fuzzy match reasoning + scores |
| `hop top` | Alias for `hop history` |
| `hop prune [--dry-run] [--quiet]` | Remove stale entries |
| `hop reindex [--dry-run]` | Rebuild the filesystem index |
| `hop clear [--force]` | Wipe all history and bookmarks |
| `hop stats [--verbose]` | Show database statistics |
| `hop import <type> <file>` | Import from zsh/fasd/autojump/zoxide/thefuck |
| `hop export [--format json\|csv\|tsv]` | Dump history and bookmarks |
| `hop update` | Check for and install a new release |
| `hop doctor` | Verify installation |
| `hop init <shell>` | Print shell integration snippet |
| `hop completions <shell>` | Print completions for the shell |

---

## Data storage

All data lives in a single SQLite database:

- **macOS**: `~/Library/Application Support/hop/hop.db`
- **Linux**: `~/.local/share/hop/hop.db`

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
```bash
hop rm /path/to/wrong/folder
```

**"Is it working?"**
```bash
hop doctor
```

**"I broke it."**
```bash
hop clear --force
```
Or delete everything and start fresh:
```bash
rm -rf ~/Library/Application\ Support/hop   # macOS
rm -rf ~/.local/share/hop                    # Linux
```

---

## Changelog

### v1.3.0 — New shells, smarter bookmarks

**New features:**
- **`hop book edit`** — rename a bookmark (`--alias`), change its path (`--path`), or update its description (`--description`). Running with no flags prints the current values.
- **`hop book list --json`** — machine-readable bookmark output.
- **Nushell integration** — `hop init nushell` emits a Nushell module with a `def --env h` command.
- **Elvish integration** — `hop init elvish` emits an Elvish script with an `fn h` and auto-recording prompt hook.
- **Shell completions** for Nushell and Elvish via `hop completions nushell` / `hop completions elvish`.

**Bug fixes:**
- `hop book rm` and `hop rm` now return exit code 1 when no matching entry is found.
- `hop reindex` now reports errors instead of silently succeeding; added `--dry-run` flag.
- Fish completions fixed: `hop list --limit` now correctly uses `-r` for require-a-value.
- `hop book rm` now gets proper alias completions.

**Performance:**
- `prune_auto` now uses O(1) memory instead of loading all entries at once, and batch-deletes in groups of 100.
- `batch_upsert_indexed_dirs` now wraps inserts in a single `BEGIN IMMEDIATE` / `COMMIT` transaction.
- `counts()` reduced from 5 separate SQL queries to a single query.
- Parallelized `prune` stat calls using rayon.
- Data directory permissions locked to `0o700`.
- Picker query cache — avoids redundant DB + scoring work on repeated keystrokes.
- Regex queries no longer spawn a thread per row.

---

### v1.2.x

See the git history for earlier releases.

---

## License

MIT

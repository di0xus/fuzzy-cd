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

---

## Install

### One-liner (recommended)

```bash
curl -fsSL https://codeberg.org/dioxus/hop/raw/branch/main/install.sh | bash
```

Grabs the latest binary for your platform, drops it in `~/.local/bin`, and tells you if you need to add that to your `PATH`.

### From source

Requires Rust 1.75+.

```bash
git clone https://codeberg.org/dioxus/hop
cd hop
cargo install --path .
```

---

## Shell setup

Restart your shell after installing. Then add the init line for your shell:

| Shell    | Config file                | Add this to your config file                   |
|----------|----------------------------|-------------------------------------------------|
| Bash     | `~/.bashrc`                | `eval "$(hop init bash)"`                      |
| Zsh      | `~/.zshrc`                 | `eval "$(hop init zsh)"`                        |
| Fish     | `~/.config/fish/config.fish` | `hop init fish \| source`                     |
| Nushell  | `~/.config/nushell/config.nu` | `hop init nushell \| save ~/.local/bin/hop.nu` then `use ~/.local/bin/hop.nu` in config.nu |
| Elvish   | `~/.config/elvish/rc.elv`  | `hop init elvish > ~/.config/hop/hop.elv` then `use ~/.config/hop/hop.elv` in rc.elv |

That's it. Every directory you visit is recorded automatically. `h` is now an alias for `hop`.

---

## Usage

### Navigate

```
h proj          → fuzzy match against your visit history
hop /tmp        → literal path works too
h ..            → relative paths work
h -             → previous directory
hop             → open the interactive picker
hop pick        → same as above (shortcut: hop p)
h               → same as above
```

### Bookmarks

```
hop book work ~/code/work      → create a bookmark
hop book dot ~/.config         → name a frequently-used directory
h work                         → bookmark short-circuits fuzzy matching

hop book list                  → see all bookmarks
hop book list --json           → machine-readable output

hop book edit work --alias proj        → rename
hop book edit proj --path ~/code/proj  → change the path
hop book edit proj --description "Main codebase" → add a note
hop book edit proj               → with no flags, prints current values

hop book rm work                → delete a bookmark
```

### Add & remove

```
hop add ~/code/project          → manually record a visit
hop rm ~/old/project            → remove exact path from history
hop forget proj                 → fuzzy-find and remove (no exact path needed)
hop clear                       → wipe all history (asks for confirmation)
hop clear --force               → wipe without asking
```

### History

```
hop recent                      → last 20 visited directories
hop history                     → top 20 by visit count
hop top                         → top 10 (alias for history)
hop list proj                   → list all entries matching "proj"
hop score proj                  → show why a query scored the way it did
hop explain proj                → fuzzy breakdown + individual component scores
```

### Import from other tools

Seed hop with your existing data on first install:

```bash
hop import zsh ~/.zsh_history
hop import fasd ~/.fasd
hop import autojump ~/.local/share/autojump/autojump.txt
hop import zoxide ~/.local/share/zoxide/data.zzd
```

### Housekeeping

```
hop prune               → remove entries pointing to deleted directories
hop prune --dry-run     → preview what would be removed
hop prune --quiet       → suppress output on success
hop reindex             → rebuild the filesystem index
hop reindex --dry-run    → preview what would be indexed
hop doctor              → sanity-check your install
hop update              → check for a new release and upgrade
hop update --dry-run    → check without installing
```

### Export

```
hop export                  → dump everything as JSON
hop export --format csv     → or CSV (alias, path, description)
hop export --format tsv     → or TSV
```

### Shell completions

```bash
hop completions fish      > ~/.config/fish/completions/hop.fish
hop completions zsh        > ~/.zfunc/_hop
hop completions bash      > /usr/local/etc/bash_completion.d/hop
hop completions nushell    > ~/.config/nushell/completions/hop.nu
hop completions elvish    > ~/.config/elvish/completions/hop.elv
```

---

## Configuration

Create `~/.config/hop/config.toml`:

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

Run `hop reindex` after changing `index_roots`, `skip_dirs`, or `max_depth`.

---

## All commands

| Command                              | Description                                        |
|--------------------------------------|----------------------------------------------------|
| `hop [query]` / `h [query]`          | Jump to best match; empty opens picker             |
| `hop add <path>`                     | Manually record a visit                            |
| `hop rm <path>`                      | Remove exact path from history                      |
| `hop forget <query>`                  | Fuzzy-find and remove from history                 |
| `hop book [alias] [path]`             | Set or resolve a bookmark                          |
| `hop book list [--json]`             | List all bookmarks                                 |
| `hop book edit <alias> [--alias] [--path] [--description]` | Edit bookmark metadata            |
| `hop book rm <alias>`                | Delete a bookmark                                  |
| `hop history` / `hop top`            | Top 20 (top 10) by visit count                     |
| `hop recent`                          | Last 20 visited directories                        |
| `hop list [query]`                    | List all entries matching query                   |
| `hop score <query>`                   | Show score breakdown                               |
| `hop explain <query>`                | Fuzzy reasoning + per-component scores             |
| `hop prune [--dry-run] [--quiet]`    | Remove stale entries                               |
| `hop reindex [--dry-run]`            | Rebuild filesystem index                           |
| `hop clear [--force]`                | Wipe all history and bookmarks                     |
| `hop stats [--verbose]`              | Show database statistics                           |
| `hop import <type> <file>`           | Import from zsh/fasd/autojump/zoxide               |
| `hop export [--format json\|csv\|tsv]` | Dump history and bookmarks                       |
| `hop update [--dry-run]`             | Self-update to latest release                      |
| `hop doctor`                          | Verify installation                                |
| `hop init <shell>`                   | Print shell integration snippet                    |
| `hop completions <shell>`            | Print tab-completion script                        |

---

## Data storage

All data lives in a single SQLite database:

- **macOS**: `~/Library/Application Support/hop/hop.db`
- **Linux**: `~/.local/share/hop/hop.db`

To back up or migrate: copy the `.db` file. Everything comes with you.

---

## Upgrading from `fuzzy-cd`

If you previously used the `fuzzy-cd` binary: hop automatically imports your old database from `~/Library/Application Support/fuzzy-cd/` on first run. Your history and bookmarks are preserved.

Update your shell config from `fuzzy-cd init ...` to `hop init <shell>` (see shell setup above), then restart the shell.

---

## Troubleshooting

**"It doesn't find the folder I want."**
`cd` into it the normal way a few times. Hop learns from visits. Or bookmark it directly:
```
hop book myproj ~/code/myproject
```

**"Wrong folder keeps winning."**
```
hop rm /path/to/wrong/folder
```

**"Is it working?"**
```
hop doctor
```

**"I broke it."**
```
hop clear --force
```

Or nuke everything and start fresh:
```bash
rm -rf ~/Library/Application\ Support/hop   # macOS
rm -rf ~/.local/share/hop                    # Linux
```

---

## Changelog

### v1.3.1

- Install script fix: use `python3` to parse JSON instead of `grep` (handles single-line JSON from Codeberg)
- Install script: improved error message when Codeberg is unreachable
- CI: added `.forgejo/workflows/release.yml` for Codeberg-native CI/CD

### v1.3.0 — New shells, smarter bookmarks

**New features:**
- `hop book edit` — rename a bookmark (`--alias`), change its path (`--path`), or update its description (`--description`). Running with no flags prints current values.
- `hop book list --json` — machine-readable bookmark output.
- Nushell integration via `hop init nushell`
- Elvish integration via `hop init elvish`
- Shell completions for Nushell and Elvish

**Bug fixes:**
- `hop rm` and `hop book rm` now return exit code 1 when nothing matched
- `hop reindex` now reports errors instead of silently succeeding
- Fish completions: `hop list --limit` now correctly uses `-r` for require-a-value
- `hop book rm` now gets proper alias completions

**Performance:**
- `prune_auto` now uses O(1) memory instead of loading all entries at once
- `batch_upsert_indexed_dirs` now wraps inserts in a single transaction
- `counts()` reduced from 5 SQL queries to 1
- Parallelized `prune` stale checks using rayon
- Picker query cache — avoids redundant work on repeated keystrokes
- Regex queries no longer spawn a thread per row

### Earlier releases

See the git history for details.

---

## License

MIT

# hop

A smarter `cd` for your terminal. Type a scrap of a directory name and it
jumps you to the right place.

```
~ $ cd hop
~/hop $

~ $ cd dl
~/Downloads $

~/Downloads $ cd work
~/code/work $
```

No more `cd ../../../some/long/path`. It learns where you go and remembers.

---

## Why you want this

- **Fewer keystrokes.** `cd proj` instead of `cd ~/code/work/projects/the-one`.
- **It remembers.** The more you visit a folder, the faster it finds it.
- **It still works like `cd`.** Typing a real path (`cd ~/Downloads`) behaves
  exactly like before. No surprises.
- **Fast.** Local SQLite lookup, runs in under 10 ms.
- **Yours.** Data lives on your machine, nothing goes anywhere.

---

## Install

You need Rust installed (`brew install rust` or
[rustup.rs](https://rustup.rs/)).

```bash
git clone <this repo>
cd hop
cargo install --path .
```

That puts a `hop` binary in `~/.cargo/bin`.

### Hook it into your shell

Pick your shell and add one line.

**Fish** — edit `~/.config/fish/config.fish`:

```fish
hop init fish | source
```

**Zsh** — edit `~/.zshrc`:

```zsh
eval "$(hop init zsh)"
```

**Bash** — edit `~/.bashrc`:

```bash
eval "$(hop init bash)"
```

Restart your shell (`exec fish`, `exec zsh`, etc.). That's it. `cd` is now
smart.

---

## How it works (the short version)

1. Every time you `cd` somewhere, the shell quietly tells `hop`:
   *"we just went to `/Users/you/code/work`."*
2. `hop` writes it down, counts how often you visit, notes when you were
   last there.
3. Next time you type `cd work`, it looks through everywhere you've been,
   scores each candidate, and picks the winner.

You don't have to teach it anything. Use `cd` normally for a few days and
it'll know your workflow.

---

## Everyday use

```bash
cd proj          # jump to best match for "proj"
cd /tmp          # real paths still work
cd ..            # so do the classics
cd -             # previous directory
hop              # open an interactive picker (arrow keys + enter)
```

### Bookmarks for the places you live in

Got a folder you visit ten times a day? Give it a short alias.

```bash
hop book work ~/code/work
hop book dot  ~/.config

cd work          # takes you straight there, every time
cd dot
```

List or remove them:

```bash
hop book list
hop book rm work
```

### Seed it from your old history

If you were already using zsh or
[fasd](https://github.com/clvv/fasd), import that history so `hop` starts
smart on day one:

```bash
hop import zsh  ~/.zsh_history
hop import fasd ~/.fasd
```

### Peek inside

```bash
hop stats        # summary
hop recent       # last 20 places you've been
hop history      # top 20 by visit count
hop doctor       # sanity check the setup
```

### Housekeeping

```bash
hop prune        # forget directories that no longer exist
hop rm /path     # forget one specific path
hop clear        # wipe the slate clean
```

---

## The interactive picker

Running `hop` with no arguments opens a mini picker:

```
› proj
 ★ /Users/you/code/work/projects
   /Users/you/side-projects/old
   /Users/you/projects-archive
```

- Type to filter live.
- `↑` `↓` to move, `Enter` to pick.
- `Esc` or `Ctrl-C` to back out.
- `Ctrl-U` to clear, `Backspace` to edit.

The `★` marks bookmarks.

---

## Tuning (optional)

Create `~/Library/Application Support/hop/config.toml` if you want to
customize things:

```toml
# Folders that get scanned during `hop reindex`
index_roots = ["~/code", "~/work"]

# Folders to skip during indexing
skip_dirs = ["node_modules", "target", ".venv"]

# How deep to walk
max_depth = 6

# Reject matches below this score (higher = stricter)
min_score = 20
```

Then run `hop reindex` to build a filesystem index, which kicks in as a
fallback when your history doesn't yet know a folder.

---

## Upgrading from `fuzzy-cd`

If you used the previous `fuzzy-cd` name: on first run, `hop` copies your
old database from `~/Library/Application Support/fuzzy-cd/` into the new
location automatically. Your history and bookmarks come with you.

Old shell integration still calls `fuzzy-cd`, so replace that block with
`hop init <shell> | source` (or `eval`) and restart the shell.

---

## Troubleshooting

**"It doesn't find the folder I want."**
`cd` into it the old-fashioned way a few times (full path). After a few
visits, fuzzy matching will pick it up. Or bookmark it.

**"Wrong folder keeps winning."**
Tell it to forget the wrong one:

```bash
hop rm /path/to/wrong/folder
```

**"Is it working at all?"**

```bash
hop doctor
```

Should show `✓` everywhere. If not, reload your shell and try again.

**"I broke it."**
Delete the database and start over — no harm done:

```bash
rm -rf ~/Library/Application\ Support/hop
```

---

## What it stores

One SQLite file at `~/Library/Application Support/hop/hop.db`. Four tables:
paths you've visited, bookmarks, an optional folder index, and schema
metadata. That's it.

Backup-safe: copy the `.db` to a new machine and your history comes with
you.

---

## FAQ

**Does it slow down my shell?**
No. The hook is a single `sqlite` insert that runs after `cd`. Takes a
couple of milliseconds.

**Will it mess with my existing `cd`?**
If you type a real directory path, it behaves exactly like `cd`. The
smarts only kick in when the argument isn't a real path.

**Does it follow symlinks, hidden folders, network mounts…?**
It records wherever `cd` successfully takes you — symlinks included. It
ignores nothing by default (the skip list is only for the optional
filesystem index).

**Can I uninstall?**
Yes. Remove the `hop init ...` line from your shell config, restart the
shell, and `cargo uninstall hop`. Delete
`~/Library/Application Support/hop` if you want the DB gone too.

---

## License

MIT

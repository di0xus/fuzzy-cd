# fuzzy-cd

A smarter `cd` for your terminal. Type a scrap of a directory name and it
jumps you to the right place.

```
~ $ cd fuzy
~/fuzzy-cd $

~/fuzzy-cd $ cd dl
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
cd fuzzy-cd
cargo install --path .
```

That puts a `fuzzy-cd` binary in `~/.cargo/bin`.

### Hook it into your shell

Pick your shell and add one line.

**Fish** — edit `~/.config/fish/config.fish`:

```fish
fuzzy-cd init fish | source
```

**Zsh** — edit `~/.zshrc`:

```zsh
eval "$(fuzzy-cd init zsh)"
```

**Bash** — edit `~/.bashrc`:

```bash
eval "$(fuzzy-cd init bash)"
```

Restart your shell (`exec fish`, `exec zsh`, etc.). That's it. `cd` is now
smart.

---

## How it works (the short version)

1. Every time you `cd` somewhere, the shell quietly tells `fuzzy-cd`:
   *"we just went to `/Users/you/code/work`."*
2. `fuzzy-cd` writes it down, counts how often you visit, notes when you
   were last there.
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
fuzzy-cd         # open an interactive picker (arrow keys + enter)
```

### Bookmarks for the places you live in

Got a folder you visit ten times a day? Give it a short alias.

```bash
fuzzy-cd book work ~/code/work
fuzzy-cd book dot  ~/.config

cd work          # takes you straight there, every time
cd dot
```

List or remove them:

```bash
fuzzy-cd book list
fuzzy-cd book rm work
```

### Seed it from your old history

If you were already using zsh or
[fasd](https://github.com/clvv/fasd), import that history so `fuzzy-cd`
starts smart on day one:

```bash
fuzzy-cd import zsh  ~/.zsh_history
fuzzy-cd import fasd ~/.fasd
```

### Peek inside

```bash
fuzzy-cd stats       # summary
fuzzy-cd recent      # last 20 places you've been
fuzzy-cd history     # top 20 by visit count
fuzzy-cd doctor      # sanity check the setup
```

### Housekeeping

```bash
fuzzy-cd prune       # forget directories that no longer exist
fuzzy-cd rm /path    # forget one specific path
fuzzy-cd clear       # wipe the slate clean
```

---

## The interactive picker

Running `fuzzy-cd` with no arguments opens a mini picker:

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

Create `~/Library/Application Support/fuzzy-cd/config.toml` if you want
to customize it:

```toml
# Folders that get scanned during `fuzzy-cd reindex`
index_roots = ["~/code", "~/work"]

# Folders to skip during indexing
skip_dirs = ["node_modules", "target", ".venv"]

# How deep to walk
max_depth = 6

# Reject matches below this score (higher = stricter)
min_score = 20
```

Then run `fuzzy-cd reindex` to build a filesystem index, which kicks in as
a fallback when your history doesn't yet know a folder.

---

## Troubleshooting

**"It doesn't find the folder I want."**
`cd` into it the old-fashioned way a few times (full path). After a few
visits, fuzzy matching will pick it up. Or bookmark it.

**"Wrong folder keeps winning."**
Tell it to forget the wrong one:

```bash
fuzzy-cd rm /path/to/wrong/folder
```

**"Is it working at all?"**

```bash
fuzzy-cd doctor
```

Should show `✓` everywhere. If not, reload your shell and try again.

**"I broke it."**
Delete the database and start over — no harm done:

```bash
rm -rf ~/Library/Application\ Support/fuzzy-cd
```

---

## What it stores

One SQLite file at
`~/Library/Application Support/fuzzy-cd/fuzzy-cd.db`. Four tables:
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
Yes. Remove the `fuzzy-cd init ...` line from your shell config, restart
the shell, and `cargo uninstall fuzzy-cd`. Delete
`~/Library/Application Support/fuzzy-cd` if you want the DB gone too.

---

## License

MIT

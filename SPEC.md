# hop — SPEC.md

## Concept

A fast `cd` replacement that fuzzy-matches directory history, bookmarks,
and an optional filesystem index. Sub-10 ms cold-lookup on a 10 k-row DB.

Formerly shipped as `fuzzy-cd`; renamed to `hop` in v0.4.

## Surfaces

| Command                         | Behavior                                 |
| ------------------------------- | ---------------------------------------- |
| `hop <query>`                   | Prints best match path; exit 1 if none. |
| `hop p|pick [query]`            | Same; empty query opens picker.         |
| `hop` (bare)                    | Interactive picker.                     |
| `hop add <path>`                | Record a visit.                         |
| `hop rm <path>`                 | Drop from history (exact path).       |
| `hop forget|zap <query>`        | Fuzzy-find and remove from history.  |
| `hop book <alias> [path]`       | Set or resolve bookmark.                |
| `hop book rm <alias>`           | Delete bookmark.                        |
| `hop book list`                 | List bookmarks.                         |
| `hop history [n]`               | Top n by visits (default 20).           |
| `hop recent [n]`                | Last n visited.                         |
| `hop top`                       | Top 10.                                 |
| `hop import fasd|zsh <file>`    | Import from another tool.               |
| `hop prune`                     | Remove stale paths.                     |
| `hop clear`                     | Wipe history.                           |
| `hop stats`                     | DB counts.                              |
| `hop reindex`                   | Rebuild filesystem index.               |
| `hop doctor`                    | Diagnose DB + shell hook.               |
| `hop init <bash|zsh|fish>`      | Emit shell integration.                 |

## Scoring

`score = fuzzy_match + visit_boost + recency + git_bonus + basename_bonus +
shortness_bonus`

- fuzzy: SkimV2 `fuzzy_indices` (smart-case).
- visit_boost: `min(5, sqrt(visits)) * 20`.
- recency: {today: 45, week: 30, month: 15, older: 7.5}.
- git_bonus: +30 if `.git` exists (cached per visit).
- basename_bonus: +40 when query substring hits folder name.
- shortness: `max(1, 10/depth) * 5`.
- bookmarks: `fuzzy × 3 + 100`; exact alias short-circuits.
- index fallback: `fuzzy/2 + shortness*5`, only consulted when best
  history/bookmark candidate < `2 * min_score`.
- `min_score` gate rejects weak matches → exit 1.

## Storage

SQLite at `$XDG_DATA_HOME/hop/hop.db` (or macOS
`~/Library/Application Support/hop/`), WAL mode, `synchronous=NORMAL`.
Schema versioned via `meta.schema_version`; `Database::migrate` runs on
open.

**Legacy migration**: on first run, if `hop.db` is missing but
`fuzzy-cd.db` exists at the old `ProjectDirs` location, it's copied (plus
any `-wal`/`-shm` siblings) into the new directory.

**Tables**
- `history(path UNIQUE, basename, visits, last_visited, created_at, is_git_repo)`
- `bookmarks(alias UNIQUE, path, created_at)`
- `dir_index(path UNIQUE, basename, parent, indexed_at)`
- `meta(key PRIMARY KEY, value)`

## Shell integration

`hop init <shell>` emits:

1. A `chpwd` hook → records every successful `cd` to history.
2. A `__hop_cd` function that short-circuits on `-`, `..`, `.`, `~`, `~/`,
   absolute paths, and existing directories before falling back to fuzzy.
3. `alias cd=__hop_cd`.

The shell function is deliberately named with a double underscore to avoid
colliding with the `hop` binary itself — users can still call
`hop book …`, `hop doctor` directly.

## Config

Optional `config.toml`, tiny purpose-built parser:

```toml
index_roots = ["~/code"]
skip_dirs   = ["node_modules"]
max_depth   = 6
min_score   = 20
```

## Non-goals

- Cloud sync.
- Per-repo shortcuts.
- Tmux/session awareness.
- Middle-of-command completion (needs full shell plugin).

## Modules

```
src/
├── main.rs        thin entrypoint
├── lib.rs         pub mod …
├── cli.rs         arg dispatch + find_best()
├── db.rs          Database + migrations + legacy copy
├── score.rs       Scorer, Scored
├── picker.rs      crossterm TUI
├── init.rs        shell init scripts
├── config.rs      Config loader
├── import.rs      fasd + zsh history importers
├── index.rs       filesystem indexer
└── doctor.rs      diagnostics
```

## Tests

- 33 unit tests covering scoring ordering, migrations (incl. legacy v0.2
  schema), import parsing, config parsing, indexing, doctor.
- 4 integration tests invoking the built binary via
  `CARGO_BIN_EXE_hop`.
- CI: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`,
  release build on ubuntu + macos.

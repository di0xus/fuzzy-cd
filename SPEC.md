# fuzzy-cd — SPEC.md

## Concept

A fast `cd` replacement that fuzzy-matches directory history, bookmarks, and
an optional filesystem index. Sub-10 ms cold-lookup on a 10 k-row DB.

## Surfaces

| Command                              | Behavior                                            |
| ------------------------------------ | --------------------------------------------------- |
| `fuzzy-cd <query>`                   | Prints best match path; exit 1 if no match.        |
| `fuzzy-cd p|pick [query]`            | Same; empty query opens picker.                    |
| `fuzzy-cd` (bare)                    | Interactive picker.                                |
| `fuzzy-cd add <path>`                | Record a visit.                                    |
| `fuzzy-cd rm <path>`                 | Drop from history.                                 |
| `fuzzy-cd book <alias> [path]`       | Set or resolve bookmark.                           |
| `fuzzy-cd book rm <alias>`           | Delete bookmark.                                   |
| `fuzzy-cd book list`                 | List bookmarks.                                    |
| `fuzzy-cd history [n]`               | Top n by visits (default 20).                      |
| `fuzzy-cd recent [n]`                | Last n visited.                                    |
| `fuzzy-cd top`                       | Top 10.                                            |
| `fuzzy-cd import fasd|zsh <file>`    | Import from another tool.                          |
| `fuzzy-cd prune`                     | Remove stale paths (deleted dirs).                |
| `fuzzy-cd clear`                     | Wipe history.                                      |
| `fuzzy-cd stats`                     | DB counts.                                         |
| `fuzzy-cd reindex`                   | Rebuild filesystem index.                          |
| `fuzzy-cd doctor`                    | Diagnose DB + shell hook + stale entries.         |
| `fuzzy-cd init <bash|zsh|fish>`      | Emit shell integration.                           |

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

SQLite at `$XDG_DATA_HOME/fuzzy-cd/fuzzy-cd.db` (or macOS
`~/Library/Application Support/fuzzy-cd/`), WAL mode, `synchronous=NORMAL`.
Schema versioned via `meta.schema_version`; `Database::migrate` runs on open.

**Tables**
- `history(path UNIQUE, basename, visits, last_visited, created_at, is_git_repo)`
- `bookmarks(alias UNIQUE, path, created_at)`
- `dir_index(path UNIQUE, basename, parent, indexed_at)`
- `meta(key PRIMARY KEY, value)`

## Shell integration

`fuzzy-cd init <shell>` emits:

1. A `chpwd` hook → records every successful `cd` to history.
2. A `fcd` function that short-circuits on `-`, `..`, `.`, `~`, `~/`,
   absolute paths, and existing directories before falling back to fuzzy.
3. `alias cd=fcd`.

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
├── db.rs          Database + migrations
├── score.rs       Scorer, Scored
├── picker.rs      crossterm TUI
├── init.rs        shell init scripts
├── config.rs      Config loader
├── import.rs      fasd + zsh history importers
├── index.rs       filesystem indexer
└── doctor.rs      diagnostics
```

## Tests

- 28 unit tests covering scoring ordering, migrations, import parsing,
  config parsing, indexing, doctor.
- 4 integration tests invoking the built binary via `CARGO_BIN_EXE_*`.
- CI: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, release
  build on ubuntu + macos.

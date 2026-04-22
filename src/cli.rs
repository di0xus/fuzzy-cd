use std::path::Path;
use std::process::ExitCode;

use crate::completions;
use crate::config::Config;
use crate::db::{canonicalize_path, expand_home, now_secs, Database, HistoryRow};
use crate::index;
use crate::init;
use crate::picker;
use crate::score::{ScoreBreakdown, Scored, Scorer};
use crate::{doctor, import};
use serde_json;

pub const HELP: &str = r#"hop — smart directory jump

Usage:
    hop <query>                  Jump to best match (prints path)
    hop p|pick [query]           Same; empty query opens picker
    hop add <path>               Record a visit
    hop rm <path>                Remove from history (exact path)
    hop forget|zap <query>       Fuzzy-find and remove from history
    hop book <alias> [path]      Set/resolve bookmark
    hop book rm <alias>          Delete bookmark
    hop book list                List bookmarks
    hop history [n]              Top n by visits (default 20)
    hop recent [n]               Last n visited (default 20)
    hop top                      Top 10.
    hop score <query>            Show per-component score breakdown
    hop score <query> --json     Same, JSON output
    hop list <query> [--limit N] [--json]  List all scored matches
    hop export [--format json|csv|tsv]  Dump history/bookmarks
    hop import fasd|autojump|zoxide|thefuck <file>  Import from another tool
    hop prune [--dry-run]         Remove stale (deleted) paths
    hop clear [--force]           Wipe history (prompts by default)
    hop stats                    DB stats.
    hop reindex                  Rebuild filesystem index
    hop doctor                   Diagnose setup
    hop update [--dry-run]       Self-update to latest release
    hop init <bash|zsh|fish>     Emit shell integration
    hop init --shell <shell>     Same, with explicit flag
    hop init --verify            Check shell integration
    hop completions <bash|zsh|fish>  Emit tab-completion script
    hop --help                   This help
"#;

pub fn run(args: Vec<String>) -> ExitCode {
    // Fast-path: `init` and `completions` need no DB.
    if args.len() >= 2 && args[1] == "init" {
        return cmd_init(&args);
    }
    if args.len() >= 2 && args[1] == "completions" {
        return cmd_completions(&args);
    }
    if matches!(
        args.get(1).map(String::as_str),
        Some("--help" | "-h" | "help")
    ) {
        print!("{}", HELP);
        return ExitCode::SUCCESS;
    }
    if matches!(args.get(1).map(String::as_str), Some("--version" | "-v")) {
        println!("hop {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.len() == 1 {
        // bare invocation → picker
        return run_picker_and_print("");
    }

    let db = match Database::open() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("db open failed: {}", e);
            return ExitCode::from(2);
        }
    };
    let (cfg, _) = Config::load_with_warnings();

    match args[1].as_str() {
        "p" | "pick" => {
            // Strip `--` separator if present
            let rest: Vec<&str> = args[2..]
                .iter()
                .map(String::as_str)
                .filter(|s| *s != "--")
                .collect();
            let query = rest.join(" ");
            if query.is_empty() {
                return run_picker_and_print("");
            }
            match find_best(&db, &cfg, &query) {
                Some(path) => {
                    println!("{}", path);
                    let _ = db.record_visit(&path);
                    ExitCode::SUCCESS
                }
                None => ExitCode::from(1),
            }
        }
        "add" => {
            let Some(arg) = positional(&args, 2) else {
                eprintln!("Usage: hop add <path>");
                return ExitCode::from(2);
            };
            let path = expand_home(arg);
            if !path.is_dir() {
                eprintln!("not a directory: {}", path.display());
                return ExitCode::from(1);
            }
            let _ = db.record_visit(&path.to_string_lossy());
            ExitCode::SUCCESS
        }
        "rm" => {
            let Some(arg) = positional(&args, 2) else {
                eprintln!("Usage: hop rm <path>");
                return ExitCode::from(2);
            };
            let path = expand_home(arg);
            let removed = db.forget(&path.to_string_lossy()).unwrap_or(0);
            if removed > 0 {
                println!("removed: {}", path.display());
            } else {
                println!("not found in history: {}", path.display());
            }
            ExitCode::SUCCESS
        }
        "forget" | "zap" => {
            let query = args[2..].join(" ");
            if query.is_empty() {
                eprintln!("Usage: hop forget <query>");
                return ExitCode::from(2);
            }
            match find_best(&db, &cfg, &query) {
                Some(path) => {
                    let removed = db.forget(&path).unwrap_or(0);
                    if removed > 0 {
                        println!("forgot: {}", path);
                    } else {
                        println!("not found in history: {}", path);
                    }
                    ExitCode::SUCCESS
                }
                None => {
                    eprintln!("no match for: {}", query);
                    ExitCode::from(1)
                }
            }
        }
        "book" | "bookmark" => cmd_bookmark(&db, &args[2..]),
        "history" => {
            let n = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);
            let rows = db.top(n).unwrap_or_default();
            print_rows(&Database::filter_live_rows(rows));
            ExitCode::SUCCESS
        }
        "score" => {
            let query = args[2..]
                .iter()
                .map(String::as_str)
                .filter(|s| *s != "--")
                .collect::<Vec<_>>()
                .join(" ");
            let is_json = args.iter().any(|a| a == "--json");
            if query.is_empty() {
                eprintln!("Usage: hop score <query> [--json]");
                return ExitCode::from(2);
            }
            cmd_score(&db, &cfg, &query, is_json)
        }
        "list" => {
            let query = args[2..]
                .iter()
                .map(String::as_str)
                .filter(|s| *s != "--")
                .collect::<Vec<_>>()
                .join(" ");
            let is_json = args.iter().any(|a| a == "--json");
            let limit = args
                .iter()
                .position(|a| a == "--limit")
                .and_then(|i| args.get(i + 1)?.parse().ok())
                .unwrap_or(20);
            cmd_list(&db, &cfg, &query, limit, is_json)
        }
        "export" => {
            let format = args
                .iter()
                .position(|a| a == "--format")
                .and_then(|i| args.get(i + 1).cloned())
                .unwrap_or_else(|| "json".to_string());
            cmd_export(&db, &format)
        }
        "update" => {
            let dry_run = args.get(2).map(String::as_str) == Some("--dry-run");
            cmd_update(dry_run)
        }
        "top" => {
            let rows = db.top(10).unwrap_or_default();
            print_rows(&Database::filter_live_rows(rows));
            ExitCode::SUCCESS
        }
        "recent" => {
            let n = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);
            let rows = db.recent(n).unwrap_or_default();
            print_rows(&Database::filter_live_rows(rows));
            ExitCode::SUCCESS
        }
        "import" => {
            if args.len() < 4 {
                eprintln!("Usage: hop import <fasd|autojump|zoxide|thefuck> <file>");
                return ExitCode::from(2);
            }
            let source = args[2].as_str();
            let file = Path::new(&args[3]);
            let result = match source {
                "fasd" => import::import_fasd(&db, file),
                "autojump" => import::import_autojump(&db, file),
                "zoxide" => import::import_zoxide(&db, file),
                "thefuck" => import::import_thefuck(&db, file),
                "zsh" => import::import_zsh(&db, file),
                _ => {
                    eprintln!("unknown source: {}", source);
                    return ExitCode::from(2);
                }
            };
            match result {
                Ok(stats) => {
                    println!("imported {}, skipped {}", stats.imported, stats.skipped);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("import failed: {}", e);
                    ExitCode::from(1)
                }
            }
        }
        "prune" => {
            let dry_run = args.get(2).map(String::as_str) == Some("--dry-run");
            let quiet = args.get(2).map(String::as_str) == Some("--quiet")
                || args.get(3).map(String::as_str) == Some("--quiet");
            if dry_run {
                match db.prune_stale_dry_run() {
                    Ok((history_stale, index_stale)) => {
                        let total = history_stale.len() + index_stale.len();
                        if total == 0 {
                            println!("nothing to prune");
                        } else {
                            println!("history ({}):", history_stale.len());
                            for p in &history_stale {
                                println!("  - {}", p);
                            }
                            println!("index ({}):", index_stale.len());
                            for p in &index_stale {
                                println!("  - {}", p);
                            }
                            println!(
                                "\n{} stale entr{} total. Run without --dry-run to remove.",
                                total,
                                if total == 1 { "y" } else { "ies" }
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("prune dry-run failed: {}", e);
                        return ExitCode::from(1);
                    }
                }
            } else {
                let total_paths = db.history_rows().map(|r| r.len()).unwrap_or(0);
                let total_index = db.index_rows().map(|r| r.len()).unwrap_or(0);
                let grand_total = total_paths + total_index;

                if !quiet && grand_total > 0 {
                    eprintln!("pruning {} entries...", grand_total);
                }

                match db.prune_stale_batch(256, |done, total| {
                    if !quiet && total > 0 {
                        eprintln!("  {} / {}", done, total);
                    }
                }) {
                    Ok(removed) => {
                        if !quiet {
                            println!(
                                "pruned {} stale entr{}",
                                removed,
                                if removed == 1 { "y" } else { "ies" }
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("prune failed: {}", e);
                        return ExitCode::from(1);
                    }
                }
            }
            ExitCode::SUCCESS
        }
        "clear" => {
            let force = args.get(2).map(String::as_str) == Some("--force");
            if !force {
                eprint!(
                    "this will wipe ALL history and the directory index. type 'yes' to confirm: "
                );
                let mut input = String::new();
                if std::io::stdin().read_line(&mut input).is_err() || input.trim() != "yes" {
                    println!("aborted");
                    return ExitCode::from(1);
                }
            }
            match db.clear_history() {
                Ok(()) => {
                    println!("history cleared");
                }
                Err(e) => {
                    eprintln!("clear failed: {}", e);
                    return ExitCode::from(1);
                }
            }
            ExitCode::SUCCESS
        }
        "stats" => match db.counts() {
            Ok(c) => {
                println!(
                    "paths: {}\nvisits: {}\nbookmarks: {}\nindexed dirs: {}\ntop: {}",
                    c.total,
                    c.total_visits,
                    c.bookmarks,
                    c.indexed,
                    c.top_path.unwrap_or_else(|| "(none)".into())
                );
                // Auto-suggest prune if > 20% of history is stale
                if c.total > 0 {
                    let stale = db
                        .history_rows()
                        .map(|rows| rows.iter().filter(|r| !Path::new(&r.path).is_dir()).count())
                        .unwrap_or(0);
                    let pct = (stale as f64 / c.total as f64) * 100.0;
                    if pct > 20.0 {
                        println!(
                            "\n⚠ {:.0}% stale ({} of {}) — run `hop prune` to clean up",
                            pct, stale, c.total
                        );
                    }
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("stats failed: {}", e);
                ExitCode::from(1)
            }
        },
        "reindex" | "--reindex" | "-r" => {
            let stats = index::reindex(&db, &cfg);
            println!(
                "indexed {} dirs ({} scanned)",
                stats.inserted, stats.scanned
            );
            ExitCode::SUCCESS
        }
        "doctor" => {
            let r = doctor::run(&db);
            for line in &r.lines {
                println!("{}", line);
            }
            if r.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        _ => {
            // treat unrecognized first arg as a query
            let query = args[1..].join(" ");
            match find_best(&db, &cfg, &query) {
                Some(path) => {
                    println!("{}", path);
                    let _ = db.record_visit(&path);
                    ExitCode::SUCCESS
                }
                None => ExitCode::from(1),
            }
        }
    }
}

fn positional(args: &[String], idx: usize) -> Option<&str> {
    let a = args.get(idx)?;
    if a == "--" {
        args.get(idx + 1).map(String::as_str)
    } else {
        Some(a.as_str())
    }
}

fn cmd_init(args: &[String]) -> ExitCode {
    // Flags: --verify, --shell <name>. Positional shell name still works.
    let rest: Vec<&str> = args[2..].iter().map(String::as_str).collect();
    let mut shell: Option<&str> = None;
    let mut verify = false;
    let mut i = 0;
    while i < rest.len() {
        match rest[i] {
            "--verify" => verify = true,
            "--shell" => {
                i += 1;
                if i >= rest.len() {
                    eprintln!("--shell requires an argument");
                    return ExitCode::from(2);
                }
                shell = Some(rest[i]);
            }
            s if !s.starts_with('-') => shell = Some(s),
            s => {
                eprintln!("unknown init flag: {}", s);
                return ExitCode::from(2);
            }
        }
        i += 1;
    }

    if verify {
        let r = init::verify();
        for line in &r.lines {
            println!("{}", line);
        }
        return if r.ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        };
    }

    let chosen: Option<String> = shell
        .map(str::to_owned)
        .or_else(|| init::detect_shell().map(str::to_owned));
    match chosen.as_deref().and_then(init::script_for) {
        Some(s) => {
            print!("{}", s);
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("Usage: hop init <bash|zsh|fish> | --shell <name> | --verify");
            ExitCode::from(2)
        }
    }
}

fn cmd_completions(args: &[String]) -> ExitCode {
    let rest: Vec<&str> = args[2..].iter().map(String::as_str).collect();
    let mut shell: Option<&str> = None;
    let mut i = 0;
    while i < rest.len() {
        match rest[i] {
            "--shell" => {
                i += 1;
                if i >= rest.len() {
                    eprintln!("--shell requires an argument");
                    return ExitCode::from(2);
                }
                shell = Some(rest[i]);
            }
            s if !s.starts_with('-') => shell = Some(s),
            s => {
                eprintln!("unknown completions flag: {}", s);
                return ExitCode::from(2);
            }
        }
        i += 1;
    }
    let chosen: Option<String> = shell
        .map(str::to_owned)
        .or_else(|| init::detect_shell().map(str::to_owned));
    match chosen.as_deref().and_then(completions::script_for) {
        Some(s) => {
            print!("{}", s);
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("Usage: hop completions <bash|zsh|fish>");
            ExitCode::from(2)
        }
    }
}

fn cmd_bookmark(db: &Database, args: &[String]) -> ExitCode {
    if args.is_empty() || args[0] == "list" {
        match db.bookmarks() {
            Ok(bms) => {
                for (alias, path) in bms {
                    println!("{:20}  {}", alias, path);
                }
                ExitCode::SUCCESS
            }
            Err(_) => ExitCode::from(1),
        }
    } else if args[0] == "rm" {
        if args.len() < 2 {
            eprintln!("Usage: hop book rm <alias>");
            return ExitCode::from(2);
        }
        let _ = db.remove_bookmark(&args[1]);
        ExitCode::SUCCESS
    } else {
        let alias = &args[0];
        if args.len() > 1 {
            let path = expand_home(&args[1]);
            if !path.is_dir() {
                eprintln!("not a directory: {}", path.display());
                return ExitCode::from(1);
            }
            let _ = db.set_bookmark(alias, &path.to_string_lossy());
            ExitCode::SUCCESS
        } else {
            match db.bookmark_exact(alias) {
                Ok(Some(p)) => {
                    println!("{}", p);
                    ExitCode::SUCCESS
                }
                _ => ExitCode::from(1),
            }
        }
    }
}

fn run_picker_and_print(query: &str) -> ExitCode {
    let db = match Database::open() {
        Ok(d) => d,
        Err(_) => return ExitCode::from(2),
    };
    match picker::run(&db, query) {
        Ok(Some(path)) => {
            println!("{}", path);
            let _ = db.record_visit(&path);
            ExitCode::SUCCESS
        }
        _ => ExitCode::from(1),
    }
}

pub fn find_best(db: &Database, cfg: &Config, query: &str) -> Option<String> {
    // If the query resolves to an existing directory (possibly via symlink),
    // canonicalize it so we match the canonical path stored in history.
    if let Some(canonical) = canonicalize_path(query) {
        if Path::new(&canonical).is_dir() {
            return Some(canonical);
        }
    }

    // exact bookmark alias short-circuits
    if let Ok(Some(p)) = db.bookmark_exact(query) {
        if Path::new(&p).is_dir() {
            return Some(p);
        }
    }

    let scorer = Scorer::new(now_secs());
    let mut cands: Vec<Scored> = Vec::new();

    if let Ok(bms) = db.bookmarks() {
        for (alias, path) in bms {
            if let Some(s) = scorer.score_bookmark(&alias, &path, query) {
                if Path::new(&s.path).is_dir() {
                    cands.push(s);
                }
            }
        }
    }

    if let Ok(rows) = db.history_rows() {
        for r in rows {
            if let Some(s) = scorer.score_history(&r, query) {
                if Path::new(&s.path).is_dir() {
                    cands.push(s);
                }
            }
        }
    }

    // Fallback to filesystem index if nothing strong found
    let best_history = cands.iter().map(|c| c.score).max().unwrap_or(0);
    if best_history < cfg.min_score * 2 {
        if let Ok(paths) = db.index_rows() {
            for p in paths {
                if let Some(s) = scorer.score_indexed(&p, query) {
                    if Path::new(&s.path).is_dir() {
                        cands.push(s);
                    }
                }
            }
        }
    }

    cands.sort_by_key(|c| std::cmp::Reverse(c.score));
    cands.dedup_by(|a, b| a.path == b.path);
    cands
        .into_iter()
        .find(|c| c.score >= cfg.min_score)
        .map(|c| c.path)
}

#[allow(dead_code)]
fn row_display(r: &HistoryRow) -> String {
    format!("{:4} visits   {}", r.visits, r.path)
}

fn print_rows(rows: &[HistoryRow]) {
    for r in rows {
        println!("{:4} visits   {}", r.visits, r.path);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// v0.8: score, list, export, update
// ─────────────────────────────────────────────────────────────────────────────

fn cmd_score(db: &Database, cfg: &Config, query: &str, is_json: bool) -> ExitCode {
    let scorer = Scorer::new(now_secs());
    let mut breakdowns: Vec<ScoreBreakdown> = Vec::new();

    // exact bookmark first
    if let Ok(Some(p)) = db.bookmark_exact(query) {
        if Path::new(&p).is_dir() {
            if let Some(b) = scorer.score_bookmark_breakdown(query, &p, query) {
                breakdowns.push(b);
            }
        }
    }

    // score bookmarks
    if let Ok(bms) = db.bookmarks() {
        for (alias, path) in bms {
            if let Some(b) = scorer.score_bookmark_breakdown(&alias, &path, query) {
                if Path::new(&b.path).is_dir() && b.total > cfg.min_score {
                    breakdowns.push(b);
                }
            }
        }
    }

    // score history
    if let Ok(rows) = db.history_rows() {
        for r in rows {
            if let Some(b) = scorer.score_history_breakdown(&r, query) {
                if Path::new(&b.path).is_dir() && b.total >= cfg.min_score {
                    breakdowns.push(b);
                }
            }
        }
    }

    // fallback index only if best is weak
    let best_history = breakdowns.iter().map(|b| b.total).max().unwrap_or(0);
    if best_history < cfg.min_score * 2 {
        if let Ok(paths) = db.index_rows() {
            for p in paths {
                if let Some(b) = scorer.score_indexed_breakdown(&p, query) {
                    if Path::new(&b.path).is_dir() && b.total >= cfg.min_score {
                        breakdowns.push(b);
                    }
                }
            }
        }
    }

    breakdowns.sort_by_key(|b| std::cmp::Reverse(b.total));
    breakdowns.dedup_by(|a, b| a.path == b.path);

    if breakdowns.is_empty() {
        return ExitCode::from(1);
    }

    if is_json {
        // Print top 10 as JSON
        let tops: Vec<_> = breakdowns
            .iter()
            .take(10)
            .map(|b| {
                serde_json::json!({
                    "path": b.path,
                    "total": b.total,
                    "fuzzy": b.fuzzy,
                    "visits": b.visits,
                    "recency": b.recency,
                    "git": b.git,
                    "basename": b.basename,
                    "shortness": b.shortness,
                    "source": format!("{:?}", b.source).to_lowercase(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&tops).unwrap());
    } else {
        // Human-readable per-component breakdown
        println!("query: {}", query);
        println!();
        for (i, b) in breakdowns.iter().take(10).enumerate() {
            let trophy = if i == 0 { " (best)" } else { "" };
            println!(
                "{}{}  total={:>4}  fuzzy={:>3}  visits={:>3}  recency={:>2}  git={:>2}  basename={:>2}  shortness={:>2}  [{:?}]",
                b.path,
                trophy,
                b.total,
                b.fuzzy,
                b.visits,
                b.recency,
                b.git,
                b.basename,
                b.shortness,
                b.source,
            );
        }
    }
    ExitCode::SUCCESS
}

fn cmd_list(db: &Database, cfg: &Config, query: &str, limit: usize, is_json: bool) -> ExitCode {
    let scorer = Scorer::new(now_secs());
    let mut scored: Vec<Scored> = Vec::new();

    if let Ok(bms) = db.bookmarks() {
        for (alias, path) in bms {
            if let Some(s) = scorer.score_bookmark(&alias, &path, query) {
                if Path::new(&s.path).is_dir() {
                    scored.push(s);
                }
            }
        }
    }

    if let Ok(rows) = db.history_rows() {
        for r in rows {
            if let Some(s) = scorer.score_history(&r, query) {
                if Path::new(&s.path).is_dir() {
                    scored.push(s);
                }
            }
        }
    }

    let best_history = scored.iter().map(|c| c.score).max().unwrap_or(0);
    if best_history < cfg.min_score * 2 {
        if let Ok(paths) = db.index_rows() {
            for p in paths {
                if let Some(s) = scorer.score_indexed(&p, query) {
                    if Path::new(&s.path).is_dir() {
                        scored.push(s);
                    }
                }
            }
        }
    }

    scored.sort_by_key(|c| std::cmp::Reverse(c.score));
    scored.dedup_by(|a, b| a.path == b.path);
    scored.truncate(limit);

    if scored.is_empty() {
        return ExitCode::from(1);
    }

    if is_json {
        let items: Vec<_> = scored
            .iter()
            .map(|s| {
                serde_json::json!({
                    "path": s.path,
                    "score": s.score,
                    "source": format!("{:?}", s.source).to_lowercase(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items).unwrap());
    } else {
        for s in &scored {
            println!("{}\t{}\t{:?}", s.score, s.path, s.source);
        }
    }
    ExitCode::SUCCESS
}

fn cmd_export(db: &Database, format: &str) -> ExitCode {
    let history = db.history_rows().unwrap_or_default();
    let bookmarks = db.bookmarks().unwrap_or_default();

    match format {
        "json" => {
            let payload = serde_json::json!({
                "version": 1,
                "exported_at": now_secs(),
                "history": history.iter().map(|r| {
                    serde_json::json!({
                        "path": r.path,
                        "visits": r.visits,
                        "last_visited": r.last_visited,
                        "is_git_repo": r.is_git_repo,
                    })
                }).collect::<Vec<_>>(),
                "bookmarks": bookmarks.iter().map(|(alias, path)| {
                    serde_json::json!({
                        "alias": alias,
                        "path": path,
                    })
                }).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&payload).unwrap());
        }
        "csv" => {
            println!("type,alias_or_path,visits,last_visited,is_git_repo");
            for r in &history {
                println!(
                    "history,{},{},{},{}",
                    r.path, r.visits, r.last_visited, r.is_git_repo
                );
            }
            for (alias, path) in &bookmarks {
                println!("bookmark,{}:{},0,0,false", alias, path);
            }
        }
        "tsv" => {
            for r in &history {
                println!(
                    "history\t{}\t{}\t{}\t{}",
                    r.path, r.visits, r.last_visited, r.is_git_repo
                );
            }
            for (alias, path) in &bookmarks {
                println!("bookmark\t{}:{}\t0\t0\tfalse", alias, path);
            }
        }
        _ => {
            eprintln!("unknown format '{}': use json, csv, or tsv", format);
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}

fn cmd_update(dry_run: bool) -> ExitCode {
    // Fetch latest release info from GitHub API
    let url = "https://api.github.com/repos/di0xus/hop/releases/latest";
    let client = ureq::Agent::new();
    match client.get(url).call() {
        Ok(resp) => {
            if resp.status() != 200 {
                eprintln!("failed to fetch releases: HTTP {}", resp.status());
                return ExitCode::from(1);
            }
            let body = resp.into_string().unwrap();
            let body: serde_json::Value = serde_json::from_str(&body).unwrap();
            let tag = body["tag_name"].as_str().unwrap_or("unknown");
            let current = env!("CARGO_PKG_VERSION");
            if tag == current {
                println!("already at latest version: {}", current);
                return ExitCode::SUCCESS;
            }
            println!("latest: {}  current: {}", tag, current);
            if dry_run {
                println!("(dry-run) would download and install {}", tag);
            } else {
                println!("run without --dry-run to install");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("update check failed: {}", e);
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_best_respects_threshold() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("my-project");
        std::fs::create_dir(&real).unwrap();

        let db = Database::in_memory().unwrap();
        db.record_visit(&real.to_string_lossy()).unwrap();
        let cfg = Config::default();

        // record_visit now canonicalizes, so compare via canonical path
        let expected = canonicalize_path(real.to_str().unwrap()).unwrap();
        assert_eq!(
            find_best(&db, &cfg, "proj").as_deref(),
            Some(expected.as_str())
        );
        // total garbage query → no match
        assert!(find_best(&db, &cfg, "xxxyyyzzz").is_none());
    }

    #[test]
    fn find_best_filters_deleted_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("keep");
        let gone = tmp.path().join("gone");
        std::fs::create_dir(&real).unwrap();
        std::fs::create_dir(&gone).unwrap();
        let db = Database::in_memory().unwrap();
        db.record_visit(&real.to_string_lossy()).unwrap();
        db.record_visit(&gone.to_string_lossy()).unwrap();
        std::fs::remove_dir(&gone).unwrap();
        let cfg = Config::default();
        let best = find_best(&db, &cfg, "gone");
        assert!(
            best.is_none(),
            "must not return deleted dir, got {:?}",
            best
        );
    }

    #[test]
    fn bookmark_exact_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("bm");
        std::fs::create_dir(&real).unwrap();
        let db = Database::in_memory().unwrap();
        db.set_bookmark("xyz", &real.to_string_lossy()).unwrap();
        let cfg = Config::default();
        assert_eq!(
            find_best(&db, &cfg, "xyz").as_deref(),
            Some(real.to_str().unwrap())
        );
    }
}

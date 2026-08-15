use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::completions;
use crate::config::Config;
use crate::db::{canonicalize_path, default_data_dir, expand_home, now_secs, Database, HistoryRow};
use crate::init;
use crate::picker;
use crate::score::{Scored, Scorer};
use crate::{doctor, import};
use serde_json;

pub const HELP: &str = r#"hop — smart directory jump

Usage:
    hop <query>                  Jump to best match (prints path)
    hop -                        Previous directory (like cd -)
    hop ~/path, .., ./dir        Literal paths work like cd
    hop foo bar                  All words must match (AND)
    hop foo /                    Subdirectory of cwd starting with "foo"
    hop p|pick [query]           Same; empty query opens picker
    hop add <path>               Record a visit
    hop rm <path>                Remove from history (exact path)
    hop forget [query]           Remove from history; empty query opens picker
    hop book [alias] [path]      Set or resolve a bookmark
    hop book list [--json]       List all bookmarks
    hop book rm <alias>          Delete a bookmark
    hop book edit <alias>        Edit alias, path, or description
    hop history [n]              Top n by visits (default 20)
    hop recent [n]               Last n visited (default 20)
    hop score <query> [--json]   Show per-component score breakdown
    hop list <query> [--limit N] [--json]  List all scored matches
    hop export [--format json|csv|tsv]  Dump history/bookmarks
    hop import fasd|autojump|zoxide|zsh <file>  Import from another tool
    hop prune [--dry-run]        Remove stale (deleted) paths
    hop clear [--force]          Wipe history (prompts by default)
    hop stats                    DB stats
    hop doctor                   Diagnose setup
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

    // Auto-prune on startup if configured
    let verbose = args.iter().any(|a| a == "--verbose" || a == "-V");
    if cfg.auto_prune_on_startup {
        if let Ok(removed) = db.prune_auto() {
            if verbose && removed > 0 {
                eprintln!("auto-pruned {} stale entries", removed);
            }
        }
    }

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
            cmd_jump(&db, &cfg, &query)
        }
        "-" => cmd_prev(&db),
        "add" => {
            // Strip the `--` separator — every shell hook calls `hop add -- "$PWD"`.
            // --dry-run may appear before or after the path.
            let rest: Vec<&str> = args[2..]
                .iter()
                .map(String::as_str)
                .filter(|s| *s != "--")
                .collect();
            let dry_run = rest.contains(&"--dry-run");
            let raw_arg = rest.iter().copied().find(|a| *a != "--dry-run");

            let Some(raw_arg) = raw_arg else {
                eprintln!("Usage: hop add <path> [--dry-run]");
                return ExitCode::from(2);
            };

            if raw_arg.is_empty() {
                eprintln!("empty path; did you mean to run `hop` without arguments?");
                return ExitCode::from(1);
            }

            let path = expand_home(raw_arg);
            if !path.is_dir() {
                eprintln!("not a directory: {}", path.display());
                return ExitCode::from(1);
            }

            // Canonicalize so we check/use the stored form
            let canon = canonicalize_path(&path.to_string_lossy())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());

            if dry_run {
                // Check if entry exists to determine "would add" vs "would create"
                let existing = db
                    .history_rows()
                    .ok()
                    .and_then(|rows| rows.into_iter().find(|r| r.path == canon));
                if let Some(row) = existing {
                    println!("would add {} with {} visits", canon, row.visits + 1);
                } else {
                    println!("would create new entry: {}", canon);
                }
                return ExitCode::SUCCESS;
            }
            let _ = db.record_visit(&canon);
            ExitCode::SUCCESS
        }
        "rm" => {
            let Some(arg) = positional(&args, 2) else {
                eprintln!("Usage: hop rm <path>");
                return ExitCode::from(2);
            };
            let path = expand_home(arg);
            // History stores canonical paths (record_visit canonicalizes), so
            // match against the canonical form — otherwise `rm` fails for any
            // path reached through a symlink (e.g. /var → /private/var on macOS).
            let canon = canonicalize_path(&path.to_string_lossy())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            let removed = match db.forget(&canon) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("remove failed: {}", e);
                    return ExitCode::from(1);
                }
            };
            if removed > 0 {
                println!("removed: {}", canon);
                ExitCode::SUCCESS
            } else {
                println!("not found in history: {}", canon);
                ExitCode::from(1)
            }
        }
        "forget" => {
            let dry_run = args[2..].iter().any(|a| a == "--dry-run");
            let query: String = args[2..]
                .iter()
                .filter(|a| *a != "--dry-run")
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(" ");
            if query.is_empty() {
                if dry_run {
                    eprintln!("Usage: hop forget <query> [--dry-run]");
                    return ExitCode::from(2);
                }
                // No query → interactive picker to choose what to forget.
                return cmd_forget_pick(&db);
            }
            match find_best(&db, &cfg, &query) {
                Some(path) => {
                    if dry_run {
                        println!("would forget: {}", path);
                        return ExitCode::SUCCESS;
                    }
                    let removed = match db.forget(&path) {
                        Ok(n) => n,
                        Err(e) => {
                            eprintln!("forget failed: {}", e);
                            0
                        }
                    };
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
        "book" => {
            let book_json = args[2..].iter().any(|a| a == "--json" || a == "-j");
            cmd_bookmark(&db, &args[2..], book_json)
        }
        "history" => {
            let n = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);
            let rows = match db.top(n) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("history query failed: {}", e);
                    return ExitCode::from(1);
                }
            };
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
            cmd_score(&db, &query, is_json)
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
            cmd_list(&db, &query, limit, is_json)
        }
        "export" => {
            let format = args
                .iter()
                .position(|a| a == "--format")
                .and_then(|i| args.get(i + 1).cloned())
                .unwrap_or_else(|| "json".to_string());
            cmd_export(&db, &format)
        }
        "recent" => {
            let n = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);
            let recent_rows = match db.recent(n) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("recent query failed: {}", e);
                    return ExitCode::from(1);
                }
            };
            print_rows(&Database::filter_live_rows(recent_rows));
            ExitCode::SUCCESS
        }
        "import" => {
            // Check for --dry-run flag (can appear before or after source)
            let dry_run = args[2..].iter().any(|a| a == "--dry-run");
            let source;
            let file;

            if args.len() >= 5 && args[2] == "--dry-run" {
                // hop import --dry-run <source> <file>
                source = args[3].as_str();
                file = Path::new(&args[4]);
            } else if args.len() >= 5 && args[3] == "--dry-run" {
                // hop import <source> --dry-run <file>
                source = args[2].as_str();
                file = Path::new(&args[4]);
            } else if args.len() == 4 && args[2] == "--dry-run" {
                // hop import --dry-run <source> — file is missing
                eprintln!("Usage: hop import [--dry-run] <fasd|autojump|zoxide|zsh> <file>");
                return ExitCode::from(2);
            } else if args.len() >= 4 {
                source = args[2].as_str();
                file = Path::new(&args[3]);
            } else {
                eprintln!("Usage: hop import [--dry-run] <fasd|autojump|zoxide|zsh> <file>");
                return ExitCode::from(2);
            };

            if dry_run {
                match import::import_dry_run(source, file) {
                    Ok(preview) => {
                        println!(
                            "would import {} entries: {}",
                            preview.len(),
                            preview.join(", ")
                        );
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("import dry-run failed: {}", e);
                        ExitCode::from(1)
                    }
                }
            } else {
                let result = match source {
                    "fasd" => import::import_fasd(&db, file),
                    "autojump" => import::import_autojump(&db, file),
                    "zoxide" => import::import_zoxide(&db, file),
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
        }
        "prune" => {
            let dry_run = args.get(2).map(String::as_str) == Some("--dry-run");
            let quiet = args.get(2).map(String::as_str) == Some("--quiet")
                || args.get(3).map(String::as_str) == Some("--quiet");
            if dry_run {
                match db.prune_stale_dry_run() {
                    Ok(history_stale) => {
                        if history_stale.is_empty() {
                            println!("nothing to prune");
                        } else {
                            for p in &history_stale {
                                println!("  - {}", p);
                            }
                            println!(
                                "\n{} stale entr{} total. Run without --dry-run to remove.",
                                history_stale.len(),
                                if history_stale.len() == 1 { "y" } else { "ies" }
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("prune dry-run failed: {}", e);
                        return ExitCode::from(1);
                    }
                }
            } else {
                let grand_total = db.history_rows().map(|r| r.len()).unwrap_or(0);

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
                eprint!("this will wipe ALL history. type 'yes' to confirm: ");
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
        "stats" => {
            let verbose = args.iter().any(|a| a == "--verbose" || a == "-V");
            cmd_stats(&db, verbose)
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
            cmd_jump(&db, &cfg, &query)
        }
    }
}

/// Resolve `query` to a path and print it, recording the visit. Prints a hint
/// on cold-start (empty history) instead of failing silently.
fn cmd_jump(db: &Database, cfg: &Config, query: &str) -> ExitCode {
    match find_best(db, cfg, query) {
        Some(path) => {
            println!("{}", path);
            let _ = db.record_visit(&path);
            ExitCode::SUCCESS
        }
        None => {
            if db.history_rows().map(|r| r.is_empty()).unwrap_or(true) {
                eprintln!(
                    "no history yet — cd around (or run `hop add <dir>`) to teach hop your directories"
                );
            }
            ExitCode::from(1)
        }
    }
}

/// `hop -`: print the second-most-recent live directory, like `cd -` but
/// across sessions. Records the jump so repeated use toggles back and forth.
fn cmd_prev(db: &Database) -> ExitCode {
    let rows = match db.recent(2) {
        Ok(r) => Database::filter_live_rows(r),
        Err(e) => {
            eprintln!("history query failed: {}", e);
            return ExitCode::from(1);
        }
    };
    let Some(prev) = rows.get(1) else {
        eprintln!("no previous directory yet — visit a couple of dirs first");
        return ExitCode::from(1);
    };
    println!("{}", prev.path);
    let _ = db.record_visit(&prev.path);
    ExitCode::SUCCESS
}

/// `hop forget` with no query: pick an entry interactively, then delete it.
fn cmd_forget_pick(db: &Database) -> ExitCode {
    match picker::run(db, "") {
        Ok(Some(path)) => match db.forget(&path) {
            Ok(n) if n > 0 => {
                println!("forgot: {}", path);
                ExitCode::SUCCESS
            }
            _ => {
                eprintln!("not found in history: {}", path);
                ExitCode::from(1)
            }
        },
        Ok(None) => ExitCode::from(1),
        Err(_) => ExitCode::from(1),
    }
}

/// Escape a field for CSV output: quote and double any embedded quotes.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
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

fn cmd_bookmark(db: &Database, args: &[String], is_json: bool) -> ExitCode {
    // --json/-j may appear before or after the subcommand; strip them so
    // `hop book --json list` dispatches to list instead of creating a
    // bookmark aliased "--json".
    let args: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| *a != "--json" && *a != "-j")
        .collect();

    if args.is_empty() || args[0] == "list" {
        match db.bookmarks() {
            Ok(bms) => {
                if is_json {
                    let items: Vec<_> = bms
                        .iter()
                        .map(|(alias, path, description)| {
                            serde_json::json!({ "alias": alias, "path": path, "description": description })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&items).unwrap());
                } else {
                    for (alias, path, _description) in bms {
                        println!("{:20}  {}", alias, path);
                    }
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
        let removed = match db.remove_bookmark(args[1]) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("remove_bookmark failed: {}", e);
                return ExitCode::from(1);
            }
        };
        if removed == 0 {
            eprintln!("no such bookmark: {}", args[1]);
            return ExitCode::from(1);
        }
        ExitCode::SUCCESS
    } else if args[0] == "edit" {
        if args.len() < 2 {
            eprintln!("Usage: hop book edit <alias> [--alias <new>] [--path <path>] [--description <text>]");
            return ExitCode::from(2);
        }
        let alias = &args[1];

        // Parse --alias, --path, --description flags
        let mut new_alias: Option<&str> = None;
        let mut new_path: Option<&str> = None;
        let mut new_description: Option<&str> = None;

        let mut i = 2;
        while i < args.len() {
            match args[i] {
                "--alias" => {
                    i += 1;
                    new_alias = args.get(i).copied();
                }
                "--path" => {
                    i += 1;
                    new_path = args.get(i).copied();
                }
                "--description" => {
                    i += 1;
                    new_description = args.get(i).copied();
                }
                _ => {
                    eprintln!("unknown flag: {}\nUsage: hop book edit <alias> [--alias <new>] [--path <path>] [--description <text>]", args[i]);
                    return ExitCode::from(2);
                }
            }
            i += 1;
        }

        if new_alias.is_none() && new_path.is_none() && new_description.is_none() {
            // No flags: print current values
            match db.bookmarks() {
                Ok(bms) => {
                    if let Some((_a, _p, _d)) = bms.iter().find(|(a, _, _)| a == alias) {
                        let (a, p, d) = bms.iter().find(|(a, _, _)| *a == *alias).unwrap();
                        println!("alias:       {}", a);
                        println!("path:        {}", p);
                        println!("description: {}", d);
                    } else {
                        eprintln!("no such bookmark: {}", alias);
                        return ExitCode::from(1);
                    }
                }
                Err(e) => {
                    eprintln!("db error: {}", e);
                    return ExitCode::from(1);
                }
            }
            return ExitCode::SUCCESS;
        }

        // Validate path if provided
        let new_path_owned: Option<String> = 'blk: {
            if let Some(p) = new_path {
                let expanded = expand_home(p);
                if !expanded.is_dir() {
                    eprintln!("not a directory: {}", expanded.display());
                    break 'blk None;
                }
                break 'blk Some(expanded.to_string_lossy().into_owned());
            }
            None
        };

        if new_path_owned.is_none() && new_path.is_some() {
            return ExitCode::from(1);
        }

        match db.edit_bookmark(alias, new_alias, new_path_owned.as_deref(), new_description) {
            Ok(0) => {
                eprintln!("no such bookmark: {}", alias);
                ExitCode::from(1)
            }
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("edit failed: {}", e);
                ExitCode::from(1)
            }
        }
    } else {
        let alias = &args[0];
        if args.len() > 1 {
            let path = expand_home(args[1]);
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

fn cmd_stats(db: &Database, verbose: bool) -> ExitCode {
    match db.counts() {
        Ok(c) => {
            println!(
                "paths: {}\nvisits: {}\nbookmarks: {}\ntop: {}",
                c.total,
                c.total_visits,
                c.bookmarks,
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
            if verbose {
                println!();
                // DB file size
                if let Ok(db_path) = default_data_dir().canonicalize() {
                    if let Ok(metadata) = std::fs::metadata(&db_path) {
                        let size_bytes = metadata.len();
                        let size_str = if size_bytes > 1_073_741_824 {
                            format!("{:.2} GB", size_bytes as f64 / 1_073_741_824.0)
                        } else if size_bytes > 1_048_576 {
                            format!("{:.2} MB", size_bytes as f64 / 1_048_576.0)
                        } else if size_bytes > 1024 {
                            format!("{:.2} KB", size_bytes as f64 / 1024.0)
                        } else {
                            format!("{} B", size_bytes)
                        };
                        println!("db size: {} ({})", size_str, db_path.display());
                    }
                }
                // Date range of history
                if let Ok(rows) = db.history_rows() {
                    if !rows.is_empty() {
                        let oldest = rows
                            .iter()
                            .map(|r| r.last_visited)
                            .fold(f64::INFINITY, |a, b| a.min(b));
                        let newest = rows
                            .iter()
                            .map(|r| r.last_visited)
                            .fold(f64::NEG_INFINITY, |a, b| a.max(b));
                        let oldest_days = (now_secs() - oldest) / 86_400.0;
                        let newest_days = (now_secs() - newest) / 86_400.0;
                        println!(
                            "history range: {:.1} days ago to {:.1} days ago ({} entries)",
                            oldest_days,
                            newest_days,
                            rows.len()
                        );
                    }
                }
                // Top 10 most visited dirs
                let top10 = match db.top(10) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("top query failed: {}", e);
                        return ExitCode::from(1);
                    }
                };
                let live_top10 = Database::filter_live_rows(top10);
                if !live_top10.is_empty() {
                    println!();
                    println!("top 10 most visited:");
                    for r in &live_top10 {
                        println!("  {:>6} visits  {}", r.visits, r.path);
                    }
                } else {
                    println!();
                    println!("top 10 most visited: (none)");
                }
                // Longest-unvisited (oldest last_visited but still in DB)
                if let Ok(rows) = db.history_rows() {
                    if let Some(oldest) = rows.iter().min_by(|a, b| {
                        a.last_visited
                            .partial_cmp(&b.last_visited)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    }) {
                        println!();
                        println!(
                            "longest-unvisited: {} (last visited {:.1} days ago)",
                            oldest.path,
                            (now_secs() - oldest.last_visited) / 86_400.0
                        );
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("stats failed: {}", e);
            ExitCode::from(1)
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
    // "foo /" → the most recently modified subdirectory of cwd starting with "foo"
    if let Some((prefix, rest)) = query.split_once(' ') {
        if rest.trim() == "/" && !prefix.trim().is_empty() {
            if let Some(p) = subdir_match(prefix.trim()) {
                return Some(p);
            }
        }
    }

    // If the query resolves to an existing directory (possibly via symlink or
    // ~ expansion), canonicalize it so we match the canonical path in history.
    let expanded = expand_home(query);
    if let Some(canonical) = canonicalize_path(&expanded.to_string_lossy()) {
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

    let cands = score_candidates(db, query);
    let cutoff = cands
        .iter()
        .position(|c| c.score < cfg.min_score)
        .unwrap_or(cands.len());
    first_live(&cands[..cutoff], 20).map(|c| c.path.clone())
}

/// Shared helper: score all sources (bookmarks, history) and return sorted,
/// deduped candidates. Pure in-memory — no filesystem checks — so the hot
/// path (find_best) pays no stat() per history row. Callers that display
/// results apply [`live_candidates`]; [`first_live`] finds the jump target.
fn score_candidates(db: &Database, query: &str) -> Vec<Scored> {
    let scorer = Scorer::new(now_secs());
    let mut cands: Vec<Scored> = Vec::new();

    if let Ok(bms) = db.bookmarks() {
        for (alias, path, _description) in bms {
            if let Some(s) = scorer.score_bookmark(&alias, &path, query) {
                cands.push(s);
            }
        }
    }

    if let Ok(rows) = db.history_rows() {
        let (scored, _) = crate::score::score_history_batch(&scorer, &rows, query);
        cands.extend(scored);
    }

    cands.sort_by_key(|c| std::cmp::Reverse(c.score));
    cands.dedup_by(|a, b| a.path == b.path);
    cands
}

/// Keep only candidates whose directory still exists (for display commands).
fn live_candidates(cands: Vec<Scored>) -> Vec<Scored> {
    cands
        .into_iter()
        .filter(|c| Path::new(&c.path).is_dir())
        .collect()
}

/// First candidate whose directory still exists. Checks the top `budget`
/// (stale dirs rank low, so the fast path is almost always enough), then falls
/// back to a full scan so a pathological all-stale top can't hide a live match.
fn first_live(cands: &[Scored], budget: usize) -> Option<&Scored> {
    cands
        .iter()
        .take(budget)
        .find(|c| Path::new(&c.path).is_dir())
        .or_else(|| {
            cands
                .iter()
                .skip(budget)
                .find(|c| Path::new(&c.path).is_dir())
        })
}

/// `"foo /"` → the most recently modified subdirectory of cwd whose name
/// starts with `prefix`.
fn subdir_match(prefix: &str) -> Option<String> {
    let cwd = std::env::current_dir().ok()?;
    let mut best: Option<(PathBuf, SystemTime)> = None;
    for entry in std::fs::read_dir(&cwd).ok()?.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with(prefix) {
            continue;
        }
        let mtime = entry
            .path()
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(UNIX_EPOCH);
        if best.as_ref().map(|(_, t)| mtime >= *t).unwrap_or(true) {
            best = Some((entry.path(), mtime));
        }
    }
    best.map(|(p, _)| p.to_string_lossy().into_owned())
}

fn print_rows(rows: &[HistoryRow]) {
    for r in rows {
        println!("{:4} visits   {}", r.visits, r.path);
    }
}

fn cmd_score(db: &Database, query: &str, is_json: bool) -> ExitCode {
    let cands = live_candidates(score_candidates(db, query));

    if cands.is_empty() {
        return ExitCode::from(1);
    }

    if is_json {
        // Print top 10 as JSON
        let tops: Vec<_> = cands
            .iter()
            .take(10)
            .map(|c| {
                serde_json::json!({
                    "path": c.path,
                    "total": c.score,
                    "fuzzy": c.fuzzy,
                    "visits": c.visits,
                    "recency": c.recency,
                    "git": c.git,
                    "basename": c.basename,
                    "shortness": c.shortness,
                    "session": c.session,
                    "source": format!("{:?}", c.source).to_lowercase(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&tops).unwrap());
    } else {
        // Human-readable per-component breakdown
        println!("query: {}", query);
        println!();
        for (i, c) in cands.iter().take(10).enumerate() {
            let trophy = if i == 0 { " (best)" } else { "" };
            println!(
                "{}{}  total={:>4}  fuzzy={:>3}  visits={:>3}  recency={:>2}  git={:>2}  basename={:>2}  shortness={:>2}  session={:>2}  [{:?}]",
                c.path,
                trophy,
                c.score,
                c.fuzzy,
                c.visits,
                c.recency,
                c.git,
                c.basename,
                c.shortness,
                c.session,
                c.source,
            );
        }
    }
    ExitCode::SUCCESS
}

fn cmd_list(db: &Database, query: &str, limit: usize, is_json: bool) -> ExitCode {
    let mut scored = live_candidates(score_candidates(db, query));
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
    let history = match db.history_rows() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("export history failed: {}", e);
            return ExitCode::from(1);
        }
    };
    let bookmarks = match db.bookmarks() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("export bookmarks failed: {}", e);
            return ExitCode::from(1);
        }
    };

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
                "bookmarks": bookmarks.iter().map(|(alias, path, description)| {
                    serde_json::json!({
                        "alias": alias,
                        "path": path,
                        "description": description,
                    })
                }).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&payload).unwrap());
        }
        "csv" => {
            // Header: path,visits,last_visited,is_bookmark,alias,description
            println!("path,visits,last_visited,is_bookmark,alias,description");
            for r in &history {
                println!(
                    "{},{},{},false,,",
                    csv_field(&r.path),
                    r.visits,
                    r.last_visited
                );
            }
            for (alias, path, description) in &bookmarks {
                // For bookmarks, visits=0 and is_bookmark=true
                println!(
                    "{},0,0,true,{},{}",
                    csv_field(path),
                    csv_field(alias),
                    csv_field(description)
                );
            }
        }
        "tsv" => {
            for r in &history {
                println!(
                    "history\t{}\t{}\t{}\t{}",
                    r.path, r.visits, r.last_visited, r.is_git_repo
                );
            }
            for (alias, path, description) in &bookmarks {
                println!("bookmark\t{}:{}\t0\t0\tfalse\t{}", alias, path, description);
            }
        }
        _ => {
            eprintln!("unknown format '{}': use json, csv, or tsv", format);
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
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

    #[test]
    fn find_best_multi_token_and_semantics() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("my-project");
        let other = tmp.path().join("project-other");
        std::fs::create_dir(&proj).unwrap();
        std::fs::create_dir(&other).unwrap();

        let db = Database::in_memory().unwrap();
        db.record_visit(&proj.to_string_lossy()).unwrap();
        db.record_visit(&other.to_string_lossy()).unwrap();
        let cfg = Config::default();

        // Both tokens match only my-project (which gets the basename bonus);
        // the negative case (no dir contains both words) is covered
        // deterministically in score.rs with fixed paths — tempdir suffixes
        // contain random letters that can complete tokens.
        let best = find_best(&db, &cfg, "my project");
        assert_eq!(
            best.as_deref(),
            Some(
                canonicalize_path(proj.to_str().unwrap())
                    .unwrap()
                    .as_str()
            )
        );
    }

    #[test]
    fn find_best_expands_tilde() {
        // expand_home reads $HOME; point it at a temp dir.
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let sub = home.join("sub");
        std::fs::create_dir_all(&sub).unwrap();

        let db = Database::in_memory().unwrap();
        let cfg = Config::default();
        let prev = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);
        let best = find_best(&db, &cfg, "~/sub");
        match prev {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
        assert_eq!(
            best.as_deref(),
            Some(canonicalize_path(sub.to_str().unwrap()).unwrap().as_str())
        );
    }

    #[test]
    fn find_best_literal_relative_and_dotdot() {
        let tmp = tempfile::tempdir().unwrap();
        let child = tmp.path().join("child");
        std::fs::create_dir(&child).unwrap();

        let db = Database::in_memory().unwrap();
        let cfg = Config::default();

        // `hop ..` from inside child resolves to the parent.
        let prev_cwd = std::env::current_dir().ok();
        std::env::set_current_dir(&child).unwrap();
        let up = find_best(&db, &cfg, "..");
        if let Some(cwd) = prev_cwd {
            std::env::set_current_dir(cwd).unwrap();
        }
        assert_eq!(
            up.as_deref(),
            Some(canonicalize_path(tmp.path().to_str().unwrap()).unwrap().as_str())
        );
    }
}

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use rusqlite::{params, Connection};

struct HistoryEntry {
    path: String,
    visits: i32,
    last_visited: f64,
}

const APP_NAME: &str = "fuzzy-cd";
const DB_NAME: &str = "fuzzy-cd.db";

fn get_data_dir() -> PathBuf {
    ProjectDirs::from("", "", APP_NAME)
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".fuzzy-cd")
        })
}

fn get_db_path() -> PathBuf {
    get_data_dir().join(DB_NAME)
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"))
    } else if let Some(suffix) = path.strip_prefix("~/") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(suffix)
    } else {
        PathBuf::from(path)
    }
}

fn open_db() -> Connection {
    let db_path = get_db_path();
    fs::create_dir_all(db_path.parent().unwrap()).expect("Failed to create data directory");
    let conn = Connection::open(&db_path).expect("Failed to open database");
    conn.execute("PRAGMA journal_mode=WAL", []).ok();
    init_db(&conn).expect("Failed to initialize database");
    conn
}

fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY,
            path TEXT UNIQUE NOT NULL,
            visits INTEGER DEFAULT 1,
            last_visited REAL NOT NULL,
            created_at REAL NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS bookmarks (
            id INTEGER PRIMARY KEY,
            alias TEXT UNIQUE NOT NULL,
            path TEXT NOT NULL,
            created_at REAL NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS recent_dirs (
            id INTEGER PRIMARY KEY,
            path TEXT UNIQUE NOT NULL,
            visited_at REAL NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )?;
    // Build indices for fast lookups
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_history_visits ON history(visits DESC)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_history_last ON history(last_visited DESC)",
        [],
    )?;
    Ok(())
}

// ─── Scoring ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct ScoredPath {
    path: String,
    score: i64,
    is_bookmark: bool,
    is_git_repo: bool,
    basename_bonus: bool,
}

fn score_path(path: &str, query: &str, visits: i32, last_visited: f64, now: f64) -> i64 {
    let matcher = SkimMatcherV2::default();

    let fuzzy_score = matcher.fuzzy_match(path, query).unwrap_or(0);

    // Basename match is worth a lot
    let basename = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let query_lower = query.to_lowercase();
    let basename_bonus = basename.contains(&query_lower) || fuzzy_score > 80;

    // Recency: fresh visits rank higher (decay over 30 days)
    let age_days = (now - last_visited) / 86400.0;
    let recency = if age_days < 1.0 {
        3.0  // visited today
    } else if age_days < 7.0 {
        2.0  // visited this week
    } else if age_days < 30.0 {
        1.0
    } else {
        0.5
    };

    // Visit frequency
    let visit_boost = (visits as f64).sqrt().min(5.0);

    // Git repo bonus
    let is_git_repo = Path::new(path).join(".git").exists();

    // Short path bonus (shorter = likely more relevant)
    let path_depth = path.matches('/').count() as f64;
    let shortness = (10.0_f64 / path_depth).max(1.0);

    // Combined score
    let base: i64 = fuzzy_score as i64;
    let score = base
        + (visit_boost * 20.0) as i64
        + (recency * 15.0) as i64
        + if is_git_repo { 30 } else { 0 }
        + if basename_bonus { 40 } else { 0 }
        + (shortness * 5.0) as i64;

    score
}

fn get_cached_recent_dirs(conn: &Connection) -> Vec<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    let mut stmt = conn
        .prepare("SELECT path FROM recent_dirs WHERE ?1 - visited_at < 86400 ORDER BY visited_at DESC LIMIT 50")
        .unwrap();
    stmt.query_map(params![now], |row| row.get::<_, String>(0))
        .unwrap()
        .flatten()
        .collect()
}

// ─── Matching ────────────────────────────────────────────────────────────────

fn find_best_match(conn: &Connection, query: &str) -> Option<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    // 1. Check bookmarks first — exact alias match wins
    {
        let mut stmt = conn
            .prepare("SELECT path FROM bookmarks WHERE alias = ?1")
            .ok()?;
        let mut rows = stmt.query(params![query]).ok()?;
        if let Some(row) = rows.next().ok()? {
            return Some(row.get::<_, String>(0).ok()?);
        }
    }

    let mut candidates: Vec<ScoredPath> = Vec::new();

    // 2. Fuzzy search bookmarks
    {
        let mut stmt = conn.prepare("SELECT alias, path FROM bookmarks").ok()?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))).ok()?;
        for row in rows.flatten() {
            let (alias, path) = row;
            let matcher = SkimMatcherV2::default();
            if let Some(score) = matcher.fuzzy_match(&alias, query) {
                candidates.push(ScoredPath {
                    path,
                    score: score * 3, // bookmarks are 3x weighted
                    is_bookmark: true,
                    is_git_repo: false,
                    basename_bonus: true,
                });
            }
        }
    }

    // 3. Fuzzy search history
    {
        let mut stmt = conn.prepare("SELECT path, visits, last_visited FROM history").ok()?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i32>(1)?,
                row.get::<_, f64>(2)?,
            ))
        }).ok()?;

        for row in rows.flatten() {
            let (path, visits, last_visited) = row;
            let score = score_path(&path, query, visits, last_visited, now);
            let is_git_repo = Path::new(&path).join(".git").exists();
            candidates.push(ScoredPath {
                path,
                score,
                is_bookmark: false,
                is_git_repo,
                basename_bonus: false,
            });
        }
    }

    // 4. Fallback: scan recent dirs not yet in history
    for path in get_cached_recent_dirs(conn) {
        if !candidates.iter().any(|c| c.path == path) {
            let matcher = SkimMatcherV2::default();
            if let Some(score) = matcher.fuzzy_match(&path, query) {
                candidates.push(ScoredPath {
                    path,
                    score: score / 2,
                    is_bookmark: false,
                    is_git_repo: false,
                    basename_bonus: false,
                });
            }
        }
    }

    // Sort by score descending
    candidates.sort_by(|a, b| b.score.cmp(&a.score));

    // Return best match if score is reasonable
    candidates.first().map(|c| c.path.clone())
}

// ─── History Management ────────────────────────────────────────────────────────

fn record_visit(conn: &Connection, path: &str) -> rusqlite::Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    // Upsert history
    conn.execute(
        "INSERT INTO history (path, visits, last_visited, created_at)
         VALUES (?1, 1, ?2, ?2)
         ON CONFLICT(path) DO UPDATE SET
            visits = visits + 1,
            last_visited = ?2",
        params![path, now],
    )?;

    // Update recent dirs
    conn.execute(
        "INSERT INTO recent_dirs (path, visited_at) VALUES (?1, ?2)
         ON CONFLICT(path) DO UPDATE SET visited_at = ?2",
        params![path, now],
    )?;

    // Keep recent_dirs table trimmed
    conn.execute(
        "DELETE FROM recent_dirs WHERE id NOT IN (
            SELECT id FROM recent_dirs ORDER BY visited_at DESC LIMIT 200
        )",
        [],
    )?;

    Ok(())
}

fn list_history(conn: &Connection, limit: usize) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT path, visits, last_visited FROM history ORDER BY visits DESC, last_visited DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i32], |row| {
        Ok(HistoryEntry {
            path: row.get::<_, String>(0)?,
            visits: row.get::<_, i32>(1)?,
            last_visited: row.get::<_, f64>(2)?,
        })
    })?;

    for row in rows {
        let entry = row?;
        let age = age_string(entry.last_visited);
        println!("{:4} visits  {:>8}  {}", entry.visits, age, entry.path);
    }
    Ok(())
}

fn age_string(last_visited: f64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let secs = now - last_visited;
    if secs < 60.0 {
        "just now".into()
    } else if secs < 3600.0 {
        format!("{:.0}m ago", secs / 60.0)
    } else if secs < 86400.0 {
        format!("{:.0}h ago", secs / 3600.0)
    } else if secs < 86400.0 * 7.0 {
        format!("{:.0}d ago", secs / 86400.0)
    } else {
        format!("{:.0}w ago", secs / (86400.0 * 7.0))
    }
}

// ─── Bookmarks ────────────────────────────────────────────────────────────────

fn add_bookmark(conn: &Connection, alias: &str, path: &str) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    conn.execute(
        "INSERT INTO bookmarks (alias, path, created_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(alias) DO UPDATE SET path = ?2",
        params![alias, path, now],
    ).ok();
    println!("Bookmark '{}' -> {}", alias, path);
}

fn list_bookmarks(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare("SELECT alias, path FROM bookmarks ORDER BY alias")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    for row in rows {
        let (alias, path) = row?;
        println!("{:20}  {}", alias, path);
    }
    Ok(())
}

fn remove_bookmark(conn: &Connection, alias: &str) {
    conn.execute("DELETE FROM bookmarks WHERE alias = ?1", params![alias]).ok();
    println!("Removed bookmark: {}", alias);
}

// ─── Import ─────────────────────────────────────────────────────────────────

fn import_fasd(conn: &Connection, path: &str) -> Result<usize, std::io::Error> {
    let content = fs::read_to_string(path)?;
    let mut count = 0;

    for line in content.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.is_empty() {
            continue;
        }
        let p = expand_home(parts[0].trim());
        if p.exists() && p.is_dir() {
            let visits = parts.get(1).and_then(|v| v.trim().parse::<i32>().ok()).unwrap_or(1);
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64();

            let _ = conn.execute(
                "INSERT INTO history (path, visits, last_visited, created_at)
                 VALUES (?1, ?2, ?3, ?3)
                 ON CONFLICT(path) DO UPDATE SET visits = visits + ?2",
                params![p.to_string_lossy(), visits.min(100), now],
            );
            count += 1;
        }
    }

    Ok(count)
}

fn import_zsh_history(conn: &Connection, path: &str) -> Result<usize, std::io::Error> {
    let content = fs::read_to_string(path)?;
    let mut count = 0;
    let mut seen = std::collections::HashSet::new();

    for line in content.lines() {
        // Skip non-cd commands
        let line = line.split_whitespace().nth(1).unwrap_or("");
        if !line.starts_with("cd") && !line.starts_with("pushd") {
            continue;
        }

        // Extract directory from cd command
        let cmd = line.trim_start_matches("cd").trim_start_matches("pushd").trim();
        let p = expand_home(cmd.trim_start_matches('~'));
        if p.exists() && p.is_dir() && !seen.contains(&p) {
            seen.insert(p.clone());
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64();

            conn.execute(
                "INSERT INTO history (path, visits, last_visited, created_at)
                 VALUES (?1, 1, ?2, ?2)
                 ON CONFLICT(path) DO UPDATE SET visits = visits + 1, last_visited = ?2",
                params![p.to_string_lossy(), now],
            ).ok();
            count += 1;
        }
    }

    Ok(count)
}

// ─── Indexing ─────────────────────────────────────────────────────────────────

fn index_directories(conn: &Connection, root: &Path) -> rusqlite::Result<usize> {
    let mut count = 0;

    fn walk_dir(conn: &Connection, dir: &Path, count: &mut usize) -> rusqlite::Result<()> {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        let skip_dirs = [
            "Library", "Music", "Movies", "Pictures",
            "Documents", "Applications", "Desktop",
            "node_modules", "target", ".git", ".svn", ".hg",
            "__pycache__", ".venv", "venv", ".cache",
        ];

        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') || skip_dirs.contains(&name_str.as_ref()) {
                    continue;
                }

                if path.is_dir() {
                    *count += 1;
                    let _ = walk_dir(conn, &path, count);
                }
            }
        }
        Ok(())
    }

    walk_dir(conn, root, &mut count)?;
    Ok(count)
}

// ─── CLI ─────────────────────────────────────────────────────────────────────

fn print_help() {
    println!(r#"fuzzy-cd — smart directory jump

Usage:
    fuzzy-cd <query>           Jump to best matching directory
    fuzzy-cd p <query>         Same as above (for shell integration)
    fuzzy-cd add <path>         Add a directory to history
    fuzzy-cd rm <path>         Remove a directory from history
    fuzzy-cd book <alias> [path]   Create bookmark, or show path if no path given
    fuzzy-cd book rm <alias>   Remove bookmark
    fuzzy-cd book list          List all bookmarks
    fuzzy-cd history [n]        Show top n visited dirs (default 20)
    fuzzy-cd import fasd <path> Import from fasd cache
    fuzzy-cd import zsh <path>  Import from zsh history
    fuzzy-cd top               Show top 10 by visit count
    fuzzy-cd recent            Show recently visited
    fuzzy-cd clear             Clear all history
    fuzzy-cd stats             Show database statistics
    fuzzy-cd --reindex         Rebuild internal index
    fuzzy-cd --help            Show this help
"#);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() == 1 {
        print_help();
        return;
    }

    let conn = open_db();
    let home = dirs::home_dir().expect("Could not find home directory");

    match args[1].as_str() {
        "p" | "pick" => {
            let query = args.get(2).map(|s| s.as_str()).unwrap_or("");
            if query.is_empty() {
                return;
            }
            if let Some(path) = find_best_match(&conn, query) {
                println!("{}", path);
                record_visit(&conn, &path).ok();
            }
        }
        "add" => {
            if args.len() < 3 {
                eprintln!("Usage: fuzzy-cd add <path>");
                process::exit(1);
            }
            let path = expand_home(&args[2]);
            if path.exists() && path.is_dir() {
                record_visit(&conn, &path.to_string_lossy()).ok();
                println!("Added: {}", path.display());
            } else {
                eprintln!("Not a directory: {}", path.display());
                process::exit(1);
            }
        }
        "rm" => {
            if args.len() < 3 {
                eprintln!("Usage: fuzzy-cd rm <path>");
                process::exit(1);
            }
            let path = expand_home(&args[2]);
            conn.execute("DELETE FROM history WHERE path = ?1", params![path.to_string_lossy()]).ok();
            println!("Removed: {}", path.display());
        }
        "book" | "bookmark" => {
            handle_bookmark(&conn, &args[2..]);
        }
        "history" => {
            let limit = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);
            list_history(&conn, limit).ok();
        }
        "top" => {
            list_history(&conn, 10).ok();
        }
        "recent" => {
            for path in get_cached_recent_dirs(&conn).into_iter().take(20) {
                println!("{}", path);
            }
        }
        "import" => {
            if args.len() < 4 {
                eprintln!("Usage: fuzzy-cd import <fasd|zsh> <path>");
                process::exit(1);
            }
            let (source, path) = (&args[2][..], &args[3][..]);
            let count = match source {
                "fasd" => import_fasd(&conn, path).unwrap_or(0),
                "zsh" => import_zsh_history(&conn, path).unwrap_or(0),
                _ => {
                    eprintln!("Unknown import source: {}", source);
                    process::exit(1);
                }
            };
            println!("Imported {} entries.", count);
        }
        "clear" => {
            conn.execute("DELETE FROM history", []).ok();
            conn.execute("DELETE FROM recent_dirs", []).ok();
            println!("History cleared.");
        }
        "stats" => {
            let total: i64 = conn.query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0)).unwrap_or(0);
            let total_visits: i64 = conn.query_row("SELECT SUM(visits) FROM history", [], |r| r.get(0)).unwrap_or(0);
            let bookmarks: i64 = conn.query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get(0)).unwrap_or(0);
            let indexed: i64 = conn.query_row("SELECT COUNT(*) FROM recent_dirs", [], |r| r.get(0)).unwrap_or(0);
            let top_path: String = conn.query_row(
                "SELECT path FROM history ORDER BY visits DESC LIMIT 1",
                [],
                |r| r.get(0),
            ).unwrap_or_else(|_| "none".into());
            println!(r#"Database Statistics
  Total paths in history:  {}
  Total visits recorded:   {}
  Bookmarks:              {}
  Recent dirs cached:      {}
  Most visited:            {}"#, total, total_visits, bookmarks, indexed, top_path);
        }
        "--reindex" | "-r" => {
            print!("Indexing {} ... ", home.display());
            let count = index_directories(&conn, &home).unwrap_or(0);
            println!("done ({} dirs scanned)", count);
        }
        "--help" | "-h" | "help" => {
            print_help();
        }
        _ => {
            // Treat as query
            if let Some(path) = find_best_match(&conn, &args[1]) {
                println!("{}", path);
                record_visit(&conn, &path).ok();
            }
        }
    }
}

fn handle_bookmark(conn: &Connection, args: &[String]) {
    if args.is_empty() || args[0] == "list" {
        list_bookmarks(conn).ok();
        return;
    }

    if args[0] == "rm" {
        if args.len() < 2 {
            eprintln!("Usage: fuzzy-cd book rm <alias>");
            process::exit(1);
        }
        remove_bookmark(conn, &args[1]);
        return;
    }

    let alias = &args[0];

    // If second arg given, create bookmark
    if args.len() > 1 {
        let path = expand_home(&args[1]);
        if path.exists() && path.is_dir() {
            add_bookmark(conn, alias, &path.to_string_lossy());
        } else {
            eprintln!("Not a directory: {}", path.display());
            process::exit(1);
        }
        return;
    }

    // No second arg — look up and print the bookmark path
    let mut stmt = conn
        .prepare("SELECT path FROM bookmarks WHERE alias = ?1")
        .ok();
    if let Some(ref mut s) = stmt {
        if let Ok(mut rows) = s.query(params![alias]) {
            if let Ok(Some(row)) = rows.next().map(|r| r.map(|row| row.get::<_, String>(0))) {
                if let Ok(path_str) = row {
                    println!("{}", path_str);
                    return;
                }
            }
        }
    }
    eprintln!(
        "Bookmark '{}' not found. Usage: fuzzy-cd book <alias> [path]",
        alias
    );
    process::exit(1);
}

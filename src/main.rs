use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use directories::ProjectDirs;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use rusqlite::{params, Connection};

const APP_NAME: &str = "fuzzy-cd";
const HISTORY_DB: &str = "history.db";

fn get_data_dir() -> PathBuf {
    ProjectDirs::from("", "", APP_NAME)
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".fuzzy-cd")
        })
}

fn get_history_db_path() -> PathBuf {
    get_data_dir().join(HISTORY_DB)
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

fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY,
            path TEXT UNIQUE NOT NULL,
            visits INTEGER DEFAULT 1,
            last_visited REAL NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS indexed_paths (
            id INTEGER PRIMARY KEY,
            path TEXT UNIQUE NOT NULL,
            basename TEXT NOT NULL,
            parent TEXT NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
        [],
    )?;
    Ok(())
}

fn touch_path(conn: &Connection, path: &str) -> rusqlite::Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    conn.execute(
        "INSERT INTO history (path, visits, last_visited)
         VALUES (?1, 1, ?2)
         ON CONFLICT(path) DO UPDATE SET
            visits = visits + 1,
            last_visited = ?2",
        params![path, now],
    )?;
    Ok(())
}

fn get_matches(conn: &Connection, query: &str, limit: usize) -> rusqlite::Result<Vec<Match>> {
    let matcher = SkimMatcherV2::default();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    // Search history with fuzzy matching
    let mut stmt = conn.prepare(
        "SELECT path, visits, last_visited FROM history",
    )?;

    let history_rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i32>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })?;

    let mut matches: Vec<Match> = Vec::new();
    for row in history_rows {
        let (path, visits, last_visited) = row?;
        if let Some(score) = matcher.fuzzy_match(&path, query) {
            let recency = (now - last_visited).min(86400.0 * 30.0) / (86400.0 * 30.0);
            let final_score = score as f64 * visits as f64 * (1.0 + recency);
            matches.push(Match {
                path: path.clone(),
                score: final_score,
                highlighted: highlight_match(&path, query, &matcher),
            });
        }
    }

    // Also search indexed paths
    let mut stmt = conn.prepare("SELECT path FROM indexed_paths")?;
    let index_rows = stmt.query_map([], |row| Ok(row.get::<_, String>(0)?))?;

    for row in index_rows {
        let path = row?;
        if !matches.iter().any(|m| m.path == path) {
            if let Some(score) = matcher.fuzzy_match(&path, query) {
                let final_score = score as f64 * 0.5;
                matches.push(Match {
                    path: path.clone(),
                    score: final_score,
                    highlighted: highlight_match(&path, query, &matcher),
                });
            }
        }
    }

    matches.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    matches.truncate(limit);
    Ok(matches)
}

fn highlight_match(path: &str, query: &str, matcher: &SkimMatcherV2) -> String {

    if let Some((_, matched_indices)) = matcher.fuzzy_indices(path, query) {
        let mut result = String::new();
        let bytes = path.as_bytes();
        let mut in_match = false;

        for (i, &byte) in bytes.iter().enumerate() {
            let is_matched = matched_indices.contains(&i);
            if is_matched && !in_match {
                result.push_str("\x1b[1m");
                in_match = true;
            } else if !is_matched && in_match {
                result.push_str("\x1b[0m");
                in_match = false;
            }
            result.push(byte as char);
        }

        if in_match {
            result.push_str("\x1b[0m");
        }
        result
    } else {
        path.to_string()
    }
}

struct Match {
    path: String,
    score: f64,
    highlighted: String,
}

fn interactive_pick(matches: &[Match]) -> Option<String> {
    if matches.is_empty() {
        return None;
    }

    let mut selected = 0;

    loop {
        print!("\x1b[2J\x1b[H");
        for (i, m) in matches.iter().enumerate() {
            if i == selected {
                println!("> {}", m.highlighted);
            } else {
                println!("  {}", m.path);
            }
        }
        println!("\n\x1b[2Kcd: {}", matches[selected].path);
        println!("\x1b[1m↑↓ navigate  ↵ confirm  esc cancel\x1b[0m");

        let key = read_key();
        match key {
            3 | 27 => return None,
            13 => return Some(matches[selected].path.clone()),
            65 => {
                if selected > 0 {
                    selected -= 1;
                }
            }
            66 => {
                if selected < matches.len() - 1 {
                    selected += 1;
                }
            }
            _ => {}
        }
    }
}

fn read_key() -> u8 {
    use std::io::Read;
    let mut buf = [0u8; 1];
    std::io::stdin().read_exact(&mut buf).ok();
    buf[0]
}

fn index_directories(conn: &Connection, root: &Path) -> rusqlite::Result<usize> {
    let mut count = 0;

    fn walk_dir(
        conn: &Connection,
        dir: &Path,
        count: &mut usize,
    ) -> rusqlite::Result<()> {
        let entries = fs::read_dir(dir).map_err(|_| rusqlite::Error::InvalidQuery)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.')
                    || name_str == "node_modules"
                    || name_str == "target"
                    || name_str == ".git"
                {
                    continue;
                }

                if path.is_dir() {
                    let full_path = path.to_string_lossy().to_string();
                    let basename = name_str.to_string();
                    let parent = path
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                    let _ = conn.execute(
                        "INSERT OR REPLACE INTO indexed_paths (path, basename, parent) VALUES (?1, ?2, ?3)",
                        params![full_path, basename, parent],
                    );
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

fn print_history(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT path, visits FROM history ORDER BY visits DESC LIMIT 20",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(HistoryEntry {
            path: row.get::<_, String>(0)?,
            visits: row.get::<_, i32>(1)?,
        })
    })?;

    for row in rows {
        let entry = row?;
        println!("{:4} visits  {}", entry.visits, entry.path);
    }
    Ok(())
}

struct HistoryEntry {
    path: String,
    visits: i32,
}

fn cmd_pick(conn: &Connection, query: &str) -> Option<String> {
    let matches = get_matches(conn, query, 10).ok()?;

    if matches.len() == 1 && matches[0].score > 100.0 {
        return Some(matches[0].path.clone());
    }

    interactive_pick(&matches)
}

fn cmd_add(conn: &Connection, path_str: &str) {
    let path = expand_home(path_str);
    if path.exists() && path.is_dir() {
        if touch_path(conn, &path.to_string_lossy()).is_ok() {
            println!("Added: {}", path.display());
        }
    } else {
        eprintln!("Not a valid directory: {}", path.display());
        process::exit(1);
    }
}

fn cmd_reindex(conn: &Connection, home: &Path) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM indexed_paths", [])?;
    index_directories(conn, home)
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let data_dir = get_data_dir();
    fs::create_dir_all(&data_dir).expect("Failed to create data directory");

    let db_path = get_history_db_path();
    let conn = Connection::open(&db_path).expect("Failed to open database");
    init_db(&conn).expect("Failed to initialize database");

    let home = dirs::home_dir().expect("Could not find home directory");

    if args.len() == 1 {
        interactive_mode(&conn);
        return;
    }

    match args[1].as_str() {
        "pick" => {
            let query = args.get(2).map(|s| s.as_str()).unwrap_or("");
            if let Some(path) = cmd_pick(&conn, query) {
                println!("{}", path);
                touch_path(&conn, &path).ok();
            }
        }
        "add" => {
            if args.len() < 3 {
                eprintln!("Usage: fuzzy-cd add <path>");
                process::exit(1);
            }
            cmd_add(&conn, &args[2]);
        }
        "history" => {
            print_history(&conn).ok();
        }
        "--clear-history" | "-c" => {
            conn.execute("DELETE FROM history", []).ok();
            println!("History cleared.");
        }
        "--reindex" | "-r" => {
            print!("Indexing {} ... ", home.display());
            let count = cmd_reindex(&conn, &home).unwrap_or(0);
            println!("{} directories indexed.", count);
        }
        "--help" | "-h" | "help" => {
            print_help();
        }
        "init" => {
            print_init_script();
        }
        _ => {
            if let Some(path) = cmd_pick(&conn, &args[1]) {
                println!("{}", path);
                touch_path(&conn, &path).ok();
            }
        }
    }
}

fn interactive_mode(conn: &Connection) {
    let matches = get_matches(conn, "", 10).unwrap_or_default();
    if let Some(path) = interactive_pick(&matches) {
        println!("{}", path);
        touch_path(conn, &path).ok();
    }
}

fn print_help() {
    println!(
        "fuzzy-cd — intelligent directory navigation

USAGE:
    fuzzy-cd [command] [query]

COMMANDS:
    fuzzy-cd [query]        Interactive fuzzy search, prints selected path
    fuzzy-cd pick <query>   Non-interactive: prints best match or none
    fuzzy-cd add <path>     Manually add a path to history
    fuzzy-cd history        Show most visited directories
    fuzzy-cd --clear-history   Clear visit history
    fuzzy-cd --reindex      Rebuild directory index
    fuzzy-cd init           Print shell integration script
    fuzzy-cd --help         Show this help

SHELL INTEGRATION:
    Add to ~/.zshrc:
        fcd() {{
            local dir
            dir=$(fuzzy-cd pick \"$1\")
            [ -n \"$dir\" ] && cd \"$dir\"
        }}
        alias cd='fcd'
"
    );
}

fn print_init_script() {
    println!(
        "# fuzzy-cd shell integration
# Add to ~/.zshrc (or ~/.bashrc)

fcd() {{
    local dir
    dir=$(fuzzy-cd pick \"$1\")
    [ -n \"$dir\" ] && cd \"$dir\"
}}
alias cd='fcd'
"
    );
}

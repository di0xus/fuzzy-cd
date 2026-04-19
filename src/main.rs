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

fn get_best_match(conn: &Connection, query: &str) -> Option<String> {
    let matcher = SkimMatcherV2::default();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();

    let mut best_score: f64 = 0.0;
    let mut best_path: Option<String> = None;

    // Search history
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
        if let Some(score) = matcher.fuzzy_match(&path, query) {
            let recency = (now - last_visited).min(86400.0 * 30.0) / (86400.0 * 30.0);
            let final_score = score as f64 * visits as f64 * (1.0 + recency);
            if final_score > best_score {
                best_score = final_score;
                best_path = Some(path);
            }
        }
    }

    // Search indexed paths
    let mut stmt = conn.prepare("SELECT path FROM indexed_paths").ok()?;
    let rows = stmt.query_map([], |row| Ok(row.get::<_, String>(0)?)).ok()?;

    for row in rows.flatten() {
        let path = row;
        if let Some(score) = matcher.fuzzy_match(&path, query) {
            // 0.5 weight for indexed paths (less trusted than history)
            let final_score = score as f64 * 0.5;
            if final_score > best_score {
                best_score = final_score;
                best_path = Some(path);
            }
        }
    }

    // Require a minimum score to avoid garbage matches
    if best_score < 10.0 {
        return None;
    }

    best_path
}

fn index_directories(conn: &Connection, root: &Path) -> rusqlite::Result<usize> {
    let mut count = 0;

    fn walk_dir(conn: &Connection, dir: &Path, count: &mut usize) -> rusqlite::Result<()> {
        let entries = fs::read_dir(dir).map_err(|_| rusqlite::Error::InvalidQuery)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                let skip_dirs = [
                    "Library", "Music", "Movies", "Pictures",
                    "Documents", "Applications", "Desktop",
                    "node_modules", "target", ".git",
                ];
                if name_str.starts_with('.') || skip_dirs.contains(&name_str.as_ref()) {
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
    let mut stmt = conn.prepare("SELECT path, visits FROM history ORDER BY visits DESC LIMIT 20")?;
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

fn main() {
    let args: Vec<String> = env::args().collect();

    let data_dir = get_data_dir();
    fs::create_dir_all(&data_dir).expect("Failed to create data directory");

    let db_path = get_history_db_path();
    let conn = Connection::open(&db_path).expect("Failed to open database");
    init_db(&conn).expect("Failed to initialize database");

    let home = dirs::home_dir().expect("Could not find home directory");

    if args.len() == 1 {
        print_help();
        return;
    }

    match args[1].as_str() {
        "pick" => {
            let query = args.get(2).map(|s| s.as_str()).unwrap_or("");
            if let Some(path) = get_best_match(&conn, query) {
                println!("{}", path);
                touch_path(&conn, &path).ok();
            }
        }
        "add" => {
            if args.len() < 3 {
                eprintln!("Usage: fuzzy-cd add <path>");
                process::exit(1);
            }
            let path = expand_home(&args[2]);
            if path.exists() && path.is_dir() {
                if touch_path(&conn, &path.to_string_lossy()).is_ok() {
                    println!("Added: {}", path.display());
                }
            } else {
                eprintln!("Not a valid directory: {}", path.display());
                process::exit(1);
            }
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
            let count = index_directories(&conn, &home).unwrap_or(0);
            println!("{} directories indexed.", count);
        }
        "--help" | "-h" | "help" => {
            print_help();
        }
        "init" => {
            print_init_script();
        }
        _ => {
            // Treat as query
            if let Some(path) = get_best_match(&conn, &args[1]) {
                println!("{}", path);
                touch_path(&conn, &path).ok();
            }
        }
    }
}

fn print_help() {
    println!(
        "fuzzy-cd — fuzzy directory navigation

Usage:
    fuzzy-cd [query]       Print best matching path
    fuzzy-cd pick <query>  Same, for shell integration
    fuzzy-cd add <path>    Add path to history
    fuzzy-cd history       Show top visited directories
    fuzzy-cd --clear-history   Clear history
    fuzzy-cd --reindex     Rebuild directory index
    fuzzy-cd init           Print fish shell function
    fuzzy-cd --help         Show this help
"
    );
}

fn print_init_script() {
    println!(
        "# Add to ~/.config/fish/config.fish

function fcd
    if test (count $argv) -eq 1
        if test \"$argv[1]\" = \"..\" -o \"$argv[1]\" = \".\" -o \"$argv[1]\" = \"~\" -o \"$argv[1]\" = \"~/\" -o \"$argv[1]\" = \"-\"
            builtin cd $argv[1]
            return
        end
    end
    if test (count $argv) -ge 1
        and string match -qr \"^/\" -- \"$argv[1]\"
        builtin cd $argv
        return
    end
    set -l dir (fuzzy-cd pick $argv)
    if test -n \"$dir\"
        builtin cd $dir
        return
    end
    if test (count $argv) -ge 1
        builtin cd $argv
    end
end
abbr --add cd fcd
"
    );
}

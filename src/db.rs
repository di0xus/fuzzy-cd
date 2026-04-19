use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use directories::ProjectDirs;
use rusqlite::{Connection, OptionalExtension, params};

pub const APP_NAME: &str = "fuzzy-cd";
pub const DB_NAME: &str = "fuzzy-cd.db";
pub const SCHEMA_VERSION: i64 = 2;

pub struct Database {
    pub conn: Connection,
}

pub struct HistoryRow {
    pub path: String,
    pub visits: i32,
    pub last_visited: f64,
    pub is_git_repo: bool,
}

pub fn default_data_dir() -> PathBuf {
    ProjectDirs::from("", "", APP_NAME)
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| home_dir().join(".fuzzy-cd"))
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        home_dir()
    } else if let Some(suffix) = path.strip_prefix("~/") {
        home_dir().join(suffix)
    } else {
        PathBuf::from(path)
    }
}

pub fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

impl Database {
    pub fn open() -> rusqlite::Result<Self> {
        Self::open_at(&default_data_dir().join(DB_NAME))
    }

    pub fn open_at(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        conn.pragma_update(None, "synchronous", "NORMAL").ok();
        let db = Database { conn };
        db.migrate()?;
        Ok(db)
    }

    pub fn in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Database { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;

        let version: i64 = self
            .conn
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);

        if version < 1 {
            // Create tables that may be brand-new. Indexes on columns that
            // might not exist on legacy DBs are deferred to the v<2 step.
            self.conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS history (
                    id INTEGER PRIMARY KEY,
                    path TEXT UNIQUE NOT NULL,
                    basename TEXT NOT NULL DEFAULT '',
                    visits INTEGER NOT NULL DEFAULT 1,
                    last_visited REAL NOT NULL,
                    created_at REAL NOT NULL,
                    is_git_repo INTEGER NOT NULL DEFAULT 0
                );
                CREATE INDEX IF NOT EXISTS idx_history_visits ON history(visits DESC);
                CREATE INDEX IF NOT EXISTS idx_history_last ON history(last_visited DESC);

                CREATE TABLE IF NOT EXISTS bookmarks (
                    id INTEGER PRIMARY KEY,
                    alias TEXT UNIQUE NOT NULL,
                    path TEXT NOT NULL,
                    created_at REAL NOT NULL
                );

                CREATE TABLE IF NOT EXISTS dir_index (
                    id INTEGER PRIMARY KEY,
                    path TEXT UNIQUE NOT NULL,
                    basename TEXT NOT NULL,
                    parent TEXT NOT NULL,
                    indexed_at REAL NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_dir_index_basename ON dir_index(basename);",
            )?;
        }

        if version < 2 {
            // Ensure columns exist on older DBs (pre-lib schema).
            let cols: Vec<String> = self
                .conn
                .prepare("PRAGMA table_info(history)")?
                .query_map([], |r| r.get::<_, String>(1))?
                .flatten()
                .collect();
            if !cols.iter().any(|c| c == "basename") {
                self.conn.execute(
                    "ALTER TABLE history ADD COLUMN basename TEXT NOT NULL DEFAULT ''",
                    [],
                )?;
                // Backfill in Rust — SQLite has no reverse()/basename builtin.
                let paths: Vec<(i64, String)> = self
                    .conn
                    .prepare("SELECT id, path FROM history")?
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .flatten()
                    .collect();
                for (id, path) in paths {
                    let base = basename_of(&path);
                    self.conn.execute(
                        "UPDATE history SET basename = ?1 WHERE id = ?2",
                        params![base, id],
                    )?;
                }
            }
            if !cols.iter().any(|c| c == "is_git_repo") {
                self.conn.execute(
                    "ALTER TABLE history ADD COLUMN is_git_repo INTEGER NOT NULL DEFAULT 0",
                    [],
                )?;
            }
            self.conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_history_basename ON history(basename)",
                [],
            )?;
        }

        self.conn.execute(
            "INSERT OR REPLACE INTO meta(key, value) VALUES('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
    }

    pub fn record_visit(&self, path: &str) -> rusqlite::Result<()> {
        let now = now_secs();
        let basename = basename_of(path);
        let is_git = Path::new(path).join(".git").exists() as i64;
        self.conn.execute(
            "INSERT INTO history (path, basename, visits, last_visited, created_at, is_git_repo)
             VALUES (?1, ?2, 1, ?3, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET
                visits = visits + 1,
                last_visited = ?3,
                is_git_repo = ?4",
            params![path, basename, now, is_git],
        )?;
        Ok(())
    }

    pub fn forget(&self, path: &str) -> rusqlite::Result<usize> {
        self.conn
            .execute("DELETE FROM history WHERE path = ?1", params![path])
    }

    pub fn clear_history(&self) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM history", [])?;
        Ok(())
    }

    pub fn prune_stale(&self) -> rusqlite::Result<usize> {
        let paths: Vec<String> = self
            .conn
            .prepare("SELECT path FROM history")?
            .query_map([], |r| r.get::<_, String>(0))?
            .flatten()
            .collect();
        let mut removed = 0;
        for p in paths {
            if !Path::new(&p).is_dir() {
                removed += self.forget(&p)?;
            }
        }
        let idx_paths: Vec<String> = self
            .conn
            .prepare("SELECT path FROM dir_index")?
            .query_map([], |r| r.get::<_, String>(0))?
            .flatten()
            .collect();
        for p in idx_paths {
            if !Path::new(&p).is_dir() {
                self.conn
                    .execute("DELETE FROM dir_index WHERE path = ?1", params![p])?;
            }
        }
        Ok(removed)
    }

    pub fn history_rows(&self) -> rusqlite::Result<Vec<HistoryRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, visits, last_visited, is_git_repo FROM history")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(HistoryRow {
                    path: r.get(0)?,
                    visits: r.get(1)?,
                    last_visited: r.get(2)?,
                    is_git_repo: r.get::<_, i64>(3)? != 0,
                })
            })?
            .flatten()
            .collect();
        Ok(rows)
    }

    pub fn top(&self, limit: usize) -> rusqlite::Result<Vec<HistoryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, visits, last_visited, is_git_repo FROM history
             ORDER BY visits DESC, last_visited DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                Ok(HistoryRow {
                    path: r.get(0)?,
                    visits: r.get(1)?,
                    last_visited: r.get(2)?,
                    is_git_repo: r.get::<_, i64>(3)? != 0,
                })
            })?
            .flatten()
            .collect();
        Ok(rows)
    }

    pub fn recent(&self, limit: usize) -> rusqlite::Result<Vec<HistoryRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, visits, last_visited, is_git_repo FROM history
             ORDER BY last_visited DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                Ok(HistoryRow {
                    path: r.get(0)?,
                    visits: r.get(1)?,
                    last_visited: r.get(2)?,
                    is_git_repo: r.get::<_, i64>(3)? != 0,
                })
            })?
            .flatten()
            .collect();
        Ok(rows)
    }

    pub fn bookmark_exact(&self, alias: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT path FROM bookmarks WHERE alias = ?1",
                params![alias],
                |r| r.get(0),
            )
            .optional()
    }

    pub fn bookmarks(&self) -> rusqlite::Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT alias, path FROM bookmarks ORDER BY alias")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .flatten()
            .collect();
        Ok(rows)
    }

    pub fn set_bookmark(&self, alias: &str, path: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO bookmarks(alias, path, created_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(alias) DO UPDATE SET path = ?2",
            params![alias, path, now_secs()],
        )?;
        Ok(())
    }

    pub fn remove_bookmark(&self, alias: &str) -> rusqlite::Result<usize> {
        self.conn
            .execute("DELETE FROM bookmarks WHERE alias = ?1", params![alias])
    }

    pub fn upsert_indexed_dir(&self, path: &str) -> rusqlite::Result<()> {
        let basename = basename_of(path);
        let parent = Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.conn.execute(
            "INSERT INTO dir_index(path, basename, parent, indexed_at)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET basename = ?2, parent = ?3, indexed_at = ?4",
            params![path, basename, parent, now_secs()],
        )?;
        Ok(())
    }

    pub fn index_rows(&self) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT path FROM dir_index")?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .flatten()
            .collect();
        Ok(rows)
    }

    pub fn counts(&self) -> rusqlite::Result<DbCounts> {
        let total = self
            .conn
            .query_row("SELECT COUNT(*) FROM history", [], |r| r.get::<_, i64>(0))?;
        let total_visits =
            self.conn
                .query_row("SELECT COALESCE(SUM(visits), 0) FROM history", [], |r| {
                    r.get::<_, i64>(0)
                })?;
        let bookmarks = self
            .conn
            .query_row("SELECT COUNT(*) FROM bookmarks", [], |r| r.get::<_, i64>(0))?;
        let indexed = self
            .conn
            .query_row("SELECT COUNT(*) FROM dir_index", [], |r| r.get::<_, i64>(0))?;
        let top_path: Option<String> = self
            .conn
            .query_row(
                "SELECT path FROM history ORDER BY visits DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(DbCounts {
            total,
            total_visits,
            bookmarks,
            indexed,
            top_path,
        })
    }
}

pub struct DbCounts {
    pub total: i64,
    pub total_visits: i64,
    pub bookmarks: i64,
    pub indexed: i64,
    pub top_path: Option<String>,
}

pub fn basename_of(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_and_migrate() {
        let db = Database::in_memory().unwrap();
        let c = db.counts().unwrap();
        assert_eq!(c.total, 0);
    }

    #[test]
    fn migrates_legacy_v02_schema() {
        // Simulate the pre-lib v0.2 layout: history without basename/is_git_repo.
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("legacy.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE history (
                    id INTEGER PRIMARY KEY,
                    path TEXT UNIQUE NOT NULL,
                    visits INTEGER DEFAULT 1,
                    last_visited REAL NOT NULL,
                    created_at REAL NOT NULL
                );
                INSERT INTO history(path, visits, last_visited, created_at)
                VALUES('/tmp/foo', 3, 1000, 500);",
            )
            .unwrap();
        }
        let db = Database::open_at(&path).expect("migrate should succeed");
        let rows = db.history_rows().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "/tmp/foo");
        assert_eq!(rows[0].visits, 3);
        assert!(!rows[0].is_git_repo);
    }

    #[test]
    fn record_and_read_visit() {
        let db = Database::in_memory().unwrap();
        db.record_visit("/tmp/example").unwrap();
        db.record_visit("/tmp/example").unwrap();
        let rows = db.history_rows().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].visits, 2);
        assert_eq!(rows[0].path, "/tmp/example");
    }

    #[test]
    fn bookmark_roundtrip() {
        let db = Database::in_memory().unwrap();
        db.set_bookmark("proj", "/tmp/p").unwrap();
        assert_eq!(
            db.bookmark_exact("proj").unwrap().as_deref(),
            Some("/tmp/p")
        );
        db.remove_bookmark("proj").unwrap();
        assert!(db.bookmark_exact("proj").unwrap().is_none());
    }

    #[test]
    fn expand_home_passthrough_for_abs_paths() {
        // Tilde behavior exercised end-to-end in integration tests to avoid
        // mutating HOME from parallel unit tests.
        assert_eq!(expand_home("/abs"), PathBuf::from("/abs"));
        assert_eq!(expand_home("rel"), PathBuf::from("rel"));
    }

    #[test]
    fn basename_lowercased() {
        assert_eq!(basename_of("/Users/Foo/Bar"), "bar");
    }
}

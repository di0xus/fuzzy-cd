use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::score::basename_lower;

use directories::ProjectDirs;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};

pub const APP_NAME: &str = "hop";
pub const DB_NAME: &str = "hop.db";
pub const LEGACY_APP_NAME: &str = "fuzzy-cd";
pub const LEGACY_DB_NAME: &str = "fuzzy-cd.db";
pub const SCHEMA_VERSION: i64 = 3;

pub struct Database {
    conn: Connection,
}

#[derive(Clone, Debug)]
pub struct HistoryRow {
    pub path: String,
    pub visits: i32,
    pub last_visited: f64,
    pub is_git_repo: bool,
}

pub fn default_data_dir() -> PathBuf {
    ProjectDirs::from("", "", APP_NAME)
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| home_dir().join(".hop"))
}

pub fn legacy_data_dir() -> PathBuf {
    ProjectDirs::from("", "", LEGACY_APP_NAME)
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| home_dir().join(".fuzzy-cd"))
}

/// If a legacy fuzzy-cd DB exists and the new one does not, copy it so the
/// user's history survives the rename. One-shot; no-op otherwise.
pub fn migrate_legacy_data_dir() {
    let new_db = default_data_dir().join(DB_NAME);
    if new_db.exists() {
        return;
    }
    let legacy_db = legacy_data_dir().join(LEGACY_DB_NAME);
    if !legacy_db.exists() {
        return;
    }
    if let Some(parent) = new_db.parent() {
        let _ = std::fs::create_dir_all(parent);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
        }
    }
    let _ = std::fs::copy(&legacy_db, &new_db);
    // Best-effort copy of WAL/SHM siblings so we don't lose uncommitted rows.
    for ext in ["-wal", "-shm"] {
        let src = legacy_db.with_file_name(format!("{}{}", LEGACY_DB_NAME, ext));
        let dst = new_db.with_file_name(format!("{}{}", DB_NAME, ext));
        if src.exists() {
            let _ = std::fs::copy(src, dst);
        }
    }
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

/// Resolve a path to its canonical form, following symlinks.
/// Returns the canonical path as a string, or None if resolution fails.
pub fn canonicalize_path(path: &str) -> Option<String> {
    let p = Path::new(path);
    p.canonicalize()
        .ok()
        .map(|c| c.to_string_lossy().into_owned())
}

pub fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

impl Database {
    pub fn open() -> rusqlite::Result<Self> {
        migrate_legacy_data_dir();
        Self::open_at(&default_data_dir().join(DB_NAME))
    }

    pub fn open_at(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
            #[cfg(unix)]
            std::fs::set_permissions(parent, PermissionsExt::from_mode(0o700)).ok();
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
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
                    let base = basename_lower(&path);
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
            // Index parent column for efficient child-lookup queries.
            self.conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_dir_index_parent ON dir_index(parent)",
                [],
            )?;
        }

        if version < 3 {
            // Add description column to bookmarks table.
            self.conn.execute(
                "ALTER TABLE bookmarks ADD COLUMN description TEXT NOT NULL DEFAULT ''",
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
        // Resolve symlinks to canonical path so /link/to/project and
        // /real/project share one history row.
        let canonical = canonicalize_path(path).unwrap_or_else(|| path.to_owned());
        let basename = basename_lower(&canonical);
        let is_git = Path::new(&canonical).join(".git").exists() as i64;
        self.conn.execute(
            "INSERT INTO history (path, basename, visits, last_visited, created_at, is_git_repo)
             VALUES (?1, ?2, 1, ?3, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET
                visits = visits + 1,
                last_visited = ?3,
                is_git_repo = ?4",
            params![canonical, basename, now, is_git],
        )?;
        Ok(())
    }

    pub fn forget(&self, path: &str) -> rusqlite::Result<usize> {
        self.conn
            .execute("DELETE FROM history WHERE path = ?1", params![path])
    }

    pub fn clear_history(&self) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM history", [])?;
        self.conn.execute("DELETE FROM dir_index", [])?;
        Ok(())
    }

    pub fn prune_stale(&self) -> rusqlite::Result<usize> {
        self.prune_stale_batch(256, |_, _| {})
    }

    /// Remove stale history/index entries, processing in batches of `batch_size`.
    /// Calls `progress` with (processed, total) after each batch.
    pub fn prune_stale_batch<F>(&self, batch_size: usize, progress: F) -> rusqlite::Result<usize>
    where
        F: Fn(usize, usize),
    {
        // History stale — parallel is_dir() check across all paths, then sequential removal.
        let history_paths: Vec<String> = self
            .conn
            .prepare("SELECT path FROM history")?
            .query_map([], |r| r.get::<_, String>(0))?
            .flatten()
            .collect();

        let stale_history: Vec<String> = history_paths
            .par_iter()
            .filter(|p| !Path::new(p).is_dir())
            .cloned()
            .collect();

        let mut removed = 0;
        for chunk in stale_history.chunks(batch_size) {
            for p in chunk {
                removed += self.forget(p)?;
            }
            progress(chunk.len(), stale_history.len());
        }
        progress(removed, stale_history.len());

        // Index stale — parallel is_dir() check, then sequential removal.
        let index_paths: Vec<String> = self
            .conn
            .prepare("SELECT path FROM dir_index")?
            .query_map([], |r| r.get::<_, String>(0))?
            .flatten()
            .collect();

        let stale_index: Vec<String> = index_paths
            .par_iter()
            .filter(|p| !Path::new(p).is_dir())
            .cloned()
            .collect();

        for chunk in stale_index.chunks(batch_size) {
            for p in chunk {
                self.conn
                    .execute("DELETE FROM dir_index WHERE path = ?1", params![p])?;
            }
            progress(chunk.len(), stale_index.len());
        }
        progress(stale_index.len(), stale_index.len());
        Ok(removed)
    }

    /// Returns paths that would be removed by prune_stale, without deleting anything.
    pub fn prune_stale_dry_run(&self) -> rusqlite::Result<(Vec<String>, Vec<String>)> {
        let history_stale: Vec<String> = self
            .conn
            .prepare("SELECT path FROM history")?
            .query_map([], |r| r.get::<_, String>(0))?
            .flatten()
            .filter(|p| !Path::new(p).is_dir())
            .collect();
        let index_stale: Vec<String> = self
            .conn
            .prepare("SELECT path FROM dir_index")?
            .query_map([], |r| r.get::<_, String>(0))?
            .flatten()
            .filter(|p| !Path::new(p).is_dir())
            .collect();
        Ok((history_stale, index_stale))
    }

    /// Auto-prune: remove history rows with visits=1 AND last_visited > 90 days ago.
    /// Also removes stale (deleted dirs) entries. Skips paths whose basename matches skip_dirs.
    /// Returns the number of rows removed.
    pub fn prune_auto(&self, skip_dirs: &[String]) -> rusqlite::Result<usize> {
        let now = now_secs();
        let ninety_days = 90.0 * 86_400.0;
        let cutoff = now - ninety_days;

        // Load single-visit+old entries for age-based pruning
        let mut stmt1 = self
            .conn
            .prepare("SELECT rowid, path FROM history WHERE visits = 1 AND last_visited < ?1")?;
        let old_single: Vec<(i64, String)> = stmt1
            .query_map(params![cutoff], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt1);

        // Load all rows for stale-path check (deleted directories)
        let mut stmt2 = self.conn.prepare("SELECT rowid, path FROM history")?;
        let stale: Vec<(i64, String)> = stmt2
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt2);

        let mut to_remove: Vec<i64> = Vec::new();

        // Age-based: single visit, old, not in skip_dirs
        for (rowid, path) in old_single {
            if let Some(name) = Path::new(&path).file_name() {
                let name_str = name.to_string_lossy();
                if skip_dirs.iter().any(|d| d == name_str.as_ref()) {
                    continue;
                }
            }
            to_remove.push(rowid);
        }

        // Stale: path no longer exists
        for (rowid, path) in stale {
            if !Path::new(&path).is_dir() {
                to_remove.push(rowid);
            }
        }

        if to_remove.is_empty() {
            return Ok(0);
        }

        to_remove.sort_unstable();
        to_remove.dedup();

        let mut removed = 0;
        for chunk in to_remove.chunks(100) {
            let placeholders: Vec<String> = chunk.iter().map(|_| "?".to_string()).collect();
            let sql = format!(
                "DELETE FROM history WHERE rowid IN ({})",
                placeholders.join(",")
            );
            removed += self.conn.execute(&sql, params_from_iter(chunk))?;
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

    /// Returns rows filtered to only include paths that still exist on disk.
    pub fn filter_live_rows(rows: Vec<HistoryRow>) -> Vec<HistoryRow> {
        rows.into_iter()
            .filter(|r| Path::new(&r.path).is_dir())
            .collect()
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

    pub fn bookmarks(&self) -> rusqlite::Result<Vec<(String, String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT alias, path, description FROM bookmarks ORDER BY alias")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .flatten()
            .collect();
        Ok(rows)
    }

    pub fn set_bookmark(&self, alias: &str, path: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO bookmarks(alias, path, created_at, description) VALUES(?1, ?2, ?3, '')
             ON CONFLICT(alias) DO UPDATE SET path = ?2",
            params![alias, path, now_secs()],
        )?;
        Ok(())
    }

    pub fn remove_bookmark(&self, alias: &str) -> rusqlite::Result<usize> {
        self.conn
            .execute("DELETE FROM bookmarks WHERE alias = ?1", params![alias])
    }

    /// Edit bookmark fields. At least one of new_alias, new_path, or new_description
    /// must be Some(...). Returns the number of rows updated (0 or 1).
    pub fn edit_bookmark(
        &self,
        alias: &str,
        new_alias: Option<&str>,
        new_path: Option<&str>,
        new_description: Option<&str>,
    ) -> rusqlite::Result<usize> {
        let mut set_clauses: Vec<&str> = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(a) = new_alias {
            set_clauses.push("alias = ?");
            params.push(Box::new(a.to_string()));
        }
        if let Some(p) = new_path {
            set_clauses.push("path = ?");
            params.push(Box::new(p.to_string()));
        }
        if let Some(d) = new_description {
            set_clauses.push("description = ?");
            params.push(Box::new(d.to_string()));
        }

        if set_clauses.is_empty() {
            return Ok(0);
        }

        params.push(Box::new(alias.to_string()));

        let sql = format!(
            "UPDATE bookmarks SET {} WHERE alias = ?",
            set_clauses.join(", ")
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        stmt.execute(params_refs.as_slice())
    }

    pub fn upsert_indexed_dir(&self, path: &str) -> rusqlite::Result<()> {
        let (basename, parent) = Self::indexed_dir_params(path);
        self.conn.execute(
            "INSERT INTO dir_index(path, basename, parent, indexed_at)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET basename = ?2, parent = ?3, indexed_at = ?4",
            params![path, basename, parent, now_secs()],
        )?;
        Ok(())
    }

    /// Extract (basename, parent) for dir_index. Both upsert methods share this logic.
    fn indexed_dir_params(path: &str) -> (String, String) {
        let basename = basename_lower(path);
        let parent = Path::new(path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        (basename, parent)
    }

    pub fn batch_upsert_indexed_dirs(&self, paths: &[String]) -> rusqlite::Result<()> {
        let now = now_secs();
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        for path in paths {
            let (basename, parent) = Self::indexed_dir_params(path);
            self.conn.execute(
                "INSERT INTO dir_index(path, basename, parent, indexed_at)
                 VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(path) DO UPDATE SET basename = ?2, parent = ?3, indexed_at = ?4",
                params![path, basename, parent, now],
            )?;
        }
        self.conn.execute("COMMIT", [])?;
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
        self.conn.query_row(
            r#"
            WITH
                hist_total  AS (SELECT COUNT(*)          AS n FROM history),
                hist_visits AS (SELECT COALESCE(SUM(visits), 0) AS n FROM history),
                bm_total    AS (SELECT COUNT(*)          AS n FROM bookmarks),
                idx_total   AS (SELECT COUNT(*)          AS n FROM dir_index),
                top_row     AS (
                    SELECT path FROM history
                    WHERE visits = (SELECT MAX(visits) FROM history)
                    LIMIT 1
                )
            SELECT
                (SELECT n FROM hist_total)  AS total,
                (SELECT n FROM hist_visits) AS total_visits,
                (SELECT n FROM bm_total)    AS bookmarks,
                (SELECT n FROM idx_total)   AS indexed,
                (SELECT path FROM top_row)  AS top_path
            "#,
            [],
            |r| {
                Ok(DbCounts {
                    total: r.get(0)?,
                    total_visits: r.get(1)?,
                    bookmarks: r.get(2)?,
                    indexed: r.get(3)?,
                    top_path: r.get(4)?,
                })
            },
        )
    }

    /// Returns the schema version stored in the meta table, or 0 if not set.
    pub fn schema_version(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map(|opt| opt.unwrap_or(0))
    }
}

pub struct DbCounts {
    pub total: i64,
    pub total_visits: i64,
    pub bookmarks: i64,
    pub indexed: i64,
    pub top_path: Option<String>,
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
        assert_eq!(basename_lower("/Users/Foo/Bar"), "bar");
    }

    #[test]
    fn record_visit_with_unicode_and_special_chars() {
        let db = Database::in_memory().unwrap();
        // Spaces
        db.record_visit("/tmp/a b").unwrap();
        let rows = db.history_rows().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, "/tmp/a b");

        // Emoji
        db.record_visit("/tmp/🎉").unwrap();
        let rows = db.history_rows().unwrap();
        assert_eq!(rows.len(), 2);

        // CJK
        db.record_visit("/tmp/日本語").unwrap();
        let rows = db.history_rows().unwrap();
        assert_eq!(rows.len(), 3);

        // Incrementing visits on unicode path
        db.record_visit("/tmp/日本語").unwrap();
        let rows = db.history_rows().unwrap();
        let japanese = rows.iter().find(|r| r.path.contains("日本語")).unwrap();
        assert_eq!(japanese.visits, 2);
    }

    #[test]
    fn canonicalize_path_resolves_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = tmp.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();

        // On non-Unix this test is a no-op
        #[cfg(not(unix))]
        let _ = link;

        #[cfg(unix)]
        {
            let canonical = canonicalize_path(link.to_str().unwrap()).unwrap();
            // On macOS /var/folders is a symlink to /private/var/folders
            let real_canonical = canonicalize_path(real.to_str().unwrap()).unwrap();
            assert_eq!(canonical, real_canonical);
        }
    }

    #[test]
    fn prune_stale_dry_run_returns_stale_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_at(&tmp.path().join("hop.db")).unwrap();
        let alive = tmp.path().join("alive");
        let dead = tmp.path().join("dead");
        std::fs::create_dir(&alive).unwrap();
        std::fs::create_dir(&dead).unwrap();
        // record_visit canonicalizes before storing; pass canonical forms
        let alive_s = canonicalize_path(alive.to_str().unwrap()).unwrap();
        let dead_s = canonicalize_path(dead.to_str().unwrap()).unwrap();
        db.record_visit(&alive_s).unwrap();
        db.record_visit(&dead_s).unwrap();
        std::fs::remove_dir(&dead).unwrap();

        let (hist, idx) = db.prune_stale_dry_run().unwrap();
        assert!(hist.contains(&dead_s), "hist={hist:?}, dead_s={dead_s}",);
        assert!(!hist.contains(&alive_s));
        // alive/dead are in history, index is empty
        assert!(idx.is_empty());
    }

    #[test]
    fn prune_stale_batch_empty_db_removes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_at(&tmp.path().join("hop.db")).unwrap();
        let removed = db.prune_stale_batch(10, |_, _| {}).unwrap();
        assert_eq!(removed, 0, "empty DB should remove nothing");
    }

    #[test]
    fn prune_stale_batch_all_stale_removes_all() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_at(&tmp.path().join("hop.db")).unwrap();

        // Create two directories then delete them
        let dir1 = tmp.path().join("stale1");
        let dir2 = tmp.path().join("stale2");
        std::fs::create_dir(&dir1).unwrap();
        std::fs::create_dir(&dir2).unwrap();

        let s1 = canonicalize_path(dir1.to_str().unwrap()).unwrap();
        let s2 = canonicalize_path(dir2.to_str().unwrap()).unwrap();
        db.record_visit(&s1).unwrap();
        db.record_visit(&s2).unwrap();

        // Delete both directories
        std::fs::remove_dir(&dir1).unwrap();
        std::fs::remove_dir(&dir2).unwrap();

        let removed = db.prune_stale_batch(10, |_, _| {}).unwrap();
        assert_eq!(removed, 2, "all stale entries should be removed");
        assert_eq!(db.history_rows().unwrap().len(), 0);
    }

    #[test]
    fn prune_stale_batch_none_stale_removes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_at(&tmp.path().join("hop.db")).unwrap();

        // Create and visit valid directories
        let dir1 = tmp.path().join("alive1");
        let dir2 = tmp.path().join("alive2");
        std::fs::create_dir(&dir1).unwrap();
        std::fs::create_dir(&dir2).unwrap();

        let d1 = canonicalize_path(dir1.to_str().unwrap()).unwrap();
        let d2 = canonicalize_path(dir2.to_str().unwrap()).unwrap();
        db.record_visit(&d1).unwrap();
        db.record_visit(&d2).unwrap();

        let removed = db.prune_stale_batch(10, |_, _| {}).unwrap();
        assert_eq!(removed, 0, "no stale entries should be removed");
        assert_eq!(db.history_rows().unwrap().len(), 2);
    }

    #[test]
    fn prune_stale_batch_mix_stale_and_fresh() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_at(&tmp.path().join("hop.db")).unwrap();

        // Create one alive and one dead directory
        let alive = tmp.path().join("alive");
        let dead = tmp.path().join("dead");
        std::fs::create_dir(&alive).unwrap();
        std::fs::create_dir(&dead).unwrap();

        let alive_s = canonicalize_path(alive.to_str().unwrap()).unwrap();
        let dead_s = canonicalize_path(dead.to_str().unwrap()).unwrap();
        db.record_visit(&alive_s).unwrap();
        db.record_visit(&dead_s).unwrap();

        // Delete the dead directory
        std::fs::remove_dir(&dead).unwrap();

        let removed = db.prune_stale_batch(10, |_, _| {}).unwrap();
        assert_eq!(removed, 1, "only stale entry should be removed");
        assert_eq!(db.history_rows().unwrap().len(), 1);
        assert_eq!(db.history_rows().unwrap()[0].path, alive_s);
    }

    #[test]
    fn counts_after_inserting_history() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_at(&tmp.path().join("hop.db")).unwrap();

        // Empty DB
        let c = db.counts().unwrap();
        assert_eq!(c.total, 0);
        assert_eq!(c.total_visits, 0);

        // Insert a path once
        let dir = tmp.path().join("proj");
        std::fs::create_dir(&dir).unwrap();
        let dir_s = canonicalize_path(dir.to_str().unwrap()).unwrap();
        db.record_visit(&dir_s).unwrap();

        let c = db.counts().unwrap();
        assert_eq!(c.total, 1, "total should be 1");
        assert_eq!(c.total_visits, 1, "total_visits should be 1");

        // Visit same path again
        db.record_visit(&dir_s).unwrap();

        let c = db.counts().unwrap();
        assert_eq!(c.total, 1, "total should still be 1 (same path)");
        assert_eq!(c.total_visits, 2, "total_visits should be 2");

        // Add another path
        let dir2 = tmp.path().join("proj2");
        std::fs::create_dir(&dir2).unwrap();
        let dir2_s = canonicalize_path(dir2.to_str().unwrap()).unwrap();
        db.record_visit(&dir2_s).unwrap();

        let c = db.counts().unwrap();
        assert_eq!(c.total, 2, "total should be 2");
        assert_eq!(c.total_visits, 3, "total_visits should be 3 (2 + 1)");
    }
}

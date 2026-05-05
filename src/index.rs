use rayon::prelude::*;
use rusqlite::Error;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::db::Database;

const BATCH_SIZE: usize = 500;

#[derive(Default)]
pub struct IndexStats {
    pub scanned: usize,
    pub inserted: usize,
}

pub fn reindex(db: &Database, cfg: &Config, dry_run: bool) -> Result<IndexStats, Error> {
    // Collect all directories first
    let mut all_dirs = Vec::new();
    for root in &cfg.index_roots {
        walk_collect(root, cfg, 0, &mut all_dirs);
    }

    let paths: Vec<String> = if dry_run {
        // Dry-run: skip directories that would be upserted (no DB writes)
        all_dirs
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    } else {
        // Parallel conversion to strings using rayon, then sequential batch insert
        let paths: Vec<String> = all_dirs
            .par_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();

        // Sequential batch insert
        for chunk in paths.chunks(BATCH_SIZE) {
            db.batch_upsert_indexed_dirs(chunk)?;
        }
        paths
    };

    Ok(IndexStats {
        scanned: paths.len(),
        inserted: paths.len(),
    })
}

fn walk_collect(dir: &Path, cfg: &Config, depth: usize, dirs: &mut Vec<PathBuf>) {
    if depth > cfg.max_depth {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().map(|n| n.to_string_lossy().into_owned()) {
            Some(n) => n,
            None => continue,
        };
        let ft = match entry.file_type() {
            Ok(f) => f,
            Err(_) => continue,
        };
        if ft.is_symlink() || !ft.is_dir() {
            continue;
        }
        if cfg.should_skip(&name) {
            continue;
        }
        dirs.push(path.clone());
        walk_collect(&path, cfg, depth + 1, dirs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn reindex_finds_subdirs_skips_dotdirs() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("a")).unwrap();
        fs::create_dir(tmp.path().join("a/b")).unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::create_dir(tmp.path().join("node_modules")).unwrap();

        let db = Database::in_memory().unwrap();
        let cfg = Config {
            index_roots: vec![tmp.path().to_path_buf()],
            skip_dirs: Config::default_skip_dirs(),
            max_depth: 5,
            min_score: 20,
            auto_prune_on_startup: false,
        };
        let stats = reindex(&db, &cfg, false).unwrap();
        assert_eq!(stats.scanned, 2, "should only scan a and a/b");
        assert_eq!(stats.inserted, 2);
        let rows = db.index_rows().unwrap();
        assert_eq!(rows.len(), 2);
    }
}

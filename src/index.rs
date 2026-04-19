use std::fs;
use std::path::Path;

use crate::config::Config;
use crate::db::Database;

pub struct IndexStats {
    pub scanned: usize,
    pub inserted: usize,
}

pub fn reindex(db: &Database, cfg: &Config) -> IndexStats {
    let mut stats = IndexStats {
        scanned: 0,
        inserted: 0,
    };
    for root in &cfg.index_roots {
        walk(db, root, cfg, 0, &mut stats);
    }
    stats
}

fn walk(db: &Database, dir: &Path, cfg: &Config, depth: usize, stats: &mut IndexStats) {
    if depth > cfg.max_depth {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
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
        stats.scanned += 1;
        if db.upsert_indexed_dir(&path.to_string_lossy()).is_ok() {
            stats.inserted += 1;
        }
        walk(db, &path, cfg, depth + 1, stats);
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
        };
        let stats = reindex(&db, &cfg);
        assert_eq!(stats.scanned, 2, "should only scan a and a/b");
        assert_eq!(stats.inserted, 2);
        let rows = db.index_rows().unwrap();
        assert_eq!(rows.len(), 2);
    }
}

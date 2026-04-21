use std::collections::HashSet;
use std::path::Path;

use crate::db::{Database, SCHEMA_VERSION};

pub struct Report {
    pub ok: bool,
    pub lines: Vec<String>,
}

pub fn run(db: &Database) -> Report {
    let mut lines = Vec::new();
    let mut ok = true;

    let counts = match db.counts() {
        Ok(c) => c,
        Err(e) => {
            lines.push(format!("✗ db.counts error: {}", e));
            return Report { ok: false, lines };
        }
    };
    lines.push(format!(
        "✓ db ok — {} paths / {} visits / {} bookmarks / {} indexed",
        counts.total, counts.total_visits, counts.bookmarks, counts.indexed,
    ));

    // Schema version mismatch
    let stored_schema = db.schema_version().unwrap_or(0);
    if stored_schema != SCHEMA_VERSION {
        ok = false;
        lines.push(format!(
            "⚠ schema version mismatch: db has {}, expected {}",
            stored_schema, SCHEMA_VERSION
        ));
    } else {
        lines.push(format!("✓ schema version: {}", SCHEMA_VERSION));
    }

    let rows = db.history_rows().unwrap_or_default();
    let stale = rows.iter().filter(|r| !Path::new(&r.path).is_dir()).count();
    if stale > 0 {
        ok = false;
        let suffix = if stale > 5000 {
            " — run `hop prune` soon"
        } else {
            " — run `hop prune` to remove"
        };
        lines.push(format!("⚠ {} stale path(s) in history{}", stale, suffix));
    } else {
        lines.push("✓ no stale history entries".to_string());
    }

    // Symlink duplicates: paths that canonicalize to the same real path
    let mut seen_real: HashSet<String> = HashSet::new();
    let mut symlink_dupes: Vec<String> = Vec::new();
    for r in &rows {
        if let Ok(canonical) = std::path::Path::new(&r.path).canonicalize() {
            let canonical_str = canonical.to_string_lossy().into_owned();
            if !seen_real.insert(canonical_str.clone()) {
                symlink_dupes.push(format!("{} → {}", r.path, canonical_str));
            }
        }
    }
    if symlink_dupes.is_empty() {
        lines.push("✓ no symlink duplicates".to_string());
    } else {
        ok = false;
        lines.push(format!(
            "⚠ {} symlink duplicate(s) in history:",
            symlink_dupes.len()
        ));
        for dupe in symlink_dupes.iter().take(5) {
            lines.push(format!("  ↔ {}", dupe));
        }
        if symlink_dupes.len() > 5 {
            lines.push(format!(
                "  … and {} more (run `hop doctor` for full list)",
                symlink_dupes.len() - 5
            ));
        }
        lines.push("  → run `hop prune` to deduplicate".to_string());
    }

    match detect_shell_hook() {
        Some(shell) => lines.push(format!("✓ detected shell: {}", shell)),
        None => {
            ok = false;
            lines.push(
                "⚠ could not detect shell; add `eval \"$(hop init zsh)\"` to your rc".to_string(),
            );
        }
    }

    Report { ok, lines }
}

fn detect_shell_hook() -> Option<&'static str> {
    let shell = std::env::var("SHELL").ok()?;
    if shell.ends_with("zsh") {
        Some("zsh")
    } else if shell.ends_with("bash") {
        Some("bash")
    } else if shell.ends_with("fish") {
        Some("fish")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_on_empty_db_reports_ok_stale() {
        let db = Database::in_memory().unwrap();
        let r = run(&db);
        assert!(r.lines.iter().any(|l| l.contains("no stale history")));
    }
}

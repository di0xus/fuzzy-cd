use std::path::Path;

use crate::db::Database;

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

    let rows = db.history_rows().unwrap_or_default();
    let stale = rows.iter().filter(|r| !Path::new(&r.path).is_dir()).count();
    if stale > 0 {
        ok = false;
        lines.push(format!(
            "⚠ {} stale path(s) in history — run `hop prune` to remove",
            stale
        ));
    } else {
        lines.push("✓ no stale history entries".to_string());
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

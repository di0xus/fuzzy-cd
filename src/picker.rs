use std::io::{self, Write};
use std::time::Duration;

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
    QueueableCommand,
};

use crate::db::{now_secs, Database};
use crate::score::{Scored, Scorer, Source};

/// Returns true if the `NO_COLOR` environment variable is set (de-facto standard).
/// When set to any value, color output should be disabled.
pub fn no_color() -> bool {
    std::env::var("NO_COLOR").is_ok()
}

/// Target number of visible rows. Adapts to terminal height, capped at 20.
pub fn visible_rows() -> usize {
    terminal::size()
        .map(|(_cols, rows)| {
            // Reserve 4 rows: 1 for query input, 1 blank, 1 for help, 1 buffer
            let target = rows.saturating_sub(4) as usize;
            target.clamp(4, 20)
        })
        .unwrap_or(10)
}

pub struct PickerItem {
    pub path: String,
    pub source: Source,
    pub matched_indices: Vec<usize>,
}

/// Run interactive picker. Returns the chosen path, or None on cancel.
pub fn run(db: &Database, initial_query: &str) -> io::Result<Option<String>> {
    let mut stdout = io::stderr(); // use stderr so stdout stays clean for shell capture
    if !crossterm::tty::IsTty::is_tty(&stdout) {
        return Ok(None);
    }

    terminal::enable_raw_mode()?;
    stdout.queue(EnterAlternateScreen)?.queue(cursor::Hide)?;
    stdout.flush()?;

    let result = run_loop(&mut stdout, db, initial_query);

    stdout.queue(cursor::Show)?.queue(LeaveAlternateScreen)?;
    stdout.flush()?;
    terminal::disable_raw_mode()?;
    result
}

fn run_loop<W: Write>(
    out: &mut W,
    db: &Database,
    initial_query: &str,
) -> io::Result<Option<String>> {
    let mut query = initial_query.to_string();
    let mut cursor_idx: usize = 0;
    let mut items = compute_items(db, &query);

    loop {
        render(out, &query, &items, cursor_idx)?;

        if !event::poll(Duration::from_millis(500))? {
            continue;
        }
        match event::read()? {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => match (code, modifiers) {
                (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    return Ok(None);
                }
                (KeyCode::Enter, _) => {
                    return Ok(items.get(cursor_idx).map(|i| i.path.clone()));
                }
                (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    cursor_idx = cursor_idx.saturating_sub(1);
                }
                (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL)
                    if cursor_idx + 1 < items.len() =>
                {
                    cursor_idx += 1;
                }
                (KeyCode::Backspace, _) => {
                    query.pop();
                    items = compute_items(db, &query);
                    cursor_idx = 0;
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                    query.clear();
                    items = compute_items(db, &query);
                    cursor_idx = 0;
                }
                (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => {
                    query.push(c);
                    items = compute_items(db, &query);
                    cursor_idx = 0;
                }
                _ => {}
            },
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
}

fn compute_items(db: &Database, query: &str) -> Vec<PickerItem> {
    let scorer = Scorer::new(now_secs());
    let mut candidates: Vec<Scored> = Vec::new();
    let limit = visible_rows().max(4) * 2;

    if query.is_empty() {
        if let Ok(rows) = db.recent(limit) {
            for r in rows {
                if std::path::Path::new(&r.path).is_dir() {
                    candidates.push(Scored {
                        path: r.path,
                        score: r.last_visited as i64,
                        source: Source::History,
                        matched_indices: vec![],
                    });
                }
            }
        }
    } else {
        if let Ok(bms) = db.bookmarks() {
            for (alias, path) in bms {
                if let Some(s) = scorer.score_bookmark(&alias, &path, query) {
                    candidates.push(s);
                }
            }
        }
        if let Ok(rows) = db.history_rows() {
            for r in rows {
                if let Some(s) = scorer.score_history(&r, query) {
                    if std::path::Path::new(&s.path).is_dir() {
                        candidates.push(s);
                    }
                }
            }
        }
    }

    candidates.sort_by_key(|c| std::cmp::Reverse(c.score));
    candidates.dedup_by(|a, b| a.path == b.path);
    candidates
        .into_iter()
        .take(visible_rows().max(4))
        .map(|c| PickerItem {
            path: c.path,
            source: c.source,
            matched_indices: c.matched_indices,
        })
        .collect()
}

fn render<W: Write>(
    out: &mut W,
    query: &str,
    items: &[PickerItem],
    cursor_idx: usize,
) -> io::Result<()> {
    let nc = no_color();
    out.queue(cursor::MoveTo(0, 0))?
        .queue(Clear(ClearType::All))?;
    if !nc {
        out.queue(SetForegroundColor(Color::Cyan))?;
    }
    out.queue(Print("› "))?;
    if !nc {
        out.queue(ResetColor)?;
    }
    out.queue(Print(query))?;
    out.queue(Print("\r\n"))?;

    if items.is_empty() {
        if !nc {
            out.queue(SetForegroundColor(Color::DarkGrey))?;
        }
        out.queue(Print("  no matches\r\n"))?;
        if !nc {
            out.queue(ResetColor)?;
        }
    }

    for (i, item) in items.iter().enumerate() {
        let selected = i == cursor_idx;
        if selected && !nc {
            out.queue(SetAttribute(Attribute::Reverse))?;
        }
        let tag = match item.source {
            Source::Bookmark => "★",
            Source::History => " ",
            Source::Index => "·",
        };
        if !nc {
            out.queue(SetForegroundColor(Color::DarkGrey))?;
        }
        out.queue(Print(format!(" {} ", tag)))?;
        if !nc {
            out.queue(ResetColor)?;
        }
        if selected && !nc {
            out.queue(SetAttribute(Attribute::Reverse))?;
        }
        render_highlighted(out, &item.path, &item.matched_indices)?;
        if selected && !nc {
            out.queue(SetAttribute(Attribute::Reset))?;
        }
        out.queue(Print("\r\n"))?;
    }

    if !nc {
        out.queue(SetForegroundColor(Color::DarkGrey))?;
    }
    out.queue(Print("\r\n  enter select · esc cancel · ↑↓ move\r\n"))?;
    if !nc {
        out.queue(ResetColor)?;
    }
    out.flush()?;
    Ok(())
}

fn render_highlighted<W: Write>(out: &mut W, path: &str, indices: &[usize]) -> io::Result<()> {
    let nc = no_color();
    let mut idx_iter = indices.iter().peekable();
    for (i, ch) in path.chars().enumerate() {
        let hit = idx_iter.peek().is_some_and(|&&next| next == i);
        if hit {
            idx_iter.next();
            if !nc {
                out.queue(SetForegroundColor(Color::Yellow))?
                    .queue(SetAttribute(Attribute::Bold))?;
            }
            out.queue(Print(ch))?;
            if !nc {
                out.queue(SetAttribute(Attribute::Reset))?
                    .queue(ResetColor)?;
            }
        } else {
            out.queue(Print(ch))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_items_empty_query_returns_recent() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_at(&tmp.path().join("hop.db")).unwrap();
        let target = tmp.path().join("recent-dir");
        std::fs::create_dir(&target).unwrap();
        db.record_visit(&target.to_string_lossy()).unwrap();

        let items = compute_items(&db, "");
        assert!(!items.is_empty(), "empty query should return recent items");
        assert_eq!(items[0].source, Source::History);
    }

    #[test]
    fn compute_items_filters_deleted_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_at(&tmp.path().join("hop.db")).unwrap();
        let alive = tmp.path().join("alive");
        let dead = tmp.path().join("dead");
        std::fs::create_dir(&alive).unwrap();
        std::fs::create_dir(&dead).unwrap();
        db.record_visit(&alive.to_string_lossy()).unwrap();
        db.record_visit(&dead.to_string_lossy()).unwrap();
        std::fs::remove_dir(&dead).unwrap();

        let items = compute_items(&db, "dead");
        let paths: Vec<_> = items.iter().map(|i| i.path.clone()).collect();
        assert!(
            !paths.iter().any(|p| p.contains("dead")),
            "deleted path should not appear, got: {:?}",
            paths
        );
    }

    #[test]
    fn compute_items_non_empty_query_returns_scored() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_at(&tmp.path().join("hop.db")).unwrap();
        let target = tmp.path().join("my-project");
        std::fs::create_dir(&target).unwrap();
        db.record_visit(&target.to_string_lossy()).unwrap();

        let items = compute_items(&db, "proj");
        assert!(!items.is_empty(), "query 'proj' should match 'my-project'");
    }

    #[test]
    fn compute_items_deduplicates_by_path() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_at(&tmp.path().join("hop.db")).unwrap();
        let target = tmp.path().join("dup");
        std::fs::create_dir(&target).unwrap();
        db.record_visit(&target.to_string_lossy()).unwrap();
        db.record_visit(&target.to_string_lossy()).unwrap();
        db.record_visit(&target.to_string_lossy()).unwrap();

        let items = compute_items(&db, "dup");
        let paths: Vec<_> = items.iter().map(|i| i.path.clone()).collect();
        assert_eq!(
            paths.len(),
            paths.iter().collect::<std::collections::HashSet<_>>().len(),
            "paths should not be duplicated, got: {:?}",
            paths
        );
    }

    #[test]
    fn compute_items_empty_db_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open_at(&tmp.path().join("hop.db")).unwrap();
        // No entries recorded — should not panic
        let items = compute_items(&db, "");
        assert!(items.is_empty(), "empty DB should yield no items");
    }

    #[test]
    fn no_color_detects_env_var() {
        // Save original
        let original = std::env::var("NO_COLOR");
        std::env::remove_var("NO_COLOR");
        assert!(!no_color(), "NO_COLOR unset → false");

        std::env::set_var("NO_COLOR", "1");
        assert!(no_color(), "NO_COLOR=1 → true");

        std::env::set_var("NO_COLOR", "");
        assert!(no_color(), "NO_COLOR='' → true");

        // Restore
        match original {
            Ok(v) => std::env::set_var("NO_COLOR", v),
            Err(_) => std::env::remove_var("NO_COLOR"),
        }
    }

    #[test]
    fn visible_rows_defaults_without_panic() {
        let rows = visible_rows();
        assert!(rows >= 4, "visible_rows should be at least 4");
        assert!(rows <= 20, "visible_rows should be at most 20");
    }
}

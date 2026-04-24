use std::fs;
use std::path::{Path, PathBuf};

use crate::db::{expand_home, Database};

pub struct ImportStats {
    pub imported: usize,
    pub skipped: usize,
}

/// fasd `.fasd` cache is tab-separated: `path\tvisits\tlast`.
pub fn import_fasd(db: &Database, path: &Path) -> std::io::Result<ImportStats> {
    let content = fs::read_to_string(path)?;
    let mut stats = ImportStats {
        imported: 0,
        skipped: 0,
    };
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let raw_path = parts.next().unwrap_or("").trim();
        if raw_path.is_empty() {
            stats.skipped += 1;
            continue;
        }
        let visits: i32 = parts
            .next()
            .and_then(|v| v.trim().parse::<f64>().ok().map(|f| f as i32))
            .unwrap_or(1)
            .clamp(1, 100);
        let abs = expand_home(raw_path);
        if is_existing_dir(&abs) {
            let as_str = abs.to_string_lossy();
            for _ in 0..visits {
                db.record_visit(&as_str).ok();
            }
            stats.imported += 1;
        } else {
            stats.skipped += 1;
        }
    }
    Ok(stats)
}

/// Parse zsh `$HISTFILE`. Supports both:
///   plain:    `cd ~/foo`
///   extended: `: 1700000000:0;cd ~/foo`
/// Multi-line commands (trailing `\`) are concatenated.
pub fn import_zsh(db: &Database, path: &Path) -> std::io::Result<ImportStats> {
    let content = fs::read_to_string(path)?;
    let commands = parse_zsh_history(&content);
    let mut stats = ImportStats {
        imported: 0,
        skipped: 0,
    };

    for cmd in commands {
        if let Some(target) = extract_cd_target(&cmd) {
            let expanded = expand_home(&target);
            if is_existing_dir(&expanded) {
                db.record_visit(&expanded.to_string_lossy()).ok();
                stats.imported += 1;
            } else {
                stats.skipped += 1;
            }
        }
    }
    Ok(stats)
}

pub fn parse_zsh_history(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for raw in content.lines() {
        let line = if let Some(rest) = raw.strip_prefix(": ") {
            rest.split_once(';').map(|x| x.1).unwrap_or("")
        } else {
            raw
        };

        if let Some(stripped) = line.strip_suffix('\\') {
            buf.push_str(stripped);
            buf.push('\n');
        } else {
            buf.push_str(&line);
            if !buf.trim().is_empty() {
                out.push(std::mem::take(&mut buf));
            } else {
                buf.clear();
            }
        }
    }
    if !buf.trim().is_empty() {
        out.push(buf);
    }
    out
}

/// Extract the directory argument of a cd-like command.
/// Returns None if the line is not a cd/pushd, or uses unsupported forms
/// (no arg, `-`, env var, subshell).
pub fn extract_cd_target(cmd: &str) -> Option<String> {
    let trimmed = cmd.trim_start();
    // Skip leading `&&` / `;` compound chains by taking first segment.
    // Keep it simple: split on first unquoted ; & |.
    let first = split_first_segment(trimmed);
    let tokens = shell_tokens(first);
    let mut it = tokens.into_iter();
    let verb = it.next()?;
    if verb != "cd" && verb != "pushd" {
        return None;
    }
    let arg = it.next()?;
    if arg == "-" || arg.starts_with('-') {
        return None;
    }
    if arg.contains('$') || arg.contains('`') {
        return None;
    }
    Some(arg)
}

fn split_first_segment(s: &str) -> &str {
    let bytes = s.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b';' | b'&' | b'|' if !in_single && !in_double => return &s[..i],
            _ => {}
        }
    }
    s
}

/// Very small shell-style tokenizer: handles single and double quotes,
/// backslash escapes, and whitespace separation. Good enough for parsing
/// cd/pushd arguments out of history.
fn shell_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;
    let mut has_token = false;
    for ch in s.chars() {
        if escape {
            cur.push(ch);
            escape = false;
            has_token = true;
            continue;
        }
        match ch {
            '\\' if !in_single => escape = true,
            '\'' if !in_double => {
                in_single = !in_single;
                has_token = true;
            }
            '"' if !in_single => {
                in_double = !in_double;
                has_token = true;
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if has_token {
                    out.push(std::mem::take(&mut cur));
                    has_token = false;
                }
            }
            c => {
                cur.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        out.push(cur);
    }
    out
}

fn is_existing_dir(p: &PathBuf) -> bool {
    fs::metadata(p).map(|m| m.is_dir()).unwrap_or(false)
}

/// autojump "~/.local/share/autojump/autojump.txt" — one line per dir:
/// `weight\tpath`
pub fn import_autojump(db: &Database, path: &Path) -> std::io::Result<ImportStats> {
    let content = fs::read_to_string(path)?;
    let mut stats = ImportStats {
        imported: 0,
        skipped: 0,
    };
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let raw_path = parts.next().unwrap_or("").trim();
        if raw_path.is_empty() {
            stats.skipped += 1;
            continue;
        }
        let weight: f64 = parts
            .next()
            .and_then(|v| v.trim().parse::<f64>().ok())
            .unwrap_or(1.0);
        let abs = expand_home(raw_path);
        if is_existing_dir(&abs) {
            // Record visit once per autojump weight bucket (1-100 → 1-10 visits)
            let visits = (weight.clamp(1.0, 100.0) / 10.0) as i32;
            let as_str = abs.to_string_lossy();
            for _ in 0..visits.max(1) {
                db.record_visit(&as_str).ok();
            }
            stats.imported += 1;
        } else {
            stats.skipped += 1;
        }
    }
    Ok(stats)
}

/// zoxide "~/.local/share/zoxide/db.zo" — msgpack format.
/// Each entry is an array: [path (str), score (f64), ...]
pub fn import_zoxide(db: &Database, path: &Path) -> std::io::Result<ImportStats> {
    let data = fs::read(path)?;
    let mut stats = ImportStats {
        imported: 0,
        skipped: 0,
    };

    // Try to decode as an array of [path, score] arrays
    use rmp_serde::Deserializer;
    use serde::Deserialize;
    #[derive(Debug, Deserialize)]
    struct ZoxideEntry(String, f64);

    let mut deser = Deserializer::new(&data[..]);
    if let Ok(entries) = Vec::<ZoxideEntry>::deserialize(&mut deser) {
        for entry in entries {
            let abs = expand_home(&entry.0);
            if is_existing_dir(&abs) {
                let visits = (entry.1.clamp(1.0, 100.0) / 10.0) as i32;
                let as_str = abs.to_string_lossy();
                for _ in 0..visits.max(1) {
                    db.record_visit(&as_str).ok();
                }
                stats.imported += 1;
            } else {
                stats.skipped += 1;
            }
        }
    } else {
        // Fallback: simple string array
        let mut deser2 = Deserializer::new(&data[..]);
        if let Ok(paths) = Vec::<String>::deserialize(&mut deser2) {
            for raw_path in paths {
                let abs = expand_home(&raw_path);
                if is_existing_dir(&abs) {
                    db.record_visit(&abs.to_string_lossy()).ok();
                    stats.imported += 1;
                } else {
                    stats.skipped += 1;
                }
            }
        }
    }
    Ok(stats)
}

/// thefuck alias import — parses shell alias lines from a file and offers
/// directories mentioned in alias targets as bookmarks.
/// Looks for `alias <name>='cd <path>'` or `alias <name>="cd <path>"` patterns.
pub fn import_thefuck(db: &Database, path: &Path) -> std::io::Result<ImportStats> {
    let content = fs::read_to_string(path)?;
    let mut stats = ImportStats {
        imported: 0,
        skipped: 0,
    };
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("alias ") {
            continue;
        }
        let after_alias = match trimmed.strip_prefix("alias ") {
            Some(s) => s,
            None => continue,
        };
        // Parse alias name=expression
        let (alias, expr) = match after_alias.split_once('=') {
            Some((a, e)) => (a, e),
            None => continue,
        };
        // Strip quotes from expression
        let expr = expr.trim_matches(|c| c == '\'' || c == '"');
        // Look for cd or pushd targets
        if let Some(target) = extract_cd_target_from_alias_expr(expr) {
            let abs = expand_home(&target);
            if is_existing_dir(&abs) {
                // Register as a bookmark with the alias name
                let alias_clean = alias.trim();
                if !alias_clean.is_empty() {
                    db.set_bookmark(alias_clean, &abs.to_string_lossy()).ok();
                    stats.imported += 1;
                }
            } else {
                stats.skipped += 1;
            }
        }
    }
    Ok(stats)
}

/// Extract cd/pushd path from a thefuck alias expression like `cd /path`
fn extract_cd_target_from_alias_expr(expr: &str) -> Option<String> {
    let tokens: Vec<&str> = expr.split_whitespace().collect();
    let mut it = tokens.iter();
    let verb = it.next()?;
    if *verb != "cd" && *verb != "pushd" {
        return None;
    }
    let arg = it.next()?;
    if arg.starts_with('-') || arg.contains('$') || arg.contains('`') {
        return None;
    }
    Some(arg.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_plain_cd() {
        assert_eq!(extract_cd_target("cd /tmp").as_deref(), Some("/tmp"));
        assert_eq!(extract_cd_target("cd ~/foo").as_deref(), Some("~/foo"));
        assert_eq!(extract_cd_target("pushd /a/b").as_deref(), Some("/a/b"));
    }

    #[test]
    fn rejects_non_cd_and_bad_forms() {
        assert!(extract_cd_target("ls /tmp").is_none());
        assert!(extract_cd_target("cd -").is_none());
        assert!(extract_cd_target("cd").is_none());
        assert!(extract_cd_target("cd $HOME").is_none());
        assert!(extract_cd_target("cd $(pwd)").is_none());
    }

    #[test]
    fn strips_quotes() {
        assert_eq!(extract_cd_target("cd \"/a b\"").as_deref(), Some("/a b"));
        assert_eq!(extract_cd_target("cd '/x'").as_deref(), Some("/x"));
    }

    #[test]
    fn handles_compound() {
        assert_eq!(extract_cd_target("cd /tmp && ls").as_deref(), Some("/tmp"));
    }

    #[test]
    fn parses_extended_history() {
        let raw = ": 1700000000:0;cd /tmp\n: 1700000001:0;ls\n";
        let cmds = parse_zsh_history(raw);
        assert_eq!(cmds, vec!["cd /tmp".to_string(), "ls".to_string()]);
    }

    #[test]
    fn parses_plain_history() {
        let raw = "cd /tmp\nls\n";
        let cmds = parse_zsh_history(raw);
        assert_eq!(cmds, vec!["cd /tmp".to_string(), "ls".to_string()]);
    }

    #[test]
    fn joins_multiline_continuation() {
        let raw = ": 1700000000:0;echo a\\\nb\n";
        let cmds = parse_zsh_history(raw);
        assert_eq!(cmds, vec!["echo a\nb".to_string()]);
    }

    #[test]
    fn import_fasd_from_tempfile() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("sub");
        std::fs::create_dir(&target).unwrap();
        let fasd_file = tmp.path().join(".fasd");
        let line = format!("{}\t5\t1700000000\n", target.display());
        std::fs::write(&fasd_file, line).unwrap();

        let db = Database::in_memory().unwrap();
        let stats = import_fasd(&db, &fasd_file).unwrap();
        assert_eq!(stats.imported, 1);
        let rows = db.history_rows().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].visits, 5);
    }

    #[test]
    fn import_zsh_from_tempfile() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let hist = tmp.path().join("hist");
        let content = format!(
            ": 1700000000:0;cd {}\n: 1700000001:0;cd /definitely/not/here/xyz\n: 1700000002:0;ls\n",
            real.display()
        );
        std::fs::write(&hist, content).unwrap();
        let db = Database::in_memory().unwrap();
        let stats = import_zsh(&db, &hist).unwrap();
        assert_eq!(stats.imported, 1);
        assert_eq!(stats.skipped, 1);
    }
}

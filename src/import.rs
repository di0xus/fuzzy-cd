use std::fs;
use std::path::{Path, PathBuf};

use crate::db::{expand_home, Database};

/// Maximum size (in bytes) of an import file. A 10 GB zsh history file should
/// not be loaded into memory. 50 MB is a generous limit for any realistic
/// history or cache file.
pub const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;

/// Errors that can occur during an import operation.
#[derive(Debug)]
pub enum ImportError {
    /// The import file exceeds MAX_FILE_SIZE and was rejected to prevent OOM.
    FileTooLarge { size: u64, max: u64 },
    /// The file does not exist or could not be read.
    Io(std::io::Error),
}

impl Clone for ImportError {
    fn clone(&self) -> Self {
        match self {
            Self::FileTooLarge { size, max } => Self::FileTooLarge {
                size: *size,
                max: *max,
            },
            Self::Io(e) => Self::Io(std::io::Error::new(e.kind(), e.to_string())),
        }
    }
}

impl PartialEq for ImportError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::FileTooLarge { size: l, max: lm }, Self::FileTooLarge { size: r, max: rm }) => {
                l == r && lm == rm
            }
            (Self::Io(l), Self::Io(r)) => l.to_string() == r.to_string(),
            _ => false,
        }
    }
}

impl Eq for ImportError {}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::FileTooLarge { size, max } => {
                write!(
                    f,
                    "import file is {} bytes (max {} MB); refusing to read to prevent OOM",
                    size,
                    max / (1024 * 1024)
                )
            }
            ImportError::Io(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<std::io::Error> for ImportError {
    fn from(e: std::io::Error) -> Self {
        ImportError::Io(e)
    }
}

/// Check that `path` is smaller than MAX_FILE_SIZE before reading.
/// Returns `Err(ImportError::FileTooLarge)` if the file is too big;
/// returns `Ok(())` if reading may proceed.
fn check_file_size(path: &Path) -> Result<(), ImportError> {
    let size = fs::metadata(path)?.len();
    if size > MAX_FILE_SIZE {
        return Err(ImportError::FileTooLarge {
            size,
            max: MAX_FILE_SIZE,
        });
    }
    Ok(())
}

/// Read the full contents of `path` as a `String`, but only after checking
/// that the file is smaller than MAX_FILE_SIZE.
fn read_to_string(path: &Path) -> Result<String, ImportError> {
    check_file_size(path)?;
    fs::read_to_string(path).map_err(ImportError::Io)
}

/// Read the full contents of `path` as raw bytes, but only after checking
/// that the file is smaller than MAX_FILE_SIZE.
fn read_bytes(path: &Path) -> Result<Vec<u8>, ImportError> {
    check_file_size(path)?;
    fs::read(path).map_err(ImportError::Io)
}

pub struct ImportStats {
    pub imported: usize,
    pub skipped: usize,
}

/// Parse an import file and return a preview of what would be imported,
/// WITHOUT writing to the database. Returns list of paths that would be imported.
pub fn import_dry_run(source: &str, path: &Path) -> Result<Vec<String>, ImportError> {
    match source {
        "fasd" => dry_run_fasd(path),
        "autojump" => dry_run_autojump(path),
        "zoxide" => dry_run_zoxide(path),
        "zsh" => dry_run_zsh(path),
        _ => Err(ImportError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown source: {}", source),
        ))),
    }
}

fn dry_run_fasd(path: &Path) -> Result<Vec<String>, ImportError> {
    let content = read_to_string(path)?;
    let mut result = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(3, '\t');
        let raw_path = parts.next().unwrap_or("").trim();
        if raw_path.is_empty() {
            continue;
        }
        let abs = expand_home(raw_path);
        if is_existing_dir(&abs) {
            result.push(abs.to_string_lossy().into_owned());
        }
    }
    Ok(result)
}

fn dry_run_autojump(path: &Path) -> Result<Vec<String>, ImportError> {
    let content = read_to_string(path)?;
    let mut result = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let raw_path = parts.next().unwrap_or("").trim();
        if raw_path.is_empty() {
            continue;
        }
        let abs = expand_home(raw_path);
        if is_existing_dir(&abs) {
            result.push(abs.to_string_lossy().into_owned());
        }
    }
    Ok(result)
}

fn dry_run_zoxide(path: &Path) -> Result<Vec<String>, ImportError> {
    use rmp_serde::Deserializer;
    use serde::Deserialize;

    let data = read_bytes(path)?;
    let mut result = Vec::new();

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct ZoxideEntry(String, f64);

    let mut deser = Deserializer::new(&data[..]);
    if let Ok(entries) = Vec::<ZoxideEntry>::deserialize(&mut deser) {
        for entry in entries {
            let abs = expand_home(&entry.0);
            if is_existing_dir(&abs) {
                result.push(abs.to_string_lossy().into_owned());
            }
        }
    } else {
        let mut deser2 = Deserializer::new(&data[..]);
        if let Ok(paths) = Vec::<String>::deserialize(&mut deser2) {
            for raw_path in paths {
                let abs = expand_home(&raw_path);
                if is_existing_dir(&abs) {
                    result.push(abs.to_string_lossy().into_owned());
                }
            }
        }
    }
    Ok(result)
}

fn dry_run_zsh(path: &Path) -> Result<Vec<String>, ImportError> {
    let content = read_to_string(path)?;
    let commands = parse_zsh_history(&content);
    let mut result = Vec::new();
    for cmd in commands {
        if let Some(target) = extract_cd_target(&cmd) {
            let expanded = expand_home(&target);
            if is_existing_dir(&expanded) {
                result.push(expanded.to_string_lossy().into_owned());
            }
        }
    }
    Ok(result)
}

/// fasd `.fasd` cache is tab-separated: `path\tvisits\tlast`.
pub fn import_fasd(db: &Database, path: &Path) -> Result<ImportStats, ImportError> {
    let content = read_to_string(path)?;
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
            db.record_visits(&abs.to_string_lossy(), visits as i64).ok();
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
pub fn import_zsh(db: &Database, path: &Path) -> Result<ImportStats, ImportError> {
    let content = read_to_string(path)?;
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
            buf.push_str(line);
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
pub fn import_autojump(db: &Database, path: &Path) -> Result<ImportStats, ImportError> {
    let content = read_to_string(path)?;
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
            db.record_visits(&abs.to_string_lossy(), visits.max(1) as i64)
                .ok();
            stats.imported += 1;
        } else {
            stats.skipped += 1;
        }
    }
    Ok(stats)
}

/// zoxide "~/.local/share/zoxide/db.zo" — msgpack format.
/// Each entry is an array: [path (str), score (f64), ...]
pub fn import_zoxide(db: &Database, path: &Path) -> Result<ImportStats, ImportError> {
    use rmp_serde::Deserializer;
    use serde::Deserialize;

    let data = read_bytes(path)?;
    let mut stats = ImportStats {
        imported: 0,
        skipped: 0,
    };

    // Try to decode as an array of [path, score] arrays
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct ZoxideEntry(String, f64);

    let mut deser = Deserializer::new(&data[..]);
    if let Ok(entries) = Vec::<ZoxideEntry>::deserialize(&mut deser) {
        for entry in entries {
            let abs = expand_home(&entry.0);
            if is_existing_dir(&abs) {
                let visits = (entry.1.clamp(1.0, 100.0) / 10.0) as i32;
                db.record_visits(&abs.to_string_lossy(), visits.max(1) as i64)
                    .ok();
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

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

fn read_to_string(path: &Path) -> Result<String, ImportError> {
    check_file_size(path)?;
    fs::read_to_string(path).map_err(ImportError::Io)
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, ImportError> {
    check_file_size(path)?;
    fs::read(path).map_err(ImportError::Io)
}

pub struct ImportStats {
    pub imported: usize,
    pub skipped: usize,
}

/// Parse an import file into `(path, visits)` entries. Paths are raw (not
/// `~`-expanded); visits are derived per source from weight/score fields.
/// Shared by the real and dry-run import paths.
fn parse_source(source: &str, path: &Path) -> Result<Vec<(String, i64)>, ImportError> {
    match source {
        "fasd" => parse_fasd(path),
        "autojump" => parse_autojump(path),
        "zoxide" => parse_zoxide(path),
        "zsh" => parse_zsh(path),
        _ => Err(ImportError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("unknown source: {}", source),
        ))),
    }
}

/// fasd `.fasd` cache is tab-separated: `path\tvisits\tlast`.
fn parse_fasd(path: &Path) -> Result<Vec<(String, i64)>, ImportError> {
    let content = read_to_string(path)?;
    Ok(content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let raw_path = parts.next()?.trim();
            if raw_path.is_empty() {
                return None;
            }
            let visits = parts
                .next()
                .and_then(|v| v.trim().parse::<f64>().ok().map(|f| f as i64))
                .unwrap_or(1)
                .clamp(1, 100);
            Some((raw_path.to_string(), visits))
        })
        .collect())
}

/// autojump "~/.local/share/autojump/autojump.txt" — one line per dir:
/// `weight\tpath`
fn parse_autojump(path: &Path) -> Result<Vec<(String, i64)>, ImportError> {
    let content = read_to_string(path)?;
    Ok(content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let mut parts = line.splitn(2, '\t');
            let raw_path = parts.next()?.trim();
            if raw_path.is_empty() {
                return None;
            }
            let weight: f64 = parts
                .next()
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(1.0);
            let visits = ((weight.clamp(1.0, 100.0) / 10.0) as i64).max(1);
            Some((raw_path.to_string(), visits))
        })
        .collect())
}

/// zoxide "~/.local/share/zoxide/db.zo" — msgpack format.
/// Each entry is an array: [path (str), score (f64), ...]
fn parse_zoxide(path: &Path) -> Result<Vec<(String, i64)>, ImportError> {
    use rmp_serde::Deserializer;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct ZoxideEntry(String, f64);

    let data = read_bytes(path)?;
    let mut deser = Deserializer::new(&data[..]);
    if let Ok(entries) = Vec::<ZoxideEntry>::deserialize(&mut deser) {
        return Ok(entries
            .into_iter()
            .map(|ZoxideEntry(p, score)| {
                let visits = ((score.clamp(1.0, 100.0) / 10.0) as i64).max(1);
                (p, visits)
            })
            .collect());
    }
    // Fallback: simple string array
    let mut deser2 = Deserializer::new(&data[..]);
    Ok(Vec::<String>::deserialize(&mut deser2)
        .unwrap_or_default()
        .into_iter()
        .map(|p| (p, 1))
        .collect())
}

fn parse_zsh(path: &Path) -> Result<Vec<(String, i64)>, ImportError> {
    let content = read_to_string(path)?;
    Ok(parse_zsh_history(&content)
        .into_iter()
        .filter_map(|cmd| extract_cd_target(&cmd).map(|t| (t, 1)))
        .collect())
}

/// Record visits for every entry whose directory still exists.
fn apply_import(db: &Database, entries: Vec<(String, i64)>) -> ImportStats {
    let mut stats = ImportStats {
        imported: 0,
        skipped: 0,
    };
    for (raw_path, visits) in entries {
        let abs = expand_home(&raw_path);
        if is_existing_dir(&abs) {
            db.record_visits(&abs.to_string_lossy(), visits).ok();
            stats.imported += 1;
        } else {
            stats.skipped += 1;
        }
    }
    stats
}

/// Parse an import file and return a preview of what would be imported,
/// WITHOUT writing to the database. Returns list of paths that would be imported.
pub fn import_dry_run(source: &str, path: &Path) -> Result<Vec<String>, ImportError> {
    Ok(parse_source(source, path)?
        .into_iter()
        .filter(|(raw, _)| is_existing_dir(&expand_home(raw)))
        .map(|(raw, _)| expand_home(&raw).to_string_lossy().into_owned())
        .collect())
}

pub fn import_fasd(db: &Database, path: &Path) -> Result<ImportStats, ImportError> {
    Ok(apply_import(db, parse_fasd(path)?))
}

pub fn import_autojump(db: &Database, path: &Path) -> Result<ImportStats, ImportError> {
    Ok(apply_import(db, parse_autojump(path)?))
}

pub fn import_zoxide(db: &Database, path: &Path) -> Result<ImportStats, ImportError> {
    Ok(apply_import(db, parse_zoxide(path)?))
}

pub fn import_zsh(db: &Database, path: &Path) -> Result<ImportStats, ImportError> {
    Ok(apply_import(db, parse_zsh(path)?))
}

/// Parse zsh `$HISTFILE`. Supports both:
///   plain:    `cd ~/foo`
///   extended: `: 1700000000:0;cd ~/foo`
/// Multi-line commands (trailing `\`) are concatenated.
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

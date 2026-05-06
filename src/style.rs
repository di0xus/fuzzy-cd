//! Styled output for hop CLI.
/// ANSI style constants for hop output.
pub mod attr {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";
    pub const ITALIC: &str = "\x1b[3m";
    pub const UNDERLINE: &str = "\x1b[4m";
}

pub mod color {
    pub const BLACK: &str = "\x1b[30m";
    pub const RED: &str = "\x1b[31m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const BLUE: &str = "\x1b[34m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const CYAN: &str = "\x1b[36m";
    pub const WHITE: &str = "\x1b[37m";
    pub const DEFAULT: &str = "\x1b[39m";
}

/// Prefix/suffix helpers for common output patterns.
pub struct Fmt;

impl Fmt {
    /// Bold white — for main headings / hop logo.
    pub fn heading(s: &str) -> String {
        format!("{}{}{}", attr::BOLD, s, attr::RESET)
    }

    /// Cyan + bold — for paths and directory names.
    pub fn path(s: &str) -> String {
        format!("{}{}{}{}", attr::BOLD, color::CYAN, s, attr::RESET)
    }

    /// Green — success confirmations (added, removed, imported).
    pub fn success(s: &str) -> String {
        format!("{}{}{}", color::GREEN, s, attr::RESET)
    }

    /// Yellow — dry-run hints, warnings, "would X" messages.
    pub fn warn(s: &str) -> String {
        format!("{}{}{}", color::YELLOW, s, attr::RESET)
    }

    /// Red — errors and failures.
    pub fn error(s: &str) -> String {
        format!("{}{}{}{}", attr::BOLD, color::RED, s, attr::RESET)
    }

    /// Dim — secondary info like counts, metadata.
    pub fn dim(s: &str) -> String {
        format!("{}{}{}", attr::DIM, s, attr::RESET)
    }

    /// Bold — for numbers, version strings, key values.
    pub fn bold(s: &str) -> String {
        format!("{}{}{}", attr::BOLD, s, attr::RESET)
    }

    /// Magenta — for bookmarks / special entries.
    pub fn bookmark(s: &str) -> String {
        format!("{}{}{}", color::MAGENTA, s, attr::RESET)
    }

    /// Print a section divider line.
    pub fn divider() {
        let line: String = std::iter::repeat('─').take(40).collect();
        println!("{}", Fmt::dim(&line));
    }

    /// Print a success-prefixed line: "✓ <msg>"
    pub fn ok_line(msg: &str) {
        println!("{} {}", Fmt::success("✓"), msg);
    }

    /// Print a warn-prefixed line: "⚠ <msg>"
    pub fn warn_line(msg: &str) {
        println!("{} {}", Fmt::warn("⚠"), msg);
    }

    /// Print an error-prefixed line: "✗ <msg>"
    pub fn err_line(msg: &str) {
        eprintln!("{} {}", Fmt::error("✗"), msg);
    }

    /// Print a dry-run line: "→ <msg>" in yellow.
    pub fn dryrun_line(msg: &str) {
        println!("{} {}", Fmt::warn("→"), Fmt::warn(msg));
    }
}

/// Format a count with bold.
pub fn count(n: usize) -> String {
    Fmt::bold(&format!("{}", n))
}

/// Format a visit count: "(N visits)" in dim.
pub fn visits_fmt(n: i64) -> String {
    let s = format!("({} visits)", n);
    Fmt::dim(&s)
}

/// Format a history row: "<path>  (N visits)" with visits dimmed.
pub fn history_row(path: &str, visits: i64) -> String {
    format!("{}  {}", Fmt::path(path), visits_fmt(visits))
}

/// Format a bookmark row: alias in magenta, path in cyan.
pub fn bookmark_row(alias: &str, path: &str) -> String {
    format!("  {}  {}", Fmt::bookmark(alias), Fmt::path(path))
}

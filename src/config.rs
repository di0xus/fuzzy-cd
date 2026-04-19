use std::path::{Path, PathBuf};

use crate::db::{default_data_dir, expand_home, home_dir};

#[derive(Debug, Clone)]
pub struct Config {
    pub index_roots: Vec<PathBuf>,
    pub skip_dirs: Vec<String>,
    pub max_depth: usize,
    pub min_score: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            index_roots: vec![home_dir()],
            skip_dirs: Self::default_skip_dirs(),
            max_depth: 6,
            min_score: crate::score::MIN_SCORE,
        }
    }
}

impl Config {
    pub fn default_skip_dirs() -> Vec<String> {
        [
            "Library",
            "Music",
            "Movies",
            "Pictures",
            "Documents",
            "Applications",
            "Desktop",
            "node_modules",
            "target",
            ".git",
            ".svn",
            ".hg",
            "__pycache__",
            ".venv",
            "venv",
            ".cache",
            ".next",
            "dist",
            "build",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    pub fn should_skip(&self, name: &str) -> bool {
        if name.starts_with('.') {
            return true;
        }
        self.skip_dirs.iter().any(|d| d == name)
    }

    pub fn default_path() -> PathBuf {
        default_data_dir().join("config.toml")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::default_path())
    }

    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => Self::parse(&s),
            Err(_) => Config::default(),
        }
    }

    /// Tiny purpose-built parser — we only support the handful of keys we own.
    /// Grammar: `key = value`, `# comment`, arrays `key = ["a", "b"]`.
    pub fn parse(src: &str) -> Self {
        let mut cfg = Config::default();
        let mut explicit_roots: Option<Vec<PathBuf>> = None;
        let mut explicit_skip: Option<Vec<String>> = None;

        for raw in src.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "index_roots" => {
                    explicit_roots = Some(
                        parse_string_array(value)
                            .iter()
                            .map(|s| expand_home(s))
                            .collect(),
                    );
                }
                "skip_dirs" => {
                    explicit_skip = Some(parse_string_array(value));
                }
                "max_depth" => {
                    if let Ok(n) = value.parse::<usize>() {
                        cfg.max_depth = n;
                    }
                }
                "min_score" => {
                    if let Ok(n) = value.parse::<i64>() {
                        cfg.min_score = n;
                    }
                }
                _ => {}
            }
        }
        if let Some(roots) = explicit_roots {
            cfg.index_roots = roots;
        }
        if let Some(skip) = explicit_skip {
            cfg.skip_dirs = skip;
        }
        cfg
    }
}

fn parse_string_array(s: &str) -> Vec<String> {
    let inner = s.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|piece| {
            piece
                .trim()
                .trim_matches(|c| c == '"' || c == '\'')
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_defaults_when_empty() {
        let c = Config::parse("");
        assert!(c.max_depth > 0);
        assert!(!c.skip_dirs.is_empty());
    }

    #[test]
    fn parse_overrides() {
        let c = Config::parse(
            "# hi\nindex_roots = [\"/a/code\", \"/srv\"]\nskip_dirs = [\"foo\"]\nmax_depth = 3\nmin_score = 50\n",
        );
        assert_eq!(
            c.index_roots,
            vec![PathBuf::from("/a/code"), PathBuf::from("/srv")]
        );
        assert_eq!(c.skip_dirs, vec!["foo".to_string()]);
        assert_eq!(c.max_depth, 3);
        assert_eq!(c.min_score, 50);
    }

    #[test]
    fn should_skip_hidden_and_listed() {
        let c = Config::default();
        assert!(c.should_skip(".git"));
        assert!(c.should_skip("node_modules"));
        assert!(!c.should_skip("src"));
    }
}

use std::path::PathBuf;

use crate::db::default_data_dir;

#[derive(Debug, Clone)]
pub struct Config {
    pub min_score: i64,
    pub auto_prune_on_startup: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ParseWarnings {
    pub unknown_keys: Vec<String>,
    pub invalid_values: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            min_score: crate::score::MIN_SCORE,
            auto_prune_on_startup: false,
        }
    }
}

impl Config {
    pub fn default_path() -> PathBuf {
        default_data_dir().join("config.toml")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::default_path())
    }

    /// Load config and print any warnings to stderr.
    pub fn load_with_warnings() -> (Self, ParseWarnings) {
        let path = Self::default_path();
        let mut warnings = ParseWarnings::default();
        let cfg = Self::load_from_with_warnings(&path, &mut warnings);
        for unk in &warnings.unknown_keys {
            eprintln!(
                "hop: warning: unknown config key '{}' in {}",
                unk,
                path.display()
            );
        }
        for iv in &warnings.invalid_values {
            eprintln!("hop: warning: {} in {}", iv, path.display());
        }
        (cfg, warnings)
    }

    pub fn load_from(path: &std::path::Path) -> Self {
        let mut warnings = ParseWarnings::default();
        Self::load_from_with_warnings(path, &mut warnings)
    }

    /// Tiny purpose-built parser — we only support the handful of keys we own.
    /// Grammar: `key = value`, `# comment`.
    pub fn load_from_with_warnings(path: &std::path::Path, warnings: &mut ParseWarnings) -> Self {
        let mut cfg = Config::default();

        let content = match std::fs::read_to_string(path) {
            Ok(s) => s,
            // A missing config file is the normal default state — stay silent.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Config::default(),
            Err(e) => {
                eprintln!(
                    "hop: warning: could not read config file '{}': {}",
                    path.display(),
                    e
                );
                return Config::default();
            }
        };

        for raw in content.lines() {
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
                "min_score" => {
                    if let Ok(n) = value.parse::<i64>() {
                        if n < 0 {
                            warnings.invalid_values.push(format!(
                                "invalid min_score value '{}' (expected non-negative integer)",
                                value
                            ));
                        } else {
                            cfg.min_score = n;
                        }
                    } else {
                        warnings.invalid_values.push(format!(
                            "invalid min_score value '{}' (expected integer)",
                            value
                        ));
                    }
                }
                "auto_prune_on_startup" => {
                    if let Ok(b) = value.parse::<bool>() {
                        cfg.auto_prune_on_startup = b;
                    } else {
                        warnings.invalid_values.push(format!(
                            "invalid auto_prune_on_startup value '{}' (expected boolean)",
                            value
                        ));
                    }
                }
                _ => {
                    warnings.unknown_keys.push(key.to_string());
                }
            }
        }
        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::score::MIN_SCORE;

    #[test]
    fn parse_defaults_when_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "").unwrap();
        let mut warnings = ParseWarnings::default();
        let c = Config::load_from_with_warnings(&path, &mut warnings);
        assert_eq!(c.min_score, MIN_SCORE);
        assert!(!c.auto_prune_on_startup);
        assert!(warnings.unknown_keys.is_empty());
    }

    #[test]
    fn parse_overrides() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            "# hi\nmin_score = 50\nauto_prune_on_startup = true\n",
        )
        .unwrap();
        let mut warnings = ParseWarnings::default();
        let c = Config::load_from_with_warnings(&path, &mut warnings);
        assert_eq!(c.min_score, 50);
        assert!(c.auto_prune_on_startup);
        assert!(warnings.unknown_keys.is_empty());
    }

    #[test]
    fn parse_warns_on_invalid_min_score_negative() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "min_score = -5\n").unwrap();
        let mut warnings = ParseWarnings::default();
        let c = Config::load_from_with_warnings(&path, &mut warnings);
        assert!(!warnings.invalid_values.is_empty());
        assert!(
            warnings
                .invalid_values
                .iter()
                .any(|w| w.contains("min_score")),
            "should warn about invalid min_score"
        );
        // Negative was not applied; should keep default
        assert_eq!(c.min_score, MIN_SCORE);
    }

    #[test]
    fn parse_warns_on_invalid_min_score_non_integer() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "min_score = hello\n").unwrap();
        let mut warnings = ParseWarnings::default();
        let c = Config::load_from_with_warnings(&path, &mut warnings);
        assert!(!warnings.invalid_values.is_empty());
        assert_eq!(c.min_score, MIN_SCORE);
    }

    #[test]
    fn parse_warns_on_unknown_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "unknown_key = 123\n").unwrap();
        let mut warnings = ParseWarnings::default();
        let _c = Config::load_from_with_warnings(&path, &mut warnings);
        assert!(
            warnings.unknown_keys.iter().any(|k| k == "unknown_key"),
            "should warn about unknown key"
        );
    }

    #[test]
    fn missing_config_uses_defaults_silently() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.toml");
        // Don't create the file
        let mut warnings = ParseWarnings::default();
        let c = Config::load_from_with_warnings(&path, &mut warnings);
        // Should fall back to defaults without any warnings
        assert_eq!(c.min_score, MIN_SCORE);
        assert!(warnings.unknown_keys.is_empty());
        assert!(warnings.invalid_values.is_empty());
    }

    #[test]
    fn malformed_toml_handled_gracefully() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bad.toml");
        // TOML parse error: missing closing bracket, invalid syntax
        std::fs::write(&path, "min_score = [[[\n").unwrap();
        let mut warnings = ParseWarnings::default();
        let c = Config::load_from_with_warnings(&path, &mut warnings);
        // Should fall back to defaults (parser skips bad lines)
        assert_eq!(c.min_score, MIN_SCORE);
    }
}

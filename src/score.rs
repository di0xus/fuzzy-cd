use std::path::Path;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use regex::Regex;

use crate::db::HistoryRow;

pub const MIN_SCORE: i64 = 20;

#[derive(Clone, Debug)]
pub struct Scored {
    pub path: String,
    pub score: i64,
    pub source: Source,
    pub matched_indices: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct ScoreBreakdown {
    pub path: String,
    pub total: i64,
    pub fuzzy: i64,
    pub visits: i64,
    pub recency: i64,
    pub git: i64,
    pub basename: i64,
    pub shortness: i64,
    pub source: Source,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Bookmark,
    History,
    Index,
}

pub struct Scorer {
    matcher: SkimMatcherV2,
    now: f64,
}

impl Scorer {
    pub fn new(now: f64) -> Self {
        Self {
            matcher: SkimMatcherV2::default().smart_case(),
            now,
        }
    }

    /// Score a history row.
    ///
    /// `query_lower` is the lowercased query, pre-computed once at the batch
    /// level to avoid O(n) redundant allocations. If `None`, it is computed
    /// internally (useful for single-call sites).
    pub fn score_history(
        &self,
        row: &HistoryRow,
        query: &str,
        query_lower: Option<&str>,
    ) -> Option<Scored> {
        let (fuzzy, indices) = self.matcher.fuzzy_indices(&row.path, query)?;
        let query_lower = query_lower.unwrap_or_else(|| query.to_lowercase().leak());
        let basename_lower = basename_lower(&row.path);
        let basename_bonus = basename_lower.contains(query_lower);

        let age_days = (self.now - row.last_visited) / 86_400.0;
        let recency = if age_days < 1.0 {
            3.0
        } else if age_days < 7.0 {
            2.0
        } else if age_days < 30.0 {
            1.0
        } else {
            0.5
        };
        let visit_boost = (row.visits as f64).sqrt().min(5.0);
        let depth = row.path.matches('/').count().max(1) as f64;
        let shortness = (10.0 / depth).max(1.0);

        let score = fuzzy
            + (visit_boost * 20.0) as i64
            + (recency * 15.0) as i64
            + if row.is_git_repo { 30 } else { 0 }
            + if basename_bonus { 40 } else { 0 }
            + (shortness * 5.0) as i64;

        Some(Scored {
            path: row.path.clone(),
            score,
            source: Source::History,
            matched_indices: indices,
        })
    }

    pub fn score_bookmark(&self, alias: &str, path: &str, query: &str) -> Option<Scored> {
        let (fuzzy, indices) = self.matcher.fuzzy_indices(alias, query)?;
        Some(Scored {
            path: path.to_string(),
            score: fuzzy * 3 + 100,
            source: Source::Bookmark,
            matched_indices: indices,
        })
    }

    pub fn score_indexed(&self, path: &str, query: &str) -> Option<Scored> {
        let (fuzzy, indices) = self.matcher.fuzzy_indices(path, query)?;
        let depth = path.matches('/').count().max(1) as f64;
        let shortness = (10.0 / depth).max(1.0);
        Some(Scored {
            path: path.to_string(),
            score: fuzzy / 2 + (shortness * 5.0) as i64,
            source: Source::Index,
            matched_indices: indices,
        })
    }

    /// Score a history row using a plain string query (no fuzzy metacharacters).
    /// Used for regex/negation paths after the filter has already matched.
    ///
    /// `pattern_lower` is the lowercased pattern, pre-computed once at the batch
    /// level to avoid O(n) redundant allocations. If `None`, computed internally.
    pub fn score_history_boosted(
        &self,
        row: &HistoryRow,
        pattern: &str,
        pattern_lower: Option<&str>,
    ) -> Option<Scored> {
        // No fuzzy match — we already know the pattern matched the path.
        // Score purely on bonus components; fuzzy=0 for neutral ranking.
        let basename_lower = basename_lower(&row.path);
        let pattern_lower = pattern_lower.unwrap_or_else(|| pattern.to_lowercase().leak());
        let basename_bonus = basename_lower.contains(pattern_lower);

        let age_days = (self.now - row.last_visited) / 86_400.0;
        let recency = if age_days < 1.0 {
            3.0
        } else if age_days < 7.0 {
            2.0
        } else if age_days < 30.0 {
            1.0
        } else {
            0.5
        };
        let visit_boost = (row.visits as f64).sqrt().min(5.0);
        let depth = row.path.matches('/').count().max(1) as f64;
        let shortness = (10.0 / depth).max(1.0);

        let score = (visit_boost * 20.0) as i64
            + (recency * 15.0) as i64
            + if row.is_git_repo { 30 } else { 0 }
            + if basename_bonus { 40 } else { 0 }
            + (shortness * 5.0) as i64;

        Some(Scored {
            path: row.path.clone(),
            score,
            source: Source::History,
            matched_indices: vec![],
        })
    }

    /// Breakdown variant of score_history_boosted.
    ///
    /// `pattern_lower` is the lowercased pattern, pre-computed once at the batch
    /// level. If `None`, computed internally.
    pub fn score_history_breakdown_boosted(
        &self,
        row: &HistoryRow,
        pattern: &str,
        pattern_lower: Option<&str>,
    ) -> Option<ScoreBreakdown> {
        let basename_lower = basename_lower(&row.path);
        let pattern_lower = pattern_lower.unwrap_or_else(|| pattern.to_lowercase().leak());
        let basename_bonus = basename_lower.contains(pattern_lower) as i64;

        let age_days = (self.now - row.last_visited) / 86_400.0;
        let recency = if age_days < 1.0 {
            3.0
        } else if age_days < 7.0 {
            2.0
        } else if age_days < 30.0 {
            1.0
        } else {
            0.5
        };
        let visit_boost = (row.visits as f64).sqrt().min(5.0);
        let depth = row.path.matches('/').count().max(1) as f64;
        let shortness = (10.0 / depth).max(1.0);
        let git = if row.is_git_repo { 30 } else { 0 };

        // For boosted scoring, fuzzy=0 since we already matched via pattern/regex
        let total = (visit_boost * 20.0) as i64
            + (recency * 15.0) as i64
            + git
            + basename_bonus * 40
            + (shortness * 5.0) as i64;

        Some(ScoreBreakdown {
            path: row.path.clone(),
            total,
            fuzzy: 0,
            visits: (visit_boost * 20.0) as i64,
            recency: (recency * 15.0) as i64,
            git,
            basename: basename_bonus * 40,
            shortness: (shortness * 5.0) as i64,
            source: Source::History,
        })
    }

    /// Score a history row and return per-component breakdown.
    ///
    /// `query_lower` is the lowercased query, pre-computed once at the batch
    /// level to avoid O(n) redundant allocations. If `None`, it is computed
    /// internally.
    pub fn score_history_breakdown(
        &self,
        row: &HistoryRow,
        query: &str,
        query_lower: Option<&str>,
    ) -> Option<ScoreBreakdown> {
        let (fuzzy, _) = self.matcher.fuzzy_indices(&row.path, query)?;
        let query_lower = query_lower.unwrap_or_else(|| query.to_lowercase().leak());
        let basename_lower = basename_lower(&row.path);
        let basename_bonus = basename_lower.contains(query_lower) as i64;

        let age_days = (self.now - row.last_visited) / 86_400.0;
        let recency = if age_days < 1.0 {
            3.0
        } else if age_days < 7.0 {
            2.0
        } else if age_days < 30.0 {
            1.0
        } else {
            0.5
        };
        let visit_boost = (row.visits as f64).sqrt().min(5.0);
        let depth = row.path.matches('/').count().max(1) as f64;
        let shortness = (10.0 / depth).max(1.0);
        let git = if row.is_git_repo { 30 } else { 0 };

        let total = fuzzy
            + (visit_boost * 20.0) as i64
            + (recency * 15.0) as i64
            + git
            + basename_bonus * 40
            + (shortness * 5.0) as i64;

        Some(ScoreBreakdown {
            path: row.path.clone(),
            total,
            fuzzy,
            visits: (visit_boost * 20.0) as i64,
            recency: (recency * 15.0) as i64,
            git,
            basename: basename_bonus * 40,
            shortness: (shortness * 5.0) as i64,
            source: Source::History,
        })
    }

    /// Score a bookmark and return per-component breakdown.
    pub fn score_bookmark_breakdown(
        &self,
        alias: &str,
        path: &str,
        query: &str,
    ) -> Option<ScoreBreakdown> {
        let (fuzzy, _) = self.matcher.fuzzy_indices(alias, query)?;
        let total = fuzzy * 3 + 100;
        Some(ScoreBreakdown {
            path: path.to_string(),
            total,
            fuzzy,
            visits: 0,
            recency: 0,
            git: 0,
            basename: 0,
            shortness: 0,
            source: Source::Bookmark,
        })
    }

    /// Score an indexed path and return per-component breakdown.
    pub fn score_indexed_breakdown(&self, path: &str, query: &str) -> Option<ScoreBreakdown> {
        let (fuzzy, _) = self.matcher.fuzzy_indices(path, query)?;
        let depth = path.matches('/').count().max(1) as f64;
        let shortness = (10.0 / depth).max(1.0);
        let total = fuzzy / 2 + (shortness * 5.0) as i64;
        Some(ScoreBreakdown {
            path: path.to_string(),
            total,
            fuzzy,
            visits: 0,
            recency: 0,
            git: 0,
            basename: 0,
            shortness: (shortness * 5.0) as i64,
            source: Source::Index,
        })
    }
}

pub fn basename_lower(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Returns the effective query string after stripping regex (^) or negation (!) prefix.
/// Also returns whether the query is a regex or negation type.
///
/// Returns `(effective, is_regex, is_negation)`. When the prefix character is
/// present but nothing (or only a trailing `/`) follows it (e.g. single `/` or
/// `!` or `//`), the query is treated as a literal — no regex or negation is
/// applied — so that searching for an actual `/` or `!` character works
/// correctly.
pub fn classify_query(query: &str) -> (&str, bool, bool) {
    let is_regex = query.starts_with('/');
    let is_negation = query.starts_with('!');
    if !is_regex && !is_negation {
        return (query, false, false);
    }
    // SAFETY: &query[1..] is only called when is_regex || is_negation is true,
    // which means query.len() >= 1. A single-character query "/" or "!" has
    // len == 1, so &query[1..] = &"" (valid empty slice, not out-of-bounds).
    let stripped = &query[1..];
    // If stripping the prefix leaves nothing (or only a lone "/" remains),
    // treat the whole query as a literal — it is not a meaningful pattern.
    if stripped.is_empty() || (stripped.len() == 1 && stripped.ends_with('/')) {
        return (query, false, false);
    }
    // Strip trailing '/' delimiter for regex patterns (e.g., "/foo/" → "foo")
    let effective = if is_regex && stripped.ends_with('/') {
        &stripped[..stripped.len() - 1]
    } else {
        stripped
    };
    (effective, is_regex, is_negation)
}

/// Returns true if the path matches the given pattern.
/// Regex matching uses the `regex` crate directly (linear-time NFA, no ReDoS risk).
/// Falls back to case-insensitive substring match on timeout or for non-regex patterns.
fn path_matches_with_regex(path: &str, regex: Option<&Regex>, pattern: &str) -> bool {
    let path_lower = path.to_lowercase();
    if let Some(re) = regex {
        if re.is_match(&path_lower) {
            return true;
        }
        // Fall back to substring match
        return path_lower.contains(&pattern.to_lowercase());
    }
    path_lower.contains(&pattern.to_lowercase())
}

/// Pre-filter candidates based on regex or negation query modifiers.
/// Returns the filtered list and whether any filtering was applied.
pub fn apply_query_filter(rows: &[HistoryRow], query: &str) -> (Vec<HistoryRow>, bool) {
    let (effective, is_regex, is_negation) = classify_query(query);
    if !is_regex && !is_negation {
        return (rows.to_vec(), false);
    }
    if effective.is_empty() {
        return (rows.to_vec(), false);
    }

    // Compile the regex ONCE, not per-row
    let compiled_regex = if is_regex {
        Regex::new(effective).ok()
    } else {
        None
    };

    let filtered: Vec<HistoryRow> = rows
        .iter()
        .filter(|row| {
            let matches = path_matches_with_regex(&row.path, compiled_regex.as_ref(), effective);
            if is_negation {
                !matches
            } else {
                matches
            }
        })
        .cloned()
        .collect();

    (filtered, true)
}

/// Score a list of history rows with optional regex/negation filtering.
/// Returns (scored candidates, filter_applied).
pub fn score_history_batch(
    scorer: &Scorer,
    rows: &[HistoryRow],
    query: &str,
) -> (Vec<Scored>, bool) {
    let (effective, is_regex, is_negation) = classify_query(query);
    let match_query = if is_regex || is_negation {
        effective
    } else {
        query
    };

    // Pre-compute once at batch level to avoid O(n) allocations
    let query_lower = query.to_lowercase();
    let effective_lower = effective.to_lowercase();

    let filtered: Vec<&HistoryRow> = if is_regex || is_negation {
        // Compile regex once, not per-row
        let compiled_regex = if is_regex {
            Regex::new(effective).ok()
        } else {
            None
        };
        rows.iter()
            .filter(|row| {
                let matches =
                    path_matches_with_regex(&row.path, compiled_regex.as_ref(), effective);
                if is_negation {
                    !matches
                } else {
                    matches
                }
            })
            .collect()
    } else {
        rows.iter().collect()
    };

    // For regex/negation queries, the raw query (e.g. "foo\d+") can't be fuzzy-matched.
    // We use a neutral fuzzy score for matched candidates; for plain queries,
    // use the full score_history path.
    let scored: Vec<Scored> = if is_regex || is_negation {
        filtered
            .iter()
            .filter_map(|row| scorer.score_history_boosted(row, effective, Some(&effective_lower)))
            .collect()
    } else {
        filtered
            .iter()
            .filter_map(|row| scorer.score_history(row, match_query, Some(&query_lower)))
            .collect()
    };
    let filter_applied = is_regex || is_negation;
    (scored, filter_applied)
}

/// Score a list of history rows and return per-component breakdowns.
/// For regex/negation queries, filtering is applied but the original query is
/// still used for fuzzy matching.
pub fn score_history_breakdown_batch(
    scorer: &Scorer,
    rows: &[HistoryRow],
    query: &str,
) -> Vec<ScoreBreakdown> {
    let (effective, is_regex, is_negation) = classify_query(query);
    // For breakdown display, use effective query for matching (strip prefix)
    let match_query = if is_regex || is_negation {
        effective
    } else {
        query
    };

    // Pre-compute once at batch level to avoid O(n) allocations
    let query_lower = query.to_lowercase();
    let effective_lower = effective.to_lowercase();

    let filtered: Vec<&HistoryRow> = if is_regex || is_negation {
        // Compile regex once, not per-row
        let compiled_regex = if is_regex {
            Regex::new(effective).ok()
        } else {
            None
        };
        rows.iter()
            .filter(|row| {
                let matches =
                    path_matches_with_regex(&row.path, compiled_regex.as_ref(), effective);
                if is_negation {
                    !matches
                } else {
                    matches
                }
            })
            .collect()
    } else {
        rows.iter().collect()
    };

    filtered
        .iter()
        .filter_map(|row| {
            if is_regex || is_negation {
                scorer.score_history_breakdown_boosted(row, effective, Some(&effective_lower))
            } else {
                scorer.score_history_breakdown(row, match_query, Some(&query_lower))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(path: &str, visits: i32, age_days: f64, git: bool) -> HistoryRow {
        HistoryRow {
            path: path.into(),
            visits,
            last_visited: 1_000_000.0 - age_days * 86_400.0,
            is_git_repo: git,
        }
    }

    #[test]
    fn basename_match_outranks_substring_in_middle() {
        let s = Scorer::new(1_000_000.0);
        let a = s
            .score_history(&row("/a/project", 1, 0.0, true), "project", None)
            .unwrap();
        let b = s
            .score_history(&row("/projectile/x/y", 1, 0.0, false), "project", None)
            .unwrap();
        assert!(
            a.score > b.score,
            "basename hit should beat deep non-basename"
        );
    }

    #[test]
    fn recent_outranks_old_same_visits() {
        let s = Scorer::new(1_000_000.0);
        let a = s
            .score_history(&row("/a/proj", 1, 0.0, false), "proj", None)
            .unwrap();
        let b = s
            .score_history(&row("/b/proj", 1, 45.0, false), "proj", None)
            .unwrap();
        assert!(a.score > b.score);
    }

    #[test]
    fn bookmark_outranks_history_for_same_query() {
        let s = Scorer::new(1_000_000.0);
        let bm = s.score_bookmark("proj", "/any/path", "proj").unwrap();
        let hist = s
            .score_history(&row("/x/proj", 1, 0.0, false), "proj", None)
            .unwrap();
        assert!(bm.score > hist.score);
    }

    #[test]
    fn no_match_returns_none() {
        let s = Scorer::new(1_000_000.0);
        assert!(s
            .score_history(&row("/a/b", 1, 0.0, false), "zzzzzz", None)
            .is_none());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // classify_query tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn classify_query_single_slash_is_literal() {
        // "/" with nothing after it — must NOT panic, treated as literal
        let (effective, is_regex, is_negation) = classify_query("/");
        assert_eq!(effective, "/");
        assert!(!is_regex, "single / must not be a regex");
        assert!(!is_negation, "single / must not be a negation");
    }

    #[test]
    fn classify_query_single_bang_is_literal() {
        // "!" with nothing after it — must NOT panic, treated as literal
        let (effective, is_regex, is_negation) = classify_query("!");
        assert_eq!(effective, "!");
        assert!(!is_regex);
        assert!(!is_negation, "single ! must not be a negation");
    }

    #[test]
    fn classify_query_double_slash_is_literal() {
        // "//" — after stripping '/' the effective query is empty → treat as literal
        let (effective, is_regex, is_negation) = classify_query("//");
        assert_eq!(effective, "//");
        assert!(!is_regex, "empty pattern after strip must not be a regex");
        assert!(
            !is_negation,
            "empty pattern after strip must not be a negation"
        );
    }

    #[test]
    fn classify_query_trailing_slash_stripped() {
        let (effective, is_regex, is_negation) = classify_query("/src/test/");
        assert_eq!(effective, "src/test");
        assert!(is_regex);
        assert!(!is_negation);
    }

    #[test]
    fn classify_query_negation_basic() {
        let (effective, is_regex, is_negation) = classify_query("!node_modules");
        assert_eq!(effective, "node_modules");
        assert!(!is_regex);
        assert!(is_negation);
    }

    #[test]
    fn classify_query_plain_query() {
        let (effective, is_regex, is_negation) = classify_query("work");
        assert_eq!(effective, "work");
        assert!(!is_regex);
        assert!(!is_negation);
    }

    #[test]
    fn classify_query_empty_is_literal() {
        let (effective, is_regex, is_negation) = classify_query("");
        assert_eq!(effective, "");
        assert!(!is_regex);
        assert!(!is_negation);
    }

    #[test]
    fn classify_query_regex_no_trailing_slash() {
        let (effective, is_regex, is_negation) = classify_query("/src/test.*");
        assert_eq!(effective, "src/test.*");
        assert!(is_regex);
        assert!(!is_negation);
    }

    #[test]
    fn classify_query_negation_only_pattern() {
        // "!!" → after first '!', stripped is "!" which is not empty
        // so negation still applies with effective "!"
        let (effective, is_regex, is_negation) = classify_query("!!");
        assert_eq!(effective, "!");
        assert!(!is_regex);
        assert!(is_negation, "!! should still be negation with effective !");
    }

    #[test]
    fn score_history_batch_early_termination_only_when_score_above_threshold() {
        // score_history_batch uses find_map to look for a candidate with score >= 20000.
        // - If found: returns early with vec![scored] (single result)
        // - If not found: falls through to filter_map and returns all matches
        //
        // The score formula: fuzzy + visit_boost(~14) + recency(~45) + git(0/30) +
        // basename_bonus(0/40) + shortness(~5). Typical max ~500.
        // 20000 threshold is never actually reached, so this is purely
        // a documentation test of the intended-but-never-triggered early-exit path.
        let s = Scorer::new(1_000_000.0);
        let rows = vec![
            row("/foo/bar", 5, 0.0, false),
            row("/bar/baz", 10, 0.0, false),
        ];

        // Query "bar" matches both rows; none can reach 20000, so find_map returns
        // None and we fall through to filter_map → all matches returned.
        let (scored, _) = score_history_batch(&s, &rows, "bar");
        assert_eq!(scored.len(), 2, "no perfect match → all matches returned");

        // Query "xyz" matches nothing; find_map returns None → filter_map → empty vec
        let (scored, _) = score_history_batch(&s, &rows, "xyz");
        assert_eq!(scored.len(), 0, "no match → empty result");
    }

    #[test]
    fn score_history_batch_returns_single_match_when_found() {
        // When only ONE row matches the query, we get exactly that one result
        // (the non-matching row produces None from score_history, so filter_map drops it).
        let s = Scorer::new(1_000_000.0);
        let rows = vec![
            row("/foo/bar", 5, 0.0, false),
            row("/baz/qux", 10, 0.0, false),
        ];

        // "qux" matches only /baz/qux
        let (scored, _) = score_history_batch(&s, &rows, "qux");
        assert_eq!(scored.len(), 1, "only one row matches 'qux'");
        assert!(scored[0].path.contains("qux"));
    }
}

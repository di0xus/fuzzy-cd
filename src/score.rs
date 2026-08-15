use std::borrow::Cow;
use std::path::Path;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use regex::Regex;

use crate::db::HistoryRow;

pub const MIN_SCORE: i64 = 20;

#[derive(Clone, Debug)]
pub struct Scored {
    pub path: String,
    /// Total score; equal to the sum of the components below.
    pub score: i64,
    pub fuzzy: i64,
    pub visits: i64,
    pub recency: i64,
    pub git: i64,
    pub basename: i64,
    pub shortness: i64,
    /// Bonus for dirs visited in the current shell session (HOP_SESSION).
    pub session: i64,
    pub source: Source,
    pub matched_indices: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Bookmark,
    History,
}

pub struct Scorer {
    matcher: SkimMatcherV2,
    now: f64,
    /// Shell-session start time from the `HOP_SESSION` env var (set by the
    /// init scripts). Dirs whose last visit is at/after this time get a bonus,
    /// so "just worked here" beats "visited a lot last month".
    session_start: Option<f64>,
}

impl Scorer {
    pub fn new(now: f64) -> Self {
        Self {
            matcher: SkimMatcherV2::default().smart_case(),
            now,
            session_start: std::env::var("HOP_SESSION")
                .ok()
                .and_then(|s| s.parse().ok()),
        }
    }

    /// Score a history row.
    ///
    /// `query_lower` is the lowercased query, pre-computed once at the batch
    /// level to avoid O(n) redundant allocations. If `None`, it is computed
    /// internally (useful for single-call sites).
    ///
    /// `fuzzy_score`: `Some(i64)` when fuzzy matching has already been performed
    /// externally (e.g., for regex/negation paths that matched via
    /// `path_matches_with_regex`); `None` to run `SkimMatcherV2::fuzzy_indices`
    /// inline and include the result in the total.
    /// Shared component computation for history scoring. `fuzzy` and
    /// `basename_bonus` differ per query type (single token, multi token,
    /// regex/negation); the recency/visit/git/session terms are the same.
    fn score_parts(
        &self,
        row: &HistoryRow,
        fuzzy: i64,
        basename_bonus: bool,
        indices: Vec<usize>,
    ) -> Scored {
        let age_days = (self.now - row.last_visited) / 86_400.0;
        let recency_f = if age_days < 1.0 {
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
        let shortness_f = (10.0 / depth).max(1.0);
        let git = if row.is_git_repo { 30 } else { 0 };
        let visits = (visit_boost * 20.0) as i64;
        let recency = (recency_f * 15.0) as i64;
        let basename = basename_bonus as i64 * 40;
        let shortness = (shortness_f * 5.0) as i64;
        let session = match self.session_start {
            Some(start) if row.last_visited >= start => 30,
            _ => 0,
        };

        let score = fuzzy + visits + recency + git + basename + shortness + session;

        Scored {
            path: row.path.clone(),
            score,
            fuzzy,
            visits,
            recency,
            git,
            basename,
            shortness,
            session,
            source: Source::History,
            matched_indices: indices,
        }
    }

    /// Score a history row against a single query string.
    ///
    /// `query_lower` is the lowercased query, pre-computed once at the batch
    /// level to avoid O(n) redundant allocations. If `None`, it is computed
    /// internally (useful for single-call sites).
    ///
    /// `fuzzy_score`: `Some(i64)` when fuzzy matching has already been performed
    /// externally (e.g., for regex/negation paths that matched via
    /// `path_matches_with_regex`); `None` to run `SkimMatcherV2::fuzzy_indices`
    /// inline and include the result in the total.
    pub fn score_history(
        &self,
        row: &HistoryRow,
        query: &str,
        query_lower: Option<&str>,
        fuzzy_score: Option<i64>,
    ) -> Option<Scored> {
        let (fuzzy, indices) = match fuzzy_score {
            Some(f) => (f, vec![]),
            None => self.matcher.fuzzy_indices(&row.path, query)?,
        };
        let query_lower: Cow<'_, str> = match query_lower {
            Some(s) => Cow::Borrowed(s),
            None => Cow::Owned(query.to_lowercase()),
        };
        let basename_bonus = basename_lower(&row.path).contains(query_lower.as_ref());
        Some(self.score_parts(row, fuzzy, basename_bonus, indices))
    }

    /// Score a row against a multi-token query (`"foo bar"`): every token must
    /// fuzzy-match the path, and their scores are summed.
    pub fn score_multi_token(&self, row: &HistoryRow, tokens: &[&str]) -> Option<Scored> {
        let mut fuzzy = 0i64;
        let mut indices = Vec::new();
        for t in tokens {
            let (f, idx) = self.matcher.fuzzy_indices(&row.path, t)?;
            fuzzy += f;
            indices.extend(idx);
        }
        let base = basename_lower(&row.path);
        let all_in_base = tokens.iter().all(|t| base.contains(&t.to_lowercase()));
        Some(self.score_parts(row, fuzzy, all_in_base, indices))
    }

    /// Score a row against a plain query, dispatching single- vs multi-token.
    /// Regex/negation queries are handled by the batch callers, not here.
    pub fn score_row(&self, row: &HistoryRow, query: &str) -> Option<Scored> {
        let tokens: Vec<&str> = query.split_whitespace().collect();
        if tokens.len() > 1 {
            self.score_multi_token(row, &tokens)
        } else {
            self.score_history(row, query, None, None)
        }
    }

    pub fn score_bookmark(&self, alias: &str, path: &str, query: &str) -> Option<Scored> {
        // Multi-token queries require every token to match the alias.
        let tokens: Vec<&str> = query.split_whitespace().collect();
        let (fuzzy, indices) = if tokens.len() > 1 {
            let mut f = 0i64;
            let mut idx = Vec::new();
            for t in &tokens {
                let (tf, ti) = self.matcher.fuzzy_indices(alias, t)?;
                f += tf;
                idx.extend(ti);
            }
            (f, idx)
        } else {
            self.matcher.fuzzy_indices(alias, query)?
        };
        Some(Scored {
            path: path.to_string(),
            score: fuzzy * 3 + 100,
            fuzzy,
            visits: 0,
            recency: 0,
            git: 0,
            basename: 0,
            shortness: 0,
            session: 0,
            source: Source::Bookmark,
            matched_indices: indices,
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
    let plain_tokens: Vec<&str> = match_query.split_whitespace().collect();

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
    // use the full score_history path (multi-token queries require every
    // token to match, with scores summed).
    let scored: Vec<Scored> = if is_regex || is_negation {
        filtered
            .iter()
            .filter_map(|row| scorer.score_history(row, effective, Some(&effective_lower), Some(0)))
            .collect()
    } else if plain_tokens.len() > 1 {
        filtered
            .iter()
            .filter_map(|row| scorer.score_multi_token(row, &plain_tokens))
            .collect()
    } else {
        filtered
            .iter()
            .filter_map(|row| scorer.score_history(row, match_query, Some(&query_lower), None))
            .collect()
    };
    let filter_applied = is_regex || is_negation;
    (scored, filter_applied)
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
            .score_history(&row("/a/project", 1, 0.0, true), "project", None, None)
            .unwrap();
        let b = s
            .score_history(
                &row("/projectile/x/y", 1, 0.0, false),
                "project",
                None,
                None,
            )
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
            .score_history(&row("/a/proj", 1, 0.0, false), "proj", None, None)
            .unwrap();
        let b = s
            .score_history(&row("/b/proj", 1, 45.0, false), "proj", None, None)
            .unwrap();
        assert!(a.score > b.score);
    }

    #[test]
    fn session_visit_gets_bonus() {
        // Session started 6h ago: a dir visited inside the session beats an
        // otherwise-identical dir visited before it, by exactly the bonus.
        let mut s = Scorer::new(1_000_000.0);
        s.session_start = Some(1_000_000.0 - 0.25 * 86_400.0);
        // 21.6h ago (before session) vs 4.8h ago (inside session); both in the
        // same recency bucket so only the session component differs.
        let before = s
            .score_history(&row("/a/proj", 5, 0.9, false), "proj", None, None)
            .unwrap();
        let during = s
            .score_history(&row("/b/proj", 5, 0.2, false), "proj", None, None)
            .unwrap();
        assert_eq!(before.session, 0);
        assert_eq!(during.session, 30);
        assert_eq!(during.score, before.score + 30);
    }

    #[test]
    fn bookmark_outranks_history_for_same_query() {
        let s = Scorer::new(1_000_000.0);
        let bm = s.score_bookmark("proj", "/any/path", "proj").unwrap();
        let hist = s
            .score_history(&row("/x/proj", 1, 0.0, false), "proj", None, None)
            .unwrap();
        assert!(bm.score > hist.score);
    }

    #[test]
    fn no_match_returns_none() {
        let s = Scorer::new(1_000_000.0);
        assert!(s
            .score_history(&row("/a/b", 1, 0.0, false), "zzzzzz", None, None)
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
    fn multi_token_requires_all_tokens() {
        let s = Scorer::new(1_000_000.0);
        let both = s
            .score_row(
                &row("/home/u/project/rust-app", 1, 0.0, false),
                "rust project",
            )
            .unwrap();
        let one = s.score_row(
            &row("/home/u/project/go-app", 1, 0.0, false),
            "rust project",
        );
        let none = s.score_row(&row("/home/u/other", 1, 0.0, false), "rust project");
        assert!(both.score > 0, "path with both tokens must match");
        assert!(one.is_none(), "missing a token must not match");
        assert!(none.is_none());

        // Single-token queries still match the plain way.
        let single = s.score_row(&row("/home/u/other", 1, 0.0, false), "other");
        assert!(single.is_some());
    }

    #[test]
    fn score_history_batch_returns_all_matching_rows() {
        // Plain queries return every matching row, not just the best.
        let s = Scorer::new(1_000_000.0);
        let rows = vec![
            row("/foo/bar", 5, 0.0, false),
            row("/bar/baz", 10, 0.0, false),
        ];

        // Query "bar" matches both rows.
        let (scored, _) = score_history_batch(&s, &rows, "bar");
        assert_eq!(scored.len(), 2, "no perfect match → all matches returned");

        // Query "xyz" matches nothing.
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

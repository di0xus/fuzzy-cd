use std::path::Path;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

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

    pub fn score_history(&self, row: &HistoryRow, query: &str) -> Option<Scored> {
        let (fuzzy, indices) = self.matcher.fuzzy_indices(&row.path, query)?;
        let basename_lower = basename_lower(&row.path);
        let query_lower = query.to_lowercase();
        let basename_bonus = basename_lower.contains(&query_lower);

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

    /// Score a history row and return per-component breakdown.
    pub fn score_history_breakdown(&self, row: &HistoryRow, query: &str) -> Option<ScoreBreakdown> {
        let (fuzzy, _) = self.matcher.fuzzy_indices(&row.path, query)?;
        let basename_lower = basename_lower(&row.path);
        let query_lower = query.to_lowercase();
        let basename_bonus = basename_lower.contains(&query_lower) as i64;

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

fn basename_lower(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default()
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
            .score_history(&row("/a/project", 1, 0.0, true), "project")
            .unwrap();
        let b = s
            .score_history(&row("/projectile/x/y", 1, 0.0, false), "project")
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
            .score_history(&row("/a/proj", 1, 0.0, false), "proj")
            .unwrap();
        let b = s
            .score_history(&row("/b/proj", 1, 45.0, false), "proj")
            .unwrap();
        assert!(a.score > b.score);
    }

    #[test]
    fn bookmark_outranks_history_for_same_query() {
        let s = Scorer::new(1_000_000.0);
        let bm = s.score_bookmark("proj", "/any/path", "proj").unwrap();
        let hist = s
            .score_history(&row("/x/proj", 1, 0.0, false), "proj")
            .unwrap();
        assert!(bm.score > hist.score);
    }

    #[test]
    fn no_match_returns_none() {
        let s = Scorer::new(1_000_000.0);
        assert!(s
            .score_history(&row("/a/b", 1, 0.0, false), "zzzzzz")
            .is_none());
    }
}

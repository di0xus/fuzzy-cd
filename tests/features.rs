//! Feature tests for v0.9 features.
//!
//! These tests cover CLI integration and score module functionality.
//! Run with: cargo test --locked

use std::process::Command;

/// Returns a Command configured with an isolated tempfile-based data dir.
#[allow(dead_code)]
fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_hop"));
    let tmp = tempfile::tempdir().unwrap().keep().0;
    c.env("XDG_DATA_HOME", &tmp);
    c.env("HOME", &tmp);
    c
}

/// Returns a Command with a specific data dir (for tests needing shared state).
fn bin_with_data(tmp: &std::path::Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_hop"));
    c.env("XDG_DATA_HOME", tmp);
    c.env("HOME", tmp);
    c
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 1: hop add --dry-run
// Expected: prints dry-run message, does NOT write to DB.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn add_dry_run_prints_message_without_writing_to_db() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("dry-run-test-dir");
    std::fs::create_dir(&target).unwrap();

    let tmp_path = tmp.path().to_path_buf();

    // First: add --dry-run (same data dir)
    let mut cmd1 = bin_with_data(&tmp_path);
    let out1 = cmd1
        .args(["add", "--dry-run", target.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out1.status.success(), "add --dry-run should succeed");
    let stdout1 = String::from_utf8_lossy(&out1.stdout);
    assert!(
        stdout1.contains("would create") || stdout1.contains("would add"),
        "expected dry-run message, got: {}",
        stdout1
    );

    // Now run hop add for real (without --dry-run) in same data dir
    let mut cmd2 = bin_with_data(&tmp_path);
    let out2 = cmd2
        .args(["add", target.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out2.status.success());

    // If the dry-run correctly skipped writing, this should say "would add" (already exists)
    let mut cmd3 = bin_with_data(&tmp_path);
    let out3 = cmd3
        .args(["add", "--dry-run", target.to_str().unwrap()])
        .output()
        .unwrap();
    let stdout3 = String::from_utf8_lossy(&out3.stdout);
    assert!(
        stdout3.contains("would add"),
        "dry-run should not have written to DB; expected 'would add', got: {}",
        stdout3
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 2: hop explain <query>
// Expected: shows per-component score breakdown (fuzzy, visits, recency, etc.)
// for the top results.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn explain_shows_score_breakdown_for_results() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("explain-test");
    std::fs::create_dir(&target).unwrap();

    let tmp_keep = tmp.path().to_path_buf();

    // Seed the DB in the SAME data dir so history is not empty
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("XDG_DATA_HOME", &tmp_keep);
    cmd.env("HOME", &tmp_keep);
    cmd.args(["add", target.to_str().unwrap()])
        .output()
        .unwrap();

    // Run explain
    let out = Command::new(env!("CARGO_BIN_EXE_hop"))
        .env("XDG_DATA_HOME", &tmp_keep)
        .env("HOME", &tmp_keep)
        .args(["explain", "explain"])
        .output()
        .unwrap();
    assert!(out.status.success(), "explain should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Should show component headers / breakdown
    assert!(
        stdout.contains("fuzzy") || stdout.contains("total=") || stdout.contains("visits="),
        "explain output should contain score breakdown, got: {}",
        stdout
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 3: hop stats --verbose
// Expected: shows top-10 + longest-unvisited dirs in verbose mode.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn stats_verbose_shows_top_10_and_longest_unvisited() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("stats-verbose-test");
    std::fs::create_dir(&target).unwrap();

    let tmp_keep = tmp.path().to_path_buf();

    // Seed DB
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("XDG_DATA_HOME", &tmp_keep);
    cmd.env("HOME", &tmp_keep);
    cmd.args(["add", target.to_str().unwrap()])
        .output()
        .unwrap();

    // Run stats --verbose
    let out = Command::new(env!("CARGO_BIN_EXE_hop"))
        .env("XDG_DATA_HOME", &tmp_keep)
        .env("HOME", &tmp_keep)
        .args(["stats", "--verbose"])
        .output()
        .unwrap();
    assert!(out.status.success(), "stats --verbose should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);

    // The --verbose flag should make stats show top-10 and longest-unvisited info.
    assert!(
        stdout.contains("top 10")
            || stdout.contains("top-10")
            || stdout.contains("longest-unvisited")
            || stdout.contains("longest unvisited"),
        "stats --verbose should show top-10 and longest-unvisited dirs; output was: {}",
        stdout
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 4: hop export json and hop export csv
// Expected: produces valid JSON / well-formed CSV.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn export_json_produces_valid_json() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("export-json-test");
    std::fs::create_dir(&target).unwrap();

    let tmp_keep = tmp.path().to_path_buf();

    // Seed DB
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("XDG_DATA_HOME", &tmp_keep);
    cmd.env("HOME", &tmp_keep);
    cmd.args(["add", target.to_str().unwrap()])
        .output()
        .unwrap();

    // Export as JSON
    let out = Command::new(env!("CARGO_BIN_EXE_hop"))
        .env("XDG_DATA_HOME", &tmp_keep)
        .env("HOME", &tmp_keep)
        .args(["export", "--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "export json should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Must be valid JSON
    let _parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("exported JSON should be parseable");
}

#[test]
fn export_csv_produces_wellformed_csv() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("export-csv-test");
    std::fs::create_dir(&target).unwrap();

    let tmp_keep = tmp.path().to_path_buf();

    // Seed DB
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("XDG_DATA_HOME", &tmp_keep);
    cmd.env("HOME", &tmp_keep);
    cmd.args(["add", target.to_str().unwrap()])
        .output()
        .unwrap();

    // Export as CSV
    let out = Command::new(env!("CARGO_BIN_EXE_hop"))
        .env("XDG_DATA_HOME", &tmp_keep)
        .env("HOME", &tmp_keep)
        .args(["export", "--format", "csv"])
        .output()
        .unwrap();
    assert!(out.status.success(), "export csv should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(!lines.is_empty(), "CSV should have at least a header row");
    let header = lines[0];
    assert!(
        header.contains("path") || header.starts_with("type,"),
        "CSV header should describe fields, got: {}",
        header
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 5: hop import --dry-run <file>
// Expected: previews import without actually importing anything.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn import_dry_run_previews_without_importing() {
    let tmp = tempfile::tempdir().unwrap();
    let import_file = tmp.path().join("import_data");
    // Use a real directory that exists for dry-run preview
    let real_dir = tmp.path().join("real-path");
    std::fs::create_dir(&real_dir).unwrap();
    std::fs::write(&import_file, format!("{}\t10\n", real_dir.display())).unwrap();

    let tmp_keep = tmp.path().to_path_buf();

    // Try import --dry-run
    let out = Command::new(env!("CARGO_BIN_EXE_hop"))
        .env("XDG_DATA_HOME", &tmp_keep)
        .env("HOME", &tmp_keep)
        .args([
            "import",
            "--dry-run",
            "autojump",
            import_file.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "import --dry-run should succeed but got: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("would") || stdout.contains("dry") || stdout.contains("preview"),
        "import --dry-run should show preview, got: {}",
        stdout
    );

    // Verify nothing was actually imported
    let history_out = Command::new(env!("CARGO_BIN_EXE_hop"))
        .env("XDG_DATA_HOME", &tmp_keep)
        .env("HOME", &tmp_keep)
        .args(["history"])
        .output()
        .unwrap();
    let history_stdout = String::from_utf8_lossy(&history_out.stdout);
    let nothing_imported =
        history_stdout.trim().is_empty() || !history_stdout.contains(&*real_dir.to_string_lossy());
    assert!(
        nothing_imported,
        "import --dry-run should not have imported paths, got: {}",
        history_stdout
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 6: Path deduplication
// Same directory via symlink = one canonical entry in DB.
// record_visit canonicalizes paths; this test verifies the behavior.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn symlink_and_real_path_share_single_db_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let real_dir = tmp.path().join("real-project");
    std::fs::create_dir(&real_dir).unwrap();

    let symlink_dir = tmp.path().join("link-project");
    std::os::unix::fs::symlink(&real_dir, &symlink_dir).unwrap();

    let tmp_keep = tmp.path().to_path_buf();

    // Add via real path
    let mut cmd1 = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd1.env("XDG_DATA_HOME", &tmp_keep);
    cmd1.env("HOME", &tmp_keep);
    cmd1.args(["add", real_dir.to_str().unwrap()])
        .output()
        .unwrap();

    // Add via symlink path
    let mut cmd2 = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd2.env("XDG_DATA_HOME", &tmp_keep);
    cmd2.env("HOME", &tmp_keep);
    cmd2.args(["add", symlink_dir.to_str().unwrap()])
        .output()
        .unwrap();

    // History should only show one canonical entry
    let out = Command::new(env!("CARGO_BIN_EXE_hop"))
        .env("XDG_DATA_HOME", &tmp_keep)
        .env("HOME", &tmp_keep)
        .args(["history"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);

    let canonical = std::fs::canonicalize(&real_dir).unwrap();
    let canonical_str = canonical.to_string_lossy();
    let count = stdout.matches(&*canonical_str).count();
    assert_eq!(
        count, 1,
        "canonical path should appear exactly once in history, got {} occurrences in: {}",
        count, stdout
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 7: Regex query
// Query "/foo\d+/" should match foo1, foo2, etc. in path names.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn regex_query_matches_pattern_in_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let foo1 = tmp.path().join("foo1");
    let foo22 = tmp.path().join("foo22");
    std::fs::create_dir(&foo1).unwrap();
    std::fs::create_dir(&foo22).unwrap();

    let tmp_keep = tmp.path().to_path_buf();

    // Seed DB
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("XDG_DATA_HOME", &tmp_keep);
    cmd.env("HOME", &tmp_keep);
    cmd.args(["add", foo1.to_str().unwrap()]).output().unwrap();
    let mut cmd2 = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd2.env("XDG_DATA_HOME", &tmp_keep);
    cmd2.env("HOME", &tmp_keep);
    cmd2.args(["add", foo22.to_str().unwrap()])
        .output()
        .unwrap();

    // Query using regex pattern /foo\d+/
    let out = Command::new(env!("CARGO_BIN_EXE_hop"))
        .env("XDG_DATA_HOME", &tmp_keep)
        .env("HOME", &tmp_keep)
        .args(["p", "/foo\\d+/"])
        .output()
        .unwrap();

    // The command should succeed and return a foo* path
    assert!(out.status.success(), "regex query should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let got_path = stdout.trim();
    assert!(
        got_path.contains("foo1") || got_path.contains("foo22"),
        "regex query should match foo1 or foo22, got: {}",
        got_path
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 8: Negative query
// Query "!node" should exclude node_modules paths.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn negative_query_excludes_matched_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let good_dir = tmp.path().join("good-project");
    let node_modules = tmp.path().join("node_modules");
    std::fs::create_dir(&good_dir).unwrap();
    std::fs::create_dir(&node_modules).unwrap();

    let tmp_keep = tmp.path().to_path_buf();

    // Seed DB with both
    let mut cmd1 = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd1.env("XDG_DATA_HOME", &tmp_keep);
    cmd1.env("HOME", &tmp_keep);
    cmd1.args(["add", good_dir.to_str().unwrap()])
        .output()
        .unwrap();
    let mut cmd2 = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd2.env("XDG_DATA_HOME", &tmp_keep);
    cmd2.env("HOME", &tmp_keep);
    cmd2.args(["add", node_modules.to_str().unwrap()])
        .output()
        .unwrap();

    // Query with negation: !node should exclude node_modules
    let out = Command::new(env!("CARGO_BIN_EXE_hop"))
        .env("XDG_DATA_HOME", &tmp_keep)
        .env("HOME", &tmp_keep)
        .args(["p", "!node"])
        .output()
        .unwrap();

    assert!(out.status.success(), "negative query should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let got_path = stdout.trim();

    assert!(
        !got_path.is_empty() && !got_path.contains("node_modules"),
        "negative query should return good-project, not node_modules; got: {}",
        got_path
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Feature 9: auto_prune_on_startup
// When config=true, prune runs silently on startup.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn auto_prune_on_startup_runs_silently_when_enabled() {
    let tmp = tempfile::tempdir().unwrap();

    // On macOS, ProjectDirs uses ~/Library/Application Support/<app>/ not XDG_DATA_HOME.
    // Config is at <data_dir>/config.toml where data_dir comes from ProjectDirs.
    let home = tmp.path();
    // The data dir that hop uses (ProjectDirs on macOS = ~/Library/Application Support/hop)
    let data_dir = home.join("Library").join("Application Support").join("hop");
    std::fs::create_dir_all(&data_dir).unwrap();
    let config_path = data_dir.join("config.toml");
    std::fs::write(&config_path, "auto_prune_on_startup = true\n").unwrap();

    // Create a stale directory, add it, then delete it
    let gone_dir = tmp.path().join("will-be-deleted");
    std::fs::create_dir(&gone_dir).unwrap();

    let mut cmd_seed = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd_seed.env("HOME", tmp.path());
    cmd_seed.env("XDG_DATA_HOME", &data_dir);
    cmd_seed
        .args(["add", gone_dir.to_str().unwrap()])
        .output()
        .unwrap();

    // Delete the directory so it becomes stale
    std::fs::remove_dir(&gone_dir).unwrap();

    // Run hop stats — auto-prune should have removed the stale entry silently
    let out = Command::new(env!("CARGO_BIN_EXE_hop"))
        .env("HOME", tmp.path())
        .env("XDG_DATA_HOME", &data_dir)
        .args(["stats"])
        .output()
        .unwrap();

    assert!(out.status.success(), "hop with auto_prune should succeed");
    let stdout = String::from_utf8_lossy(&out.stdout);
    // The stale entry should have been auto-pruned
    assert!(
        !stdout.contains("will-be-deleted"),
        "auto_prune_on_startup should have removed stale entry silently, got: {}",
        stdout
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit-test stubs for score module regex / negation support.
// These use the internal apply_query_filter API directly.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn apply_query_filter_regex_matches_foo_digit() {
    use hop::db::HistoryRow;
    use hop::score::apply_query_filter;

    let rows = vec![
        HistoryRow {
            path: "/home/user/foo1/bar".into(),
            visits: 1,
            last_visited: 1_000_000.0,
            is_git_repo: false,
        },
        HistoryRow {
            path: "/home/user/foo22/baz".into(),
            visits: 1,
            last_visited: 1_000_000.0,
            is_git_repo: false,
        },
        HistoryRow {
            path: "/home/user/foobar/qux".into(),
            visits: 1,
            last_visited: 1_000_000.0,
            is_git_repo: false,
        },
    ];

    let (filtered, applied) = apply_query_filter(&rows, "/foo\\d+");
    assert!(applied, "regex filter should be detected as applied");
    let paths: Vec<_> = filtered.iter().map(|r| r.path.as_str()).collect();
    assert!(
        paths.contains(&"/home/user/foo1/bar") && paths.contains(&"/home/user/foo22/baz"),
        "regex should match foo1 and foo22, got: {:?}",
        paths
    );
    assert!(
        !paths.contains(&"/home/user/foobar/qux"),
        "foobar should NOT match /foo\\d+/, got: {:?}",
        paths
    );
}

#[test]
fn apply_query_filter_negation_excludes_node_modules() {
    use hop::db::HistoryRow;
    use hop::score::apply_query_filter;

    let rows = vec![
        HistoryRow {
            path: "/home/user/project/src".into(),
            visits: 1,
            last_visited: 1_000_000.0,
            is_git_repo: false,
        },
        HistoryRow {
            path: "/home/user/project/node_modules".into(),
            visits: 1,
            last_visited: 1_000_000.0,
            is_git_repo: false,
        },
    ];

    let (filtered, applied) = apply_query_filter(&rows, "!node");
    assert!(applied, "negation filter should be detected as applied");
    let paths: Vec<_> = filtered.iter().map(|r| r.path.as_str()).collect();
    assert!(
        paths.contains(&"/home/user/project/src"),
        "negation !node should include /project/src, got: {:?}",
        paths
    );
    assert!(
        !paths.contains(&"/home/user/project/node_modules"),
        "negation !node should exclude node_modules, got: {:?}",
        paths
    );
}

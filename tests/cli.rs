use std::process::Command;

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_hop"));
    let tmp = tempfile::tempdir().unwrap().keep();
    c.env("XDG_DATA_HOME", &tmp);
    c.env("HOME", &tmp);
    c
}

#[test]
fn help_works() {
    let out = bin().arg("--help").output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("hop"));
    assert!(s.contains("init"));
    assert!(s.contains("doctor"));
}

#[test]
fn init_emits_scripts() {
    for shell in ["bash", "zsh", "fish"] {
        let out = bin().arg("init").arg(shell).output().unwrap();
        assert!(out.status.success(), "init {shell} failed");
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("command hop"), "init {shell} missing hop call");
        assert!(
            s.contains("HOP_SESSION"),
            "init {shell} should export HOP_SESSION"
        );
        let has_h = if shell == "fish" {
            s.contains("function h")
        } else {
            s.contains("h()")
        };
        assert!(has_h, "init {shell} missing h function");
    }
}

#[test]
fn init_rejects_unknown_shell() {
    let out = bin().arg("init").arg("unknownshell").output().unwrap();
    assert!(!out.status.success());
}

#[test]
fn init_shell_flag_works() {
    let out = bin().args(["init", "--shell", "fish"]).output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("function h"));
}

#[test]
fn init_verify_runs() {
    let out = bin().args(["init", "--verify"]).output().unwrap();
    // succeeds or fails depending on $SHELL; we only assert it emits lines.
    let s = String::from_utf8_lossy(&out.stdout);
    let e = String::from_utf8_lossy(&out.stderr);
    assert!(!s.is_empty() || !e.is_empty());
}

#[test]
fn completions_emits_scripts() {
    for shell in ["bash", "zsh", "fish"] {
        let out = bin().args(["completions", shell]).output().unwrap();
        assert!(out.status.success(), "completions {shell} failed");
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("hop"), "completions {shell} missing hop refs");
    }
}

#[test]
fn completions_rejects_unknown_shell() {
    let out = bin().args(["completions", "unknownshell"]).output().unwrap();
    assert!(!out.status.success());
}

#[test]
fn add_then_pick_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("my-proj");
    std::fs::create_dir(&target).unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    let add = cmd
        .args(["add", target.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(add.status.success());

    let mut cmd2 = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd2.env("HOME", tmp.path())
        .env("XDG_DATA_HOME", tmp.path());
    let pick = cmd2.args(["p", "proj"]).output().unwrap();
    // hop now stores canonical paths; canonicalize target before comparing
    let target_canonical = std::fs::canonicalize(target).unwrap();
    let out = String::from_utf8_lossy(&pick.stdout);
    assert!(
        out.trim() == target_canonical.to_string_lossy(),
        "expected {}, got {}",
        target_canonical.display(),
        out,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Regression: `hop add -- <path>` (the exact form every shell hook emits)
// used to fail with "not a directory: --", so history was never recorded.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn add_with_separator_records_visit() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("sep-dir");
    std::fs::create_dir(&target).unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    let add = cmd
        .args(["add", "--", target.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        add.status.success(),
        "hop add -- <path> should succeed, stderr: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    let mut cmd2 = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd2.env("HOME", tmp.path())
        .env("XDG_DATA_HOME", tmp.path());
    let hist = cmd2.arg("history").output().unwrap();
    let out = String::from_utf8_lossy(&hist.stdout);
    let canonical = std::fs::canonicalize(&target).unwrap();
    assert!(
        out.contains(canonical.to_str().unwrap()),
        "history should contain the added dir, got: {}",
        out
    );
}

#[test]
fn add_with_separator_and_dry_run() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("sep-dry");
    std::fs::create_dir(&target).unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    let out = cmd
        .args(["add", "--dry-run", "--", target.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("would create"),
        "dry-run with separator should preview, got: {}",
        s
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Regression: `hop rm <path>` compared the raw argument against canonical
// history paths, so removal failed for symlinked paths (e.g. /var on macOS).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn rm_accepts_symlink_path() {
    let tmp = tempfile::tempdir().unwrap();
    let real = tmp.path().join("real-dir");
    std::fs::create_dir(&real).unwrap();
    let link = tmp.path().join("link-dir");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &link).unwrap();
    #[cfg(not(unix))]
    let _ = &link;

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    let add = cmd
        .args(["add", real.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(add.status.success());

    let mut cmd2 = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd2.env("HOME", tmp.path())
        .env("XDG_DATA_HOME", tmp.path());
    let rm = cmd2
        .args(["rm", "--", link.to_str().unwrap()])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&rm.stdout);
    assert!(
        rm.status.success() && stdout.contains("removed"),
        "rm via symlink should succeed, got: {} {}",
        rm.status,
        stdout
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Regression: `hop book --json list` used to create a bookmark aliased
// "--json" instead of listing.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn book_json_flag_before_subcommand_lists() {
    let tmp = tempfile::tempdir().unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    let out = cmd.args(["book", "--json", "list"]).output().unwrap();
    assert!(
        out.status.success(),
        "book --json list should succeed, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.trim_start().starts_with('['),
        "book --json list should print a JSON array, got: {}",
        s
    );

    // No bogus bookmark named "--json" may exist.
    let mut cmd2 = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd2.env("HOME", tmp.path())
        .env("XDG_DATA_HOME", tmp.path());
    let list = cmd2.args(["book", "list"]).output().unwrap();
    let l = String::from_utf8_lossy(&list.stdout);
    assert!(
        !l.contains("--json"),
        "no --json bookmark should exist, got: {}",
        l
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Regression: `hop import --dry-run <source>` (missing file) used to panic
// with an index-out-of-bounds instead of printing a usage error.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn import_dry_run_missing_file_is_usage_error() {
    let tmp = tempfile::tempdir().unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    let out = cmd
        .args(["import", "--dry-run", "autojump"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "should be a usage error, not a panic"
    );
    let e = String::from_utf8_lossy(&out.stderr);
    assert!(
        e.contains("Usage"),
        "stderr should show usage, got: {}",
        e
    );
    assert!(!e.contains("panicked"), "must not panic, got: {}", e);
}

// ─────────────────────────────────────────────────────────────────────────────
// New: `hop -` prints the previous directory (cd - semantics across sessions).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn dash_jumps_to_previous_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let dir_a = tmp.path().join("dir-a");
    let dir_b = tmp.path().join("dir-b");
    std::fs::create_dir(&dir_a).unwrap();
    std::fs::create_dir(&dir_b).unwrap();

    let mut seed = Command::new(env!("CARGO_BIN_EXE_hop"));
    seed.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    assert!(seed
        .args(["add", dir_a.to_str().unwrap()])
        .output()
        .unwrap()
        .status
        .success());
    let mut seed2 = Command::new(env!("CARGO_BIN_EXE_hop"));
    seed2.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    assert!(seed2
        .args(["add", dir_b.to_str().unwrap()])
        .output()
        .unwrap()
        .status
        .success());

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    let out = cmd.arg("-").output().unwrap();
    assert!(out.status.success(), "hop - should succeed");
    let got = String::from_utf8_lossy(&out.stdout);
    let canon_a = std::fs::canonicalize(&dir_a).unwrap();
    assert_eq!(
        got.trim(),
        canon_a.to_string_lossy(),
        "hop - should print the previous dir"
    );
}

#[test]
fn dash_with_single_dir_prints_hint() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("only-dir");
    std::fs::create_dir(&dir).unwrap();

    let mut seed = Command::new(env!("CARGO_BIN_EXE_hop"));
    seed.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    assert!(seed
        .args(["add", dir.to_str().unwrap()])
        .output()
        .unwrap()
        .status
        .success());

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    let out = cmd.arg("-").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let e = String::from_utf8_lossy(&out.stderr);
    assert!(e.contains("no previous directory"), "got: {}", e);
}

// ─────────────────────────────────────────────────────────────────────────────
// New: empty-history hint on failed jumps (cold start).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn empty_history_prints_hint_on_miss() {
    let tmp = tempfile::tempdir().unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    let out = cmd.args(["p", "anything"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let e = String::from_utf8_lossy(&out.stderr);
    assert!(
        e.contains("no history yet"),
        "cold start should hint, got: {}",
        e
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// New: `hop forget` with no query must not hang or panic in a non-tty; the
// picker returns None, which is a clean exit 1.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn forget_without_query_is_safe_in_non_tty() {
    let tmp = tempfile::tempdir().unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    let out = cmd.arg("forget").output().unwrap();
    assert!(
        !out.status.success() && out.status.code() != Some(101),
        "forget in non-tty should exit cleanly, got {:?}",
        out.status.code()
    );
    let e = String::from_utf8_lossy(&out.stderr);
    assert!(!e.contains("panicked"), "must not panic, got: {}", e);
}

// ─────────────────────────────────────────────────────────────────────────────
// Ergonomics: literal paths, ~ expansion, .., multi-token AND, subdir / search.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn tilde_path_jumps_like_cd() {
    let tmp = tempfile::tempdir().unwrap();
    let sub = tmp.path().join("tilde-dir");
    std::fs::create_dir(&sub).unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    let out = cmd.args(["p", "~/tilde-dir"]).output().unwrap();
    assert!(out.status.success(), "hop p ~/dir should succeed");
    let got = String::from_utf8_lossy(&out.stdout);
    let canon = std::fs::canonicalize(&sub).unwrap();
    assert_eq!(got.trim(), canon.to_string_lossy());
}

#[test]
fn dotdot_resolves_to_parent() {
    let tmp = tempfile::tempdir().unwrap();
    let child = tmp.path().join("child");
    std::fs::create_dir(&child).unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path())
        .env("XDG_DATA_HOME", tmp.path())
        .current_dir(&child);
    let out = cmd.arg("..").output().unwrap();
    assert!(out.status.success(), "hop .. should succeed");
    let got = String::from_utf8_lossy(&out.stdout);
    let canon = std::fs::canonicalize(tmp.path()).unwrap();
    assert_eq!(got.trim(), canon.to_string_lossy());
}

#[test]
fn multi_token_query_requires_all_words() {
    let tmp = tempfile::tempdir().unwrap();
    let mine = tmp.path().join("my-project");
    let other = tmp.path().join("project-other");
    std::fs::create_dir(&mine).unwrap();
    std::fs::create_dir(&other).unwrap();

    let mut seed = Command::new(env!("CARGO_BIN_EXE_hop"));
    seed.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    seed.args(["add", mine.to_str().unwrap()]).output().unwrap();
    seed.args(["add", other.to_str().unwrap()]).output().unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path()).env("XDG_DATA_HOME", tmp.path());
    let hit = cmd.args(["p", "my project"]).output().unwrap();
    assert!(hit.status.success());
    let got = String::from_utf8_lossy(&hit.stdout);
    let canon = std::fs::canonicalize(&mine).unwrap();
    assert_eq!(
        got.trim(),
        canon.to_string_lossy(),
        "both words must match; my-project has the basename bonus"
    );
}

#[test]
fn subdir_slash_finds_cwd_child() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("alpha-one")).unwrap();
    std::fs::create_dir(tmp.path().join("alpha-two")).unwrap();
    std::fs::create_dir(tmp.path().join("beta")).unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path())
        .env("XDG_DATA_HOME", tmp.path())
        .current_dir(tmp.path());
    let out = cmd.args(["p", "alpha /"]).output().unwrap();
    assert!(out.status.success(), "hop p 'alpha /' should succeed");
    let got = String::from_utf8_lossy(&out.stdout);
    let canon = std::fs::canonicalize(tmp.path()).unwrap();
    assert!(
        got.trim().starts_with(&canon.to_string_lossy().into_owned())
            && (got.contains("alpha-one") || got.contains("alpha-two")),
        "should resolve to an alpha-* child of cwd, got: {}",
        got
    );
    assert!(!got.contains("beta"), "beta must not match 'alpha /'");
}

#[test]
fn subdir_slash_missing_prefix_prints_no_match() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("alpha")).unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hop"));
    cmd.env("HOME", tmp.path())
        .env("XDG_DATA_HOME", tmp.path())
        .current_dir(tmp.path());
    let out = cmd.args(["p", "zzz /"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
}

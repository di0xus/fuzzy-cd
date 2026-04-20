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
        assert!(s.contains("__hop_cd"), "init {shell} missing cd wrapper");
    }
}

#[test]
fn init_rejects_unknown_shell() {
    let out = bin().arg("init").arg("nushell").output().unwrap();
    assert!(!out.status.success());
}

#[test]
fn init_shell_flag_works() {
    let out = bin().args(["init", "--shell", "fish"]).output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("__hop_cd"));
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
    let out = bin().args(["completions", "nushell"]).output().unwrap();
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
    let out = String::from_utf8_lossy(&pick.stdout);
    assert!(
        out.trim() == target.to_string_lossy(),
        "expected {}, got {}",
        target.display(),
        out
    );
}

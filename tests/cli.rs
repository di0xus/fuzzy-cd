use std::process::Command;

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_fuzzy-cd"));
    // isolate state per test run
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
    assert!(s.contains("fuzzy-cd"));
    assert!(s.contains("init"));
    assert!(s.contains("doctor"));
}

#[test]
fn init_emits_scripts() {
    for shell in ["bash", "zsh", "fish"] {
        let out = bin().arg("init").arg(shell).output().unwrap();
        assert!(out.status.success(), "init {shell} failed");
        let s = String::from_utf8_lossy(&out.stdout);
        assert!(s.contains("fcd"), "init {shell} missing fcd");
    }
}

#[test]
fn init_rejects_unknown_shell() {
    let out = bin().arg("init").arg("nushell").output().unwrap();
    assert!(!out.status.success());
}

#[test]
fn add_then_pick_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("my-proj");
    std::fs::create_dir(&target).unwrap();

    let mut cmd = bin();
    cmd.env("HOME", tmp.path());
    let add = cmd
        .args(["add", target.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(add.status.success());

    // reuse same state dir by passing same HOME
    let mut cmd2 = Command::new(env!("CARGO_BIN_EXE_fuzzy-cd"));
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

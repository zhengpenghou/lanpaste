use std::process::Command;

use predicates::prelude::*;

#[test]
fn help_shows_serve_and_defaults() {
    let out = Command::new(env!("CARGO_BIN_EXE_lanpaste"))
        .arg("serve")
        .arg("--help")
        .output()
        .expect("run help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(predicate::str::contains("0.0.0.0:8090").eval(&stdout));
    assert!(predicate::str::contains("--dir <DIR>").eval(&stdout));
    assert!(predicate::str::contains("--push <PUSH>").eval(&stdout));
}

#[test]
fn fails_when_git_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = Command::new(env!("CARGO_BIN_EXE_lanpaste"))
        .env("PATH", "")
        .arg("serve")
        .arg("--dir")
        .arg(dir.path())
        .output()
        .expect("run");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(predicate::str::contains("git is required").eval(&stderr));
}

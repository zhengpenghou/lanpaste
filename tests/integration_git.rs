use std::process::Command;

use lanpaste::{
    config::{PushMode, ServeCmd},
    preflight,
};

fn cfg(base: &std::path::Path) -> ServeCmd {
    ServeCmd {
        dir: base.to_path_buf(),
        bind: "127.0.0.1:0".parse().expect("bind"),
        token: None,
        max_bytes: 1024 * 1024,
        push: PushMode::Off,
        remote: "origin".to_string(),
        allow_cidr: vec![],
        git_author_name: "LAN Paste".to_string(),
        git_author_email: "paste@lan".to_string(),
    }
}

#[test]
fn bootstrap_repo_creates_initial_commit_and_layout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = cfg(dir.path());
    preflight::run_preflight(&cfg).expect("preflight");

    let repo = dir.path().join("repo");
    assert!(repo.join(".git").exists());
    assert!(repo.join("README.md").exists());
    assert!(repo.join(".gitignore").exists());
    assert!(repo.join("pastes").exists());
    assert!(repo.join("meta").exists());

    let output = Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .current_dir(repo)
        .output()
        .expect("git rev-list");
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u32>()
            .expect("count")
            >= 1
    );
}

#[test]
fn single_instance_lock_blocks_second_state() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = cfg(dir.path());
    preflight::run_preflight(&cfg).expect("preflight");
    let _state1 = preflight::build_state(cfg.clone()).expect("state1");
    let state2 = preflight::build_state(cfg);
    assert!(state2.is_err());
}

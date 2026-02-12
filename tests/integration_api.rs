use std::sync::Arc;

use axum::http::StatusCode;
use axum_test::TestServer;
use lanpaste::{
    config::{PushMode, ServeCmd},
    http,
    preflight,
};

fn test_cfg(base: &std::path::Path) -> ServeCmd {
    ServeCmd {
        dir: base.to_path_buf(),
        bind: "127.0.0.1:0".parse().expect("bind"),
        token: Some("tok".to_string()),
        max_bytes: 1024 * 1024,
        push: PushMode::Off,
        remote: "origin".to_string(),
        allow_cidr: vec!["127.0.0.0/8".parse().expect("cidr")],
        git_author_name: "LAN Paste".to_string(),
        git_author_email: "paste@lan".to_string(),
    }
}

#[tokio::test]
async fn create_and_read_endpoints_work() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = test_cfg(dir.path());
    preflight::run_preflight(&cfg).expect("preflight");
    let state = Arc::new(preflight::build_state(cfg).expect("state"));

    let server = TestServer::new(http::app(state)).expect("server");
    let create = server
        .post("/api/v1/paste?name=note.md&tag=test")
        .add_header("X-Paste-Token", "tok")
        .add_header("X-Forwarded-For", "127.0.0.1")
        .add_header("Content-Type", "text/markdown")
        .text("# hello\n\nworld")
        .await;

    create.assert_status(StatusCode::CREATED);
    let json: serde_json::Value = create.json();
    let id = json["id"].as_str().expect("id");

    server
        .get(&format!("/api/v1/p/{id}"))
        .await
        .assert_status(StatusCode::OK);

    let raw = server.get(&format!("/api/v1/p/{id}/raw")).await;
    raw.assert_status(StatusCode::OK);
    assert!(raw.text().contains("hello"));

    let recent = server.get("/api/v1/recent?n=10&tag=test").await;
    recent.assert_status(StatusCode::OK);
    let arr: serde_json::Value = recent.json();
    assert!(arr.as_array().expect("array").len() == 1);

    let view = server.get(&format!("/p/{id}")).await;
    view.assert_status(StatusCode::OK);
    assert!(view.text().contains("<h1>hello</h1>") || view.text().contains("hello"));

    server.get("/healthz").await.assert_status(StatusCode::OK);
    server.get("/readyz").await.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn auth_and_size_and_cidr_enforced() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cfg = test_cfg(dir.path());
    cfg.max_bytes = 8;
    preflight::run_preflight(&cfg).expect("preflight");
    let state = Arc::new(preflight::build_state(cfg).expect("state"));
    let server = TestServer::new(http::app(state)).expect("server");

    server
        .post("/api/v1/paste")
        .add_header("X-Forwarded-For", "127.0.0.1")
        .text("ok")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    server
        .post("/api/v1/paste")
        .add_header("X-Paste-Token", "tok")
        .add_header("X-Forwarded-For", "10.0.0.2")
        .text("ok")
        .await
        .assert_status(StatusCode::FORBIDDEN);

    server
        .post("/api/v1/paste")
        .add_header("X-Paste-Token", "tok")
        .add_header("X-Forwarded-For", "127.0.0.1")
        .text("too-long!!")
        .await
        .assert_status(StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn best_effort_push_does_not_fail_request() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cfg = test_cfg(dir.path());
    cfg.push = PushMode::BestEffort;
    cfg.remote = "no-such-remote".to_string();
    preflight::run_preflight(&cfg).expect("preflight");
    let state = Arc::new(preflight::build_state(cfg).expect("state"));
    let server = TestServer::new(http::app(state)).expect("server");

    server
        .post("/api/v1/paste")
        .add_header("X-Paste-Token", "tok")
        .add_header("X-Forwarded-For", "127.0.0.1")
        .text("best effort")
        .await
        .assert_status(StatusCode::CREATED);
}

#[tokio::test]
async fn strict_push_failure_returns_500() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cfg = test_cfg(dir.path());
    cfg.push = PushMode::Strict;
    cfg.remote = "no-such-remote".to_string();
    preflight::run_preflight(&cfg).expect("preflight");
    let state = Arc::new(preflight::build_state(cfg).expect("state"));
    let server = TestServer::new(http::app(state)).expect("server");

    server
        .post("/api/v1/paste")
        .add_header("X-Paste-Token", "tok")
        .add_header("X-Forwarded-For", "127.0.0.1")
        .text("strict push")
        .await
        .assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

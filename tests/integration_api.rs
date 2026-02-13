use std::{fs, net::SocketAddr, sync::Arc};

use axum::{extract::connect_info::MockConnectInfo, http::StatusCode};
use axum_test::TestServer;
use lanpaste::{
    config::{PushMode, ServeCmd},
    gitops::FileLock,
    http, preflight,
};

fn test_cfg(base: &std::path::Path) -> ServeCmd {
    ServeCmd {
        dir: base.to_path_buf(),
        bind: "127.0.0.1:0".parse().expect("bind"),
        token: Some("tok".to_string()),
        api_keys_file: None,
        max_bytes: 1024 * 1024,
        push: PushMode::Off,
        remote: "origin".to_string(),
        allow_cidr: vec!["127.0.0.0/8".parse().expect("cidr")],
        git_author_name: "LAN Paste".to_string(),
        git_author_email: "paste@lan".to_string(),
    }
}

fn write_api_keys_file(path: &std::path::Path) {
    let keys = serde_json::json!({
        "keys": [
            {
                "name": "reader",
                "key": "reader-key",
                "scopes": ["api:index", "paste:read", "recent:read"],
                "max_requests_per_minute": 20
            },
            {
                "name": "writer",
                "key": "writer-key",
                "scopes": ["paste:create"],
                "max_requests_per_minute": 20
            },
            {
                "name": "limited",
                "key": "limited-key",
                "scopes": ["paste:create"],
                "max_requests_per_minute": 1
            }
        ]
    });
    fs::write(
        path,
        serde_json::to_vec_pretty(&keys).expect("serialize keys"),
    )
    .expect("write keys");
}

#[tokio::test]
async fn create_and_read_endpoints_work() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = test_cfg(dir.path());
    preflight::run_preflight(&cfg).expect("preflight");
    let state = Arc::new(preflight::build_state(cfg).expect("state"));

    let server = TestServer::new(
        http::app(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4100)))),
    )
    .expect("server");
    let create = server
        .post("/api/v1/paste?name=note.md&tag=test")
        .add_header("X-Paste-Token", "tok")
        .add_header("Content-Type", "text/markdown")
        .text("# hello\n\nworld")
        .await;

    create.assert_status(StatusCode::CREATED);
    let json: serde_json::Value = create.json();
    let id = json["id"].as_str().expect("id");
    let create_commit = json["commit"].as_str().expect("commit").to_string();
    assert!(!create_commit.is_empty());

    let api = server.get("/api").await;
    api.assert_status(StatusCode::OK);
    let api_json: serde_json::Value = api.json();
    assert!(
        api_json["endpoints"]
            .as_array()
            .expect("endpoints")
            .iter()
            .any(|v| v.as_str() == Some("/api/v1/paste (POST)"))
    );

    let dashboard = server.get("/").await;
    dashboard.assert_status(StatusCode::OK);
    assert!(dashboard.text().contains("LAN Paste Dashboard"));
    assert!(dashboard.text().contains("/api/v1/paste"));

    let dashboard_alias = server.get("/dashboard").await;
    dashboard_alias.assert_status(StatusCode::OK);

    let meta = server.get(&format!("/api/v1/p/{id}")).await;
    meta.assert_status(StatusCode::OK);
    let meta_json: serde_json::Value = meta.json();
    assert_eq!(
        meta_json["commit"].as_str().expect("meta commit"),
        create_commit
    );

    let raw = server.get(&format!("/api/v1/p/{id}/raw")).await;
    raw.assert_status(StatusCode::OK);
    assert_eq!(
        raw.header("content-type").to_str().expect("content-type"),
        "application/octet-stream"
    );
    assert_eq!(
        raw.header("content-disposition")
            .to_str()
            .expect("content-disposition"),
        "attachment"
    );
    assert_eq!(
        raw.header("x-content-type-options")
            .to_str()
            .expect("x-content-type-options"),
        "nosniff"
    );
    assert!(raw.text().contains("hello"));

    let recent = server.get("/api/v1/recent?n=10&tag=test").await;
    recent.assert_status(StatusCode::OK);
    let arr: serde_json::Value = recent.json();
    assert!(arr.as_array().expect("array").len() == 1);
    assert_eq!(
        arr[0]["commit"].as_str().expect("recent commit"),
        create_commit
    );

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
    let server = TestServer::new(
        http::app(state.clone()).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4101)))),
    )
    .expect("server");
    let blocked_server = TestServer::new(
        http::app(state).layer(MockConnectInfo(SocketAddr::from(([10, 0, 0, 2], 4102)))),
    )
    .expect("server");

    server
        .post("/api/v1/paste")
        .text("ok")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);

    blocked_server
        .post("/api/v1/paste")
        .add_header("X-Paste-Token", "tok")
        .add_header("X-Forwarded-For", "127.0.0.1")
        .text("ok")
        .await
        .assert_status(StatusCode::FORBIDDEN);

    server
        .post("/api/v1/paste")
        .add_header("X-Paste-Token", "tok")
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
    let server = TestServer::new(
        http::app(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4103)))),
    )
    .expect("server");

    server
        .post("/api/v1/paste")
        .add_header("X-Paste-Token", "tok")
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
    let server = TestServer::new(
        http::app(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4104)))),
    )
    .expect("server");

    server
        .post("/api/v1/paste")
        .add_header("X-Paste-Token", "tok")
        .text("strict push")
        .await
        .assert_status(StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn readyz_stays_ok_during_git_lock_contention() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = test_cfg(dir.path());
    preflight::run_preflight(&cfg).expect("preflight");
    let state = Arc::new(preflight::build_state(cfg).expect("state"));
    let _git_lock = FileLock::acquire(&state.paths.git_lock).expect("git lock");
    let server = TestServer::new(
        http::app(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4105)))),
    )
    .expect("server");

    server.get("/readyz").await.assert_status(StatusCode::OK);
}

#[tokio::test]
async fn idempotency_key_replays_and_conflicts_on_payload_mismatch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = test_cfg(dir.path());
    preflight::run_preflight(&cfg).expect("preflight");
    let state = Arc::new(preflight::build_state(cfg).expect("state"));
    let server = TestServer::new(
        http::app(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4106)))),
    )
    .expect("server");

    let first = server
        .post("/api/v1/paste?name=idempotent.txt")
        .add_header("X-Paste-Token", "tok")
        .add_header("Idempotency-Key", "retry-123")
        .text("same payload")
        .await;
    first.assert_status(StatusCode::CREATED);
    let first_json: serde_json::Value = first.json();

    let second = server
        .post("/api/v1/paste?name=idempotent.txt")
        .add_header("X-Paste-Token", "tok")
        .add_header("Idempotency-Key", "retry-123")
        .text("same payload")
        .await;
    second.assert_status(StatusCode::OK);
    let second_json: serde_json::Value = second.json();
    assert_eq!(first_json["id"], second_json["id"]);
    assert_eq!(first_json["commit"], second_json["commit"]);

    server
        .post("/api/v1/paste?name=idempotent.txt")
        .add_header("X-Paste-Token", "tok")
        .add_header("Idempotency-Key", "retry-123")
        .text("different payload")
        .await
        .assert_status(StatusCode::CONFLICT);
}

#[tokio::test]
async fn api_keys_enforce_scopes_and_rate_limits() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cfg = test_cfg(dir.path());
    cfg.token = None;
    let keys_path = dir.path().join("keys.json");
    write_api_keys_file(&keys_path);
    cfg.api_keys_file = Some(keys_path);

    preflight::run_preflight(&cfg).expect("preflight");
    let state = Arc::new(preflight::build_state(cfg).expect("state"));
    let server = TestServer::new(
        http::app(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4107)))),
    )
    .expect("server");

    server
        .get("/api")
        .await
        .assert_status(StatusCode::UNAUTHORIZED);
    server
        .get("/api")
        .add_header("X-API-Key", "reader-key")
        .await
        .assert_status(StatusCode::OK);

    server
        .post("/api/v1/paste?name=scope.txt")
        .add_header("X-API-Key", "reader-key")
        .text("reader cannot write")
        .await
        .assert_status(StatusCode::FORBIDDEN);

    let created = server
        .post("/api/v1/paste?name=scope.txt")
        .add_header("X-API-Key", "writer-key")
        .text("writer can write")
        .await;
    created.assert_status(StatusCode::CREATED);
    let id = created.json::<serde_json::Value>()["id"]
        .as_str()
        .expect("id")
        .to_string();

    server
        .get(&format!("/api/v1/p/{id}"))
        .add_header("X-API-Key", "writer-key")
        .await
        .assert_status(StatusCode::FORBIDDEN);
    server
        .get(&format!("/api/v1/p/{id}"))
        .add_header("X-API-Key", "reader-key")
        .await
        .assert_status(StatusCode::OK);

    server
        .post("/api/v1/paste?name=limited-1.txt")
        .add_header("X-API-Key", "limited-key")
        .text("first")
        .await
        .assert_status(StatusCode::CREATED);
    server
        .post("/api/v1/paste?name=limited-2.txt")
        .add_header("X-API-Key", "limited-key")
        .text("second")
        .await
        .assert_status(StatusCode::TOO_MANY_REQUESTS);
}

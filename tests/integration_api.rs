use std::{fs, net::SocketAddr, sync::Arc};

use axum::{extract::connect_info::MockConnectInfo, http::StatusCode};
use axum_test::{
    TestServer,
    multipart::{MultipartForm, Part},
};
use lanpaste::{
    config::{PushMode, ServeCmd},
    gitops::FileLock,
    http, preflight,
};

const ONE_PX_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
    0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xB5,
    0x1C, 0x0C, 0x02, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xFC,
    0x5F, 0x0F, 0x00, 0x02, 0x7F, 0x01, 0xF5, 0x90, 0xA1, 0x8D, 0xA5, 0x00, 0x00, 0x00, 0x00,
    0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

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
    let view_url = json["view_url"].as_str().expect("view_url").to_string();
    assert_eq!(view_url, format!("/p/{id}"));
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
    assert!(dashboard.text().contains(&format!("/p/{id}/md")));

    let dashboard_alias = server.get("/dashboard").await;
    dashboard_alias.assert_status(StatusCode::OK);

    let recent_page = server.get("/recent?tag=test").await;
    recent_page.assert_status(StatusCode::OK);
    assert!(recent_page.text().contains("Recent Pastes"));

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

    let view = server.get(&view_url).await;
    view.assert_status(StatusCode::OK);
    assert!(view.text().contains("<h1>hello</h1>") || view.text().contains("hello"));
    server
        .get(&format!("/p/{id}"))
        .await
        .assert_status(StatusCode::OK);
    let md_view = server.get(&format!("/p/{id}/md")).await;
    md_view.assert_status(StatusCode::OK);
    assert!(md_view.text().contains("<h1>hello</h1>") || md_view.text().contains("hello"));
    server
        .get(&format!("/p/{id}/note"))
        .await
        .assert_status(StatusCode::OK);

    server
        .get(&format!("/p/%2e%2e%2fmeta%2f{id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);

    server
        .get(&format!("/api/v1/p/%2e%2e%2fmeta%2f{id}"))
        .await
        .assert_status(StatusCode::NOT_FOUND);

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
async fn upload_image_and_embed_render_works() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = test_cfg(dir.path());
    preflight::run_preflight(&cfg).expect("preflight");
    let state = Arc::new(preflight::build_state(cfg).expect("state"));
    let server = TestServer::new(
        http::app(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4108)))),
    )
    .expect("server");

    let form = MultipartForm::new()
        .add_text("name", "chart.png")
        .add_text("tag", "charts")
        .add_part(
            "file",
            Part::bytes(ONE_PX_PNG.to_vec())
                .file_name("chart.png")
                .mime_type("image/png"),
        );

    let upload = server
        .post("/api/v1/upload")
        .add_header("X-Paste-Token", "tok")
        .multipart(form)
        .await;
    upload.assert_status(StatusCode::OK);
    let upload_json: serde_json::Value = upload.json();
    for key in [
        "id",
        "url",
        "contentType",
        "bytes",
        "width",
        "height",
        "createdAt",
    ] {
        assert!(upload_json.get(key).is_some(), "missing upload key {key}");
    }
    assert_eq!(upload_json["contentType"], "image/png");
    assert_eq!(upload_json["width"], 1);
    assert_eq!(upload_json["height"], 1);

    let file_url = upload_json["url"].as_str().expect("url").to_string();
    let file = server.get(&file_url).await;
    file.assert_status(StatusCode::OK);
    assert_eq!(
        file.header("content-type").to_str().expect("content-type"),
        "image/png"
    );
    assert_eq!(
        file.header("cache-control")
            .to_str()
            .expect("cache-control"),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(
        file.header("x-content-type-options")
            .to_str()
            .expect("x-content-type-options"),
        "nosniff"
    );
    assert_eq!(file.as_bytes(), ONE_PX_PNG);

    let upload_dup = server
        .post("/api/v1/upload")
        .add_header("X-Paste-Token", "tok")
        .multipart(
            MultipartForm::new().add_part(
                "file",
                Part::bytes(ONE_PX_PNG.to_vec())
                    .file_name("chart-dup.png")
                    .mime_type("image/png"),
            ),
        )
        .await;
    upload_dup.assert_status(StatusCode::OK);
    let upload_dup_json: serde_json::Value = upload_dup.json();
    assert_eq!(upload_json["id"], upload_dup_json["id"]);
    assert_eq!(upload_json["url"], upload_dup_json["url"]);

    let paste = format!("# chart\\n\\n![Chart]({file_url})");
    let created = server
        .post("/api/v1/paste?name=with-chart.md&tag=charts")
        .add_header("X-Paste-Token", "tok")
        .add_header("Content-Type", "text/markdown")
        .text(&paste)
        .await;
    created.assert_status(StatusCode::CREATED);
    let id = created.json::<serde_json::Value>()["id"]
        .as_str()
        .expect("id")
        .to_string();
    let view = server.get(&format!("/p/{id}/md")).await;
    view.assert_status(StatusCode::OK);
    let body = view.text();
    assert!(body.contains(&format!("src=\"{file_url}\"")));
    assert!(body.contains("Copy raw markdown"));
}

#[tokio::test]
async fn slug_alias_redirects_to_canonical_id() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = test_cfg(dir.path());
    preflight::run_preflight(&cfg).expect("preflight");
    let state = Arc::new(preflight::build_state(cfg).expect("state"));
    let server = TestServer::new(
        http::app(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4109)))),
    )
    .expect("server");

    let first = server
        .post("/api/v1/paste?name=brief-2026-03-03.md")
        .add_header("X-Paste-Token", "tok")
        .add_header("Content-Type", "text/markdown")
        .text("# first")
        .await;
    first.assert_status(StatusCode::CREATED);
    let first_id = first.json::<serde_json::Value>()["id"]
        .as_str()
        .expect("id")
        .to_string();

    let second = server
        .post("/api/v1/paste?name=brief-2026-03-03.md")
        .add_header("X-Paste-Token", "tok")
        .add_header("Content-Type", "text/markdown")
        .text("# second")
        .await;
    second.assert_status(StatusCode::CREATED);
    let second_id = second.json::<serde_json::Value>()["id"]
        .as_str()
        .expect("id")
        .to_string();

    let alias = server.get("/p/brief-2026-03-03").await;
    alias.assert_status(StatusCode::FOUND);
    assert_eq!(
        alias.header("location").to_str().expect("location"),
        format!("/p/{first_id}")
    );

    let alias_two = server.get("/p/brief-2026-03-03-2").await;
    alias_two.assert_status(StatusCode::FOUND);
    assert_eq!(
        alias_two.header("location").to_str().expect("location"),
        format!("/p/{second_id}")
    );

    server
        .get(&format!("/p/{first_id}"))
        .await
        .assert_status(StatusCode::OK);
    server
        .get(&format!("/p/{second_id}"))
        .await
        .assert_status(StatusCode::OK);
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

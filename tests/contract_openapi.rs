use std::{net::SocketAddr, sync::Arc};

use axum::{extract::connect_info::MockConnectInfo, http::StatusCode};
use axum_test::TestServer;
use lanpaste::{
    config::{PushMode, ServeCmd},
    http, preflight,
};
use serde_yaml::Value as YamlValue;

fn cfg(base: &std::path::Path) -> ServeCmd {
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

fn mapping_has_key(map: &serde_yaml::Mapping, key: &str) -> bool {
    map.contains_key(YamlValue::String(key.to_string()))
}

#[test]
fn openapi_spec_declares_core_routes_and_security() {
    let spec: YamlValue =
        serde_yaml::from_str(include_str!("../openapi.yaml")).expect("parse openapi");
    let paths = spec["paths"].as_mapping().expect("paths mapping");
    for path in [
        "/",
        "/dashboard",
        "/api",
        "/api/v1/paste",
        "/api/v1/p/{id}",
        "/api/v1/p/{id}/raw",
        "/api/v1/recent",
        "/p/{id}",
        "/healthz",
        "/readyz",
    ] {
        assert!(mapping_has_key(paths, path), "missing path {path}");
    }

    let schemes = spec["components"]["securitySchemes"]
        .as_mapping()
        .expect("securitySchemes mapping");
    assert!(mapping_has_key(schemes, "ApiKeyAuth"));
    assert!(mapping_has_key(schemes, "PasteTokenAuth"));
}

#[tokio::test]
async fn runtime_contract_matches_openapi_critical_shapes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = cfg(dir.path());
    preflight::run_preflight(&cfg).expect("preflight");
    let state = Arc::new(preflight::build_state(cfg).expect("state"));
    let server = TestServer::new(
        http::app(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4110)))),
    )
    .expect("server");

    let api = server.get("/api").await;
    api.assert_status(StatusCode::OK);
    let api_json: serde_json::Value = api.json();
    assert!(api_json.get("name").is_some());
    assert!(api_json.get("version").is_some());
    assert!(
        api_json
            .get("endpoints")
            .and_then(serde_json::Value::as_array)
            .is_some()
    );

    let created = server
        .post("/api/v1/paste?name=contract.md&tag=contract")
        .add_header("X-Paste-Token", "tok")
        .add_header("Content-Type", "text/markdown")
        .text("# contract")
        .await;
    created.assert_status(StatusCode::CREATED);
    let created_json: serde_json::Value = created.json();
    for key in ["id", "path", "commit", "raw_url", "view_url", "meta_url"] {
        assert!(created_json.get(key).is_some(), "missing create key {key}");
    }
    let id = created_json["id"].as_str().expect("id");

    let meta = server.get(&format!("/api/v1/p/{id}")).await;
    meta.assert_status(StatusCode::OK);
    let meta_json: serde_json::Value = meta.json();
    for key in [
        "id",
        "created_at",
        "path",
        "size",
        "content_type",
        "commit",
        "sha256",
    ] {
        assert!(meta_json.get(key).is_some(), "missing meta key {key}");
    }

    let recent = server.get("/api/v1/recent?n=1").await;
    recent.assert_status(StatusCode::OK);
    let recent_json: serde_json::Value = recent.json();
    let first = recent_json
        .as_array()
        .and_then(|v| v.first())
        .expect("recent item");
    for key in ["id", "created_at", "path", "commit", "size", "content_type"] {
        assert!(first.get(key).is_some(), "missing recent key {key}");
    }
}

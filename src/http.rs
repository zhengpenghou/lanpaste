use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use axum::{
    Router,
    body::Body,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tracing::warn;

use crate::{
    auth::{self, Scope},
    errors::{AppError, AppResult},
    gitops::{self, FileLock},
    render, store,
    types::{AppState, CreatePasteInput, CreatePasteResponse, IdempotencyRecord, RecentItem},
};

#[derive(Debug, Deserialize)]
struct CreateParams {
    name: Option<String>,
    msg: Option<String>,
    tag: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecentParams {
    n: Option<usize>,
    tag: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApiIndex {
    name: &'static str,
    version: &'static str,
    endpoints: Vec<&'static str>,
}

const IDEMPOTENCY_KEY_HEADER: &str = "Idempotency-Key";

pub fn app(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(dashboard))
        .route("/dashboard", get(dashboard))
        .route("/api", get(api_index))
        .route("/api/v1/paste", post(create_paste))
        .route("/api/v1/p/{id}", get(get_meta))
        .route("/api/v1/p/{id}/raw", get(get_raw))
        .route("/api/v1/recent", get(recent))
        .route("/p/{id}", get(render_view))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .layer(axum::extract::DefaultBodyLimit::max(state.cfg.max_bytes))
        .with_state(state)
}

pub async fn run_server(state: Arc<AppState>) -> AppResult<()> {
    let listener = TcpListener::bind(state.cfg.bind)
        .await
        .map_err(|e| AppError::internal(format!("bind failed: {e}")))?;
    axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(|e| AppError::internal(format!("server failed: {e}")))
}

async fn dashboard(State(state): State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
    let list = store::read_recent(&state.paths.repo, &state.cfg, 20, None)?;
    let out: Vec<RecentItem> = list
        .into_iter()
        .map(|m| RecentItem {
            id: m.id,
            created_at: m.created_at,
            path: m.path,
            commit: m.commit,
            tag: m.tag,
            size: m.size,
            content_type: m.content_type,
        })
        .collect();
    Ok(Html(render::render_dashboard(&out)))
}

async fn api_index(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> AppResult<impl IntoResponse> {
    auth::authorize(&state.api_keys, &headers, Scope::ApiIndex)?;
    Ok(axum::Json(ApiIndex {
        name: "lanpaste",
        version: "v1",
        endpoints: vec![
            "/api/v1/paste (POST)",
            "/api/v1/p/{id} (GET)",
            "/api/v1/p/{id}/raw (GET)",
            "/api/v1/recent?n=50&tag=... (GET)",
        ],
    }))
}

async fn create_paste(
    State(state): State<Arc<AppState>>,
    ConnectInfo(remote_addr): ConnectInfo<SocketAddr>,
    Query(params): Query<CreateParams>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> AppResult<impl IntoResponse> {
    if state.api_keys.enabled() {
        auth::authorize(&state.api_keys, &headers, Scope::PasteCreate)?;
    } else {
        let provided_token = headers.get("X-Paste-Token").and_then(|v| v.to_str().ok());
        store::verify_token(state.cfg.token.as_deref(), provided_token)?;
    }

    let ip = Some(client_ip(ConnectInfo(remote_addr)));
    store::check_cidr(&state.cfg.allow_cidr, ip)?;

    if body.len() > state.cfg.max_bytes {
        return Err(AppError::TooLarge(
            "request body exceeds max-bytes".to_string(),
        ));
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(ToString::to_string);

    let input = CreatePasteInput {
        name: params.name,
        msg: params.msg,
        tag: params.tag,
        content_type,
        bytes: body.to_vec(),
        client_ip: ip,
        user_agent,
    };

    let idempotency_key = headers
        .get(IDEMPOTENCY_KEY_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string);

    let _git_lock = FileLock::acquire(&state.paths.git_lock)?;
    let request_fingerprint = idempotency_key
        .as_deref()
        .map(|_| store::idempotency_fingerprint(&input));
    if let (Some(key), Some(fingerprint)) =
        (idempotency_key.as_deref(), request_fingerprint.as_deref())
        && let Some(record) = store::read_idempotency_record(&state.paths.idempotency, key)?
    {
        if record.request_fingerprint != fingerprint {
            return Err(AppError::Conflict(
                "idempotency key reuse with different payload".to_string(),
            ));
        }
        return Ok((StatusCode::OK, axum::Json(record.response)));
    }

    let draft = store::build_paste_draft(&state.paths.repo, &state.cfg, input)?;
    let commit = gitops::commit_paste(
        &state.paths.repo,
        &state.cfg,
        &draft,
        state.cfg.push,
        &state.cfg.remote,
    )?;

    if let Some(err) = commit.push_error {
        warn!("best-effort push failed: {err}");
    }

    let resp = CreatePasteResponse {
        id: draft.id.clone(),
        path: draft.rel_path.clone(),
        commit: commit.commit,
        raw_url: format!("/api/v1/p/{}/raw", draft.id),
        view_url: format!("/p/{}", draft.id),
        meta_url: format!("/api/v1/p/{}", draft.id),
    };

    if let (Some(key), Some(fingerprint)) = (idempotency_key.as_deref(), request_fingerprint) {
        store::write_idempotency_record(
            &state.paths.idempotency,
            key,
            &IdempotencyRecord {
                request_fingerprint: fingerprint,
                response: resp.clone(),
            },
        )?;
    }

    Ok((StatusCode::CREATED, axum::Json(resp)))
}

async fn get_meta(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    auth::authorize(&state.api_keys, &headers, Scope::PasteRead)?;
    let meta = store::read_meta(&state.paths.repo, &state.cfg, &id)?;
    Ok(axum::Json(meta))
}

async fn get_raw(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<Response> {
    auth::authorize(&state.api_keys, &headers, Scope::PasteRead)?;
    let meta = store::read_meta(&state.paths.repo, &state.cfg, &id)?;
    let bytes = store::read_paste(&state.paths.repo, &meta)?;
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_static("attachment"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    Ok(response)
}

async fn recent(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<RecentParams>,
) -> AppResult<impl IntoResponse> {
    auth::authorize(&state.api_keys, &headers, Scope::RecentRead)?;
    let n = q.n.unwrap_or(50).min(500);
    let list = store::read_recent(&state.paths.repo, &state.cfg, n, q.tag.as_deref())?;
    let out: Vec<RecentItem> = list
        .into_iter()
        .map(|m| RecentItem {
            id: m.id,
            created_at: m.created_at,
            path: m.path,
            commit: m.commit,
            tag: m.tag,
            size: m.size,
            content_type: m.content_type,
        })
        .collect();
    Ok(axum::Json(out))
}

async fn render_view(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    let meta = store::read_meta(&state.paths.repo, &state.cfg, &id)?;
    let bytes = store::read_paste(&state.paths.repo, &meta)?;
    let body = String::from_utf8_lossy(&bytes);
    let html = if meta.content_type.contains("markdown") || meta.path.ends_with(".md") {
        render::render_markdown(&body)
    } else {
        format!("<pre>{}</pre>", render::html_escape(&body))
    };
    Ok(Html(render::render_page(&meta.id, &html)))
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn readyz(State(state): State<Arc<AppState>>) -> AppResult<impl IntoResponse> {
    if let Err(err) = gitops::ready(&state.paths.repo, &state.paths.git_lock, &state.cfg) {
        return Err(AppError::ServiceUnavailable(format!("{err:?}")));
    }
    Ok((StatusCode::OK, "ok"))
}

fn client_ip(ConnectInfo(addr): ConnectInfo<SocketAddr>) -> IpAddr {
    addr.ip()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_ip_from_connect_info() {
        let addr: SocketAddr = "192.168.1.2:4321".parse().expect("addr");
        assert_eq!(
            client_ip(ConnectInfo(addr)).to_string(),
            "192.168.1.2".to_string()
        );
    }
}

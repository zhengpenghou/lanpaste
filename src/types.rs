use serde::{Deserialize, Serialize};
use std::{fs::File, net::IpAddr, path::PathBuf, sync::Arc};
use time::OffsetDateTime;

use crate::{
    auth::ApiKeyStore,
    config::{PushMode, ServeCmd},
};

#[derive(Clone)]
pub struct AppState {
    pub cfg: ServeCmd,
    pub paths: AppPaths,
    pub _daemon_lock: Arc<File>,
    pub api_keys: ApiKeyStore,
}

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub base: PathBuf,
    pub repo: PathBuf,
    pub run: PathBuf,
    pub tmp: PathBuf,
    pub git_lock: PathBuf,
    pub idempotency: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasteMeta {
    pub id: String,
    pub created_at: OffsetDateTime,
    pub path: String,
    pub size: usize,
    pub content_type: String,
    pub commit: String,
    pub sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_ip: Option<IpAddr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePasteResponse {
    pub id: String,
    pub path: String,
    pub commit: String,
    pub raw_url: String,
    pub view_url: String,
    pub meta_url: String,
}

#[derive(Debug)]
pub struct CreatePasteInput {
    pub name: Option<String>,
    pub msg: Option<String>,
    pub tag: Option<String>,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
    pub client_ip: Option<IpAddr>,
    pub user_agent: Option<String>,
}

#[derive(Debug)]
pub struct PasteDraft {
    pub id: String,
    pub rel_path: String,
    pub abs_path: PathBuf,
    pub meta_path: PathBuf,
    pub meta_rel_path: String,
    pub content_type: String,
    pub size: usize,
    pub sha256: String,
    pub subject: String,
    pub meta: PasteMeta,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub error: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    pub request_fingerprint: String,
    pub response: CreatePasteResponse,
}

#[derive(Debug, Serialize)]
pub struct RecentItem {
    pub id: String,
    pub created_at: OffsetDateTime,
    pub path: String,
    pub commit: String,
    pub tag: Option<String>,
    pub size: usize,
    pub content_type: String,
}

#[derive(Debug)]
pub struct GitCommitResult {
    pub commit: String,
    pub pushed: bool,
    pub push_error: Option<String>,
}

impl AppPaths {
    pub fn from_base(base: PathBuf) -> Self {
        let repo = base.join("repo");
        let run = base.join("run");
        let tmp = base.join("tmp");
        let git_lock = run.join("git.lock");
        let idempotency = run.join("idempotency");
        Self {
            base,
            repo,
            run,
            tmp,
            git_lock,
            idempotency,
        }
    }
}

pub fn push_mode_label(v: PushMode) -> &'static str {
    match v {
        PushMode::Off => "off",
        PushMode::BestEffort => "best_effort",
        PushMode::Strict => "strict",
    }
}

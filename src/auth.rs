use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    sync::{Arc, Mutex},
};

use axum::http::HeaderMap;
use serde::Deserialize;
use subtle::ConstantTimeEq;
use time::OffsetDateTime;

use crate::errors::{AppError, AppResult};

pub const API_KEY_HEADER: &str = "X-API-Key";

#[derive(Debug, Clone, Copy)]
pub enum Scope {
    ApiIndex,
    PasteCreate,
    PasteRead,
    RecentRead,
}

impl Scope {
    fn as_str(self) -> &'static str {
        match self {
            Scope::ApiIndex => "api:index",
            Scope::PasteCreate => "paste:create",
            Scope::PasteRead => "paste:read",
            Scope::RecentRead => "recent:read",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiKeysFile {
    pub keys: Vec<ApiKeyEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiKeyEntry {
    #[serde(default)]
    pub name: Option<String>,
    pub key: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub max_requests_per_minute: Option<u32>,
}

#[derive(Debug, Clone)]
struct RateWindow {
    minute_window: i64,
    count: u32,
}

#[derive(Clone, Default)]
pub struct ApiKeyStore {
    entries: Arc<Vec<ApiKeyEntry>>,
    counters: Arc<Mutex<HashMap<String, RateWindow>>>,
}

impl ApiKeyStore {
    pub fn from_file(path: Option<&Path>) -> AppResult<Self> {
        let Some(path) = path else {
            return Ok(Self::default());
        };

        let raw = fs::read(path).map_err(|e| AppError::io("read api key file", e))?;
        let file: ApiKeysFile = serde_json::from_slice(&raw)
            .map_err(|e| AppError::internal(format!("parse api key file: {e}")))?;

        let mut seen = HashSet::new();
        for entry in &file.keys {
            if entry.key.trim().is_empty() {
                return Err(AppError::internal("api key entry has empty key"));
            }
            if entry.scopes.is_empty() {
                return Err(AppError::internal(format!(
                    "api key '{}' must include at least one scope",
                    entry.name.as_deref().unwrap_or("unnamed")
                )));
            }
            if entry.max_requests_per_minute == Some(0) {
                return Err(AppError::internal(format!(
                    "api key '{}' has invalid max_requests_per_minute=0",
                    entry.name.as_deref().unwrap_or("unnamed")
                )));
            }
            if !seen.insert(entry.key.clone()) {
                return Err(AppError::internal("duplicate api key in api key file"));
            }
        }

        Ok(Self {
            entries: Arc::new(file.keys),
            counters: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn enabled(&self) -> bool {
        !self.entries.is_empty()
    }

    fn resolve_key(&self, provided: &str) -> Option<ApiKeyEntry> {
        self.entries
            .iter()
            .find(|entry| entry.key.as_bytes().ct_eq(provided.as_bytes()).into())
            .cloned()
    }

    fn enforce_rate_limit(&self, entry: &ApiKeyEntry) -> AppResult<()> {
        let Some(limit) = entry.max_requests_per_minute else {
            return Ok(());
        };

        let now_minute = OffsetDateTime::now_utc().unix_timestamp() / 60;
        let key_id = entry
            .name
            .clone()
            .unwrap_or_else(|| format!("key:{}", entry.key.chars().take(8).collect::<String>()));
        let mut counters = self
            .counters
            .lock()
            .map_err(|_| AppError::internal("api key rate limiter lock poisoned"))?;
        let counter = counters.entry(key_id).or_insert(RateWindow {
            minute_window: now_minute,
            count: 0,
        });

        if counter.minute_window != now_minute {
            counter.minute_window = now_minute;
            counter.count = 0;
        }

        if counter.count >= limit {
            return Err(AppError::TooManyRequests(
                "api key rate limit exceeded".to_string(),
            ));
        }
        counter.count += 1;
        Ok(())
    }
}

pub fn authorize(store: &ApiKeyStore, headers: &HeaderMap, scope: Scope) -> AppResult<()> {
    if !store.enabled() {
        return Ok(());
    }

    let provided = headers
        .get(API_KEY_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if provided.is_empty() {
        return Err(AppError::Unauthorized(
            "missing or invalid API key".to_string(),
        ));
    }

    let key = store
        .resolve_key(provided)
        .ok_or_else(|| AppError::Unauthorized("missing or invalid API key".to_string()))?;
    let needed = scope.as_str();
    let allowed = key.scopes.iter().any(|s| s == "*" || s == needed);
    if !allowed {
        return Err(AppError::Forbidden(format!(
            "api key lacks required scope '{needed}'"
        )));
    }

    store.enforce_rate_limit(&key)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_store_allows_requests() {
        let headers = HeaderMap::new();
        assert!(authorize(&ApiKeyStore::default(), &headers, Scope::ApiIndex).is_ok());
    }
}

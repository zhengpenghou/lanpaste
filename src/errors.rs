use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::types::ApiErrorBody;

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    TooLarge(String),
    Internal(String),
    ServiceUnavailable(String),
}

impl AppError {
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    pub fn io(ctx: &str, err: std::io::Error) -> Self {
        Self::Internal(format!("{ctx}: {err}"))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m),
            AppError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, "unauthorized", m),
            AppError::Forbidden(m) => (StatusCode::FORBIDDEN, "forbidden", m),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m),
            AppError::Conflict(m) => (StatusCode::CONFLICT, "conflict", m),
            AppError::TooLarge(m) => (StatusCode::PAYLOAD_TOO_LARGE, "too_large", m),
            AppError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, "internal", m),
            AppError::ServiceUnavailable(m) => {
                (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", m)
            }
        };
        (status, Json(ApiErrorBody { error: code.to_string(), message })).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn maps_status_and_json() {
        let resp = AppError::Forbidden("no".to_string()).into_response();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let bytes = to_bytes(resp.into_body(), 4096).await.expect("body");
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(v["error"], "forbidden");
        assert_eq!(v["message"], "no");
    }
}

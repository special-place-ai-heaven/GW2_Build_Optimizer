use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Unauthorized,
    NotFound,
    PayloadTooLarge,
    RateLimited { retry_after_secs: u64 },
    UpgradeRequired,
    DbUnavailable,
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error, reason, retry) = match self {
            ApiError::BadRequest(r) => (StatusCode::BAD_REQUEST, "bad_request", r, None),
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                String::new(),
                None,
            ),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found", String::new(), None),
            ApiError::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "too_large",
                "body over 16 KB".into(),
                None,
            ),
            ApiError::RateLimited { retry_after_secs } => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                String::new(),
                Some(retry_after_secs),
            ),
            ApiError::UpgradeRequired => (
                StatusCode::UPGRADE_REQUIRED,
                "addon_too_old",
                "update the addon".into(),
                None,
            ),
            ApiError::DbUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "db_unavailable",
                "database unreachable".into(),
                None,
            ),
            ApiError::Internal(r) => {
                tracing::error!("internal: {r}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    String::new(),
                    None,
                )
            }
        };
        let mut res = (status, Json(json!({ "error": error, "reason": reason }))).into_response();
        if let Some(secs) = retry {
            res.headers_mut()
                .insert("retry-after", secs.to_string().parse().unwrap());
        }
        res
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError::Internal(e.to_string())
    }
}

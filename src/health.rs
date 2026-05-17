use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// A minimal health-check response body.
///
/// Serializes as `{ "status": "ok" }` and responds with `200 OK`.
///
/// Use [`HealthResponse::degraded`] or [`HealthResponse::unhealthy`] for non-nominal states.
///
/// # Example
///
/// ```rust
/// use axum::response::IntoResponse;
/// use axum_api_kit::HealthResponse;
///
/// async fn health() -> impl IntoResponse {
///     HealthResponse::ok()
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    #[serde(skip)]
    status_code: StatusCode,
}

impl HealthResponse {
    /// Returns a `HealthResponse` with `status = "ok"` and HTTP `200 OK`.
    pub const fn ok() -> Self {
        Self {
            status: "ok",
            status_code: StatusCode::OK,
        }
    }

    /// Returns a `HealthResponse` with `status = "degraded"` and HTTP `200 OK`.
    ///
    /// Use when the service is reachable but operating in a reduced capacity
    /// (e.g., a non-critical dependency is down).
    pub const fn degraded() -> Self {
        Self {
            status: "degraded",
            status_code: StatusCode::OK,
        }
    }

    /// Returns a `HealthResponse` with `status = "unhealthy"` and HTTP `503 Service Unavailable`.
    ///
    /// Use when the service cannot fulfill requests normally
    /// (e.g., the primary database is unreachable).
    pub const fn unhealthy() -> Self {
        Self {
            status: "unhealthy",
            status_code: StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

impl IntoResponse for HealthResponse {
    fn into_response(self) -> Response {
        (self.status_code, Json(self)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_has_correct_status() {
        let r = HealthResponse::ok();
        assert_eq!(r.status, "ok");
        assert_eq!(r.status_code, StatusCode::OK);
    }

    #[test]
    fn degraded_has_correct_status() {
        let r = HealthResponse::degraded();
        assert_eq!(r.status, "degraded");
        assert_eq!(r.status_code, StatusCode::OK);
    }

    #[test]
    fn unhealthy_has_correct_status() {
        let r = HealthResponse::unhealthy();
        assert_eq!(r.status, "unhealthy");
        assert_eq!(r.status_code, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn serializes_status_field_only() {
        let r = HealthResponse::ok();
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["status"], "ok");
        assert!(v.get("status_code").is_none());
    }
}

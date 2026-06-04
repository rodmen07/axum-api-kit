use std::future::Future;

use axum::{routing::get, Router};

use crate::HealthResponse;

/// Liveness probe handler: always returns [`HealthResponse::ok`].
pub async fn liveness() -> HealthResponse {
    HealthResponse::ok()
}

/// Build a [`Router`] exposing `/healthz` (liveness) and `/readyz` (readiness) probes.
///
/// `/healthz` always reports `ok` (the process is running). `/readyz` runs `readiness` on
/// each request and returns whatever [`HealthResponse`] it produces, so a failing dependency
/// can report `unhealthy` (HTTP 503). For a service with no dependencies to check, pass
/// `|| async { HealthResponse::ok() }`.
///
/// The router is generic over the state type `S`, so it can be merged into a stateful app
/// with `app.merge(health_routes(...))`. Capture whatever the readiness check needs (a
/// database pool, etc.) in the closure.
///
/// Requires the `router` feature.
///
/// # Example
///
/// ```rust,no_run
/// use axum::Router;
/// use axum_api_kit::{health_routes, HealthResponse};
///
/// let app: Router = Router::new().merge(health_routes(|| async {
///     // probe your dependencies here...
///     HealthResponse::ok()
/// }));
/// ```
pub fn health_routes<S, F, Fut>(readiness: F) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    F: Fn() -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = HealthResponse> + Send + 'static,
{
    // Wrap in a concrete zero-arg closure so axum can resolve the `Handler` marker; the
    // returned future is the generic `Fut`, whose `Send` bound is explicit (an opaque async
    // block here would not be provably `Send` in this generic context).
    #[allow(clippy::redundant_closure)]
    let readyz = move || readiness();

    Router::new()
        .route("/healthz", get(liveness))
        .route("/readyz", get(readyz))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, http::StatusCode};
    use tower::ServiceExt;

    async fn call(app: Router, path: &str) -> (StatusCode, serde_json::Value) {
        let res = app
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, body)
    }

    #[tokio::test]
    async fn healthz_is_always_ok() {
        let app: Router = health_routes(|| async { HealthResponse::unhealthy() });
        let (status, body) = call(app, "/healthz").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn readyz_reports_ready() {
        let app: Router = health_routes(|| async { HealthResponse::ok() });
        let (status, body) = call(app, "/readyz").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn readyz_reports_unhealthy() {
        let app: Router = health_routes(|| async { HealthResponse::unhealthy() });
        let (status, body) = call(app, "/readyz").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "unhealthy");
    }

    #[tokio::test]
    async fn merges_into_stateful_app() {
        #[derive(Clone)]
        struct AppState;
        let app: Router<AppState> =
            Router::new().merge(health_routes(|| async { liveness().await }));
        let app: Router = app.with_state(AppState);
        let (status, _) = call(app, "/healthz").await;
        assert_eq!(status, StatusCode::OK);
    }
}

use std::future::Future;

use axum::{http::StatusCode, routing::get, Json, Router};

use crate::{ApiError, HealthResponse};

/// Liveness probe handler: always returns [`HealthResponse::ok`].
pub async fn liveness() -> HealthResponse {
    HealthResponse::ok()
}

/// The `code` the kit's unmatched-path fallbacks report.
const NOT_FOUND_CODE: &str = "NOT_FOUND";
/// The `message` the kit's unmatched-path fallbacks report.
const NOT_FOUND_MESSAGE: &str = "no route matches this path";
/// The `code` the kit's wrong-method fallbacks report.
const METHOD_NOT_ALLOWED_CODE: &str = "METHOD_NOT_ALLOWED";
/// The `message` the kit's wrong-method fallbacks report.
const METHOD_NOT_ALLOWED_MESSAGE: &str = "this path does not support the request method";

/// The single source of the unmatched-path error, shared by both flavours.
///
/// [`api_fallback`] serves it as `ApiError` JSON and [`problem_fallback`] runs it through the
/// existing `(StatusCode, ApiError)` -> [`Problem`](crate::Problem) bridge, so the two can never
/// disagree about the status or the `code` — they are not two definitions kept in step, they are
/// one definition rendered twice.
fn not_found_error() -> (StatusCode, ApiError) {
    (
        StatusCode::NOT_FOUND,
        ApiError::new(NOT_FOUND_CODE, NOT_FOUND_MESSAGE),
    )
}

/// The single source of the wrong-method error, shared by both flavours.
///
/// There is deliberately no `ApiError::method_not_allowed` factory: 405 is a routing outcome
/// rather than something a handler returns, so the mapping lives with the router wiring that
/// produces it instead of growing `src/error.rs`.
fn method_not_allowed_error() -> (StatusCode, ApiError) {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        ApiError::new(METHOD_NOT_ALLOWED_CODE, METHOD_NOT_ALLOWED_MESSAGE),
    )
}

/// `404 Not Found` fallback handler emitting the kit's [`ApiError`] JSON body.
///
/// Axum's built-in fallback answers an unmatched path with a bare `404` and an **empty body**,
/// which is the one response a client's JSON error handler cannot parse. Wiring this handler with
/// [`Router::fallback`] makes "no such route" speak the same contract as every failure the kit
/// already covers:
///
/// ```json
/// {"code":"NOT_FOUND","message":"no route matches this path"}
/// ```
///
/// served as `application/json`. Nothing about an existing response changes: this only fills a
/// case axum previously answered with nothing.
///
/// Requires the `router` feature. For the RFC 9457 flavour, name [`problem_fallback`] instead —
/// the format is chosen by which handler you wire, never by which features happen to be enabled.
///
/// # Example
///
/// ```rust
/// use axum::{routing::get, Router};
/// use axum_api_kit::api_fallback;
///
/// async fn list_items() -> &'static str { "[]" }
///
/// let app: Router = Router::new()
///     .route("/items", get(list_items))
///     .fallback(api_fallback);
/// ```
pub async fn api_fallback() -> (StatusCode, Json<ApiError>) {
    let (status, error) = not_found_error();
    (status, Json(error))
}

/// `405 Method Not Allowed` fallback handler emitting the kit's [`ApiError`] JSON body.
///
/// Axum answers a request whose path matches but whose method does not with a bare `405` and an
/// empty body. Wiring this handler with [`Router::method_not_allowed_fallback`] gives that case
/// the kit's contract:
///
/// ```json
/// {"code":"METHOD_NOT_ALLOWED","message":"this path does not support the request method"}
/// ```
///
/// served as `application/json`. The `Allow` header axum computes from the methods the path does
/// route is still sent — axum sets it on the way out, so replacing the body does not drop it.
///
/// **Wire it last.** `Router::method_not_allowed_fallback` rewrites the method fallback of the
/// routes registered *before* the call, so a `route(..)` added afterwards keeps axum's empty
/// `405`. This is an axum property, not a kit one, and it is the quiet way this handler ends up
/// looking like it does nothing.
///
/// Requires the `router` feature. For the RFC 9457 flavour, name
/// [`problem_method_not_allowed`] instead.
///
/// # Example
///
/// ```rust
/// use axum::{routing::get, Router};
/// use axum_api_kit::{api_fallback, api_method_not_allowed};
///
/// async fn list_items() -> &'static str { "[]" }
///
/// let app: Router = Router::new()
///     .route("/items", get(list_items))
///     .fallback(api_fallback)
///     // after every `route(..)`, or the routes added later keep axum's empty 405
///     .method_not_allowed_fallback(api_method_not_allowed);
/// ```
pub async fn api_method_not_allowed() -> (StatusCode, Json<ApiError>) {
    let (status, error) = method_not_allowed_error();
    (status, Json(error))
}

/// RFC 9457 sibling of [`api_fallback`]: `404 Not Found` as `application/problem+json`.
///
/// Same status and same `code` as [`api_fallback`] — both render the one error definition — but
/// through [`Problem`](crate::Problem), so the body is:
///
/// ```json
/// {"title":"Not Found","status":404,"detail":"no route matches this path","code":"NOT_FOUND"}
/// ```
///
/// Requires the `router` **and** `problem` features. Enabling `problem` never changes what
/// [`api_fallback`] emits: the flavour is chosen by naming this constructor. `Problem`'s opt-in
/// `Accept` negotiation is likewise not applied here; a consumer who wants it wires a handler of
/// their own calling [`Problem::into_response_for`](crate::Problem::into_response_for).
///
/// # Example
///
/// ```rust
/// use axum::{routing::get, Router};
/// use axum_api_kit::problem_fallback;
///
/// async fn list_items() -> &'static str { "[]" }
///
/// let app: Router = Router::new()
///     .route("/items", get(list_items))
///     .fallback(problem_fallback);
/// ```
#[cfg(feature = "problem")]
pub async fn problem_fallback() -> crate::Problem {
    crate::Problem::from(not_found_error())
}

/// RFC 9457 sibling of [`api_method_not_allowed`]: `405` as `application/problem+json`.
///
/// Same status and same `code` as [`api_method_not_allowed`], rendered through
/// [`Problem`](crate::Problem):
///
/// ```json
/// {"title":"Method Not Allowed","status":405,"detail":"this path does not support the request method","code":"METHOD_NOT_ALLOWED"}
/// ```
///
/// The same wire-it-last rule as [`api_method_not_allowed`] applies, for the same axum reason.
///
/// Requires the `router` **and** `problem` features.
///
/// # Example
///
/// ```rust
/// use axum::{routing::get, Router};
/// use axum_api_kit::{problem_fallback, problem_method_not_allowed};
///
/// async fn list_items() -> &'static str { "[]" }
///
/// let app: Router = Router::new()
///     .route("/items", get(list_items))
///     .fallback(problem_fallback)
///     .method_not_allowed_fallback(problem_method_not_allowed);
/// ```
#[cfg(feature = "problem")]
pub async fn problem_method_not_allowed() -> crate::Problem {
    crate::Problem::from(method_not_allowed_error())
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

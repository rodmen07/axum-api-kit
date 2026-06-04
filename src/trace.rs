use std::time::Instant;

use axum::{
    extract::{FromRequestParts, Request},
    http::{header::HeaderName, request::Parts, HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};

use crate::ApiError;

/// The header used to carry the request correlation id: `x-request-id`.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// A request correlation id, stored in request extensions by [`propagate_request_id`].
///
/// Extract it in a handler (via the [`FromRequestParts`] impl) to tag your own logs, or read
/// it from the response's `x-request-id` header on the client side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(pub String);

impl<S> FromRequestParts<S> for RequestId
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ApiError>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<RequestId>()
            .cloned()
            .ok_or_else(|| ApiError::internal("request id middleware is not installed"))
    }
}

/// Resolve the incoming `x-request-id`, or mint a fresh UUID v4 when absent or empty.
fn resolve_request_id(headers: &HeaderMap) -> String {
    headers
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

/// Axum middleware that assigns a request correlation id and echoes it on the response.
///
/// It reuses an incoming `x-request-id` header when present (and non-empty), otherwise it
/// generates a UUID v4. The id is stored in request extensions (extractable via
/// [`RequestId`]) and written to the response `x-request-id` header. Requires the `trace`
/// feature.
///
/// # Example
///
/// ```rust,no_run
/// use axum::{middleware, routing::get, Router};
/// use axum_api_kit::propagate_request_id;
///
/// let app: Router = Router::new()
///     .route("/", get(|| async { "ok" }))
///     .layer(middleware::from_fn(propagate_request_id));
/// ```
pub async fn propagate_request_id(mut req: Request, next: Next) -> Response {
    let id = resolve_request_id(req.headers());
    req.extensions_mut().insert(RequestId(id.clone()));

    let mut res = next.run(req).await;
    if let Ok(value) = HeaderValue::from_str(&id) {
        res.headers_mut()
            .insert(HeaderName::from_static(REQUEST_ID_HEADER), value);
    }
    res
}

/// Axum middleware that emits a structured `tracing` event when each request completes.
///
/// The `info`-level event records `method`, `path`, `status`, `latency_ms`, and `request_id`
/// (the latter when [`propagate_request_id`] runs earlier in the stack). With no `tracing`
/// subscriber installed the event is a no-op. Requires the `trace` feature.
///
/// # Example
///
/// ```rust,no_run
/// use axum::{middleware, routing::get, Router};
/// use axum_api_kit::{propagate_request_id, trace_requests};
///
/// // The last `.layer` is the outermost: request ids are assigned before the trace event
/// // is recorded, so the event can include them.
/// let app: Router = Router::new()
///     .route("/", get(|| async { "ok" }))
///     .layer(middleware::from_fn(trace_requests))
///     .layer(middleware::from_fn(propagate_request_id));
/// ```
pub async fn trace_requests(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_owned();
    let request_id = req.extensions().get::<RequestId>().map(|id| id.0.clone());

    let start = Instant::now();
    let response = next.run(req).await;
    let latency_ms = start.elapsed().as_millis() as u64;

    tracing::info!(
        method = %method,
        path = %path,
        status = response.status().as_u16(),
        latency_ms,
        request_id = request_id.as_deref().unwrap_or("-"),
        "http request completed"
    );

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request as HttpRequest, middleware, routing::get, Router};
    use tower::ServiceExt;

    fn app() -> Router {
        Router::new()
            .route("/", get(|| async { "ok" }))
            .route("/id", get(|RequestId(id): RequestId| async move { id }))
            .layer(middleware::from_fn(trace_requests))
            .layer(middleware::from_fn(propagate_request_id))
    }

    #[test]
    fn resolve_uses_existing_header() {
        let mut headers = HeaderMap::new();
        headers.insert(REQUEST_ID_HEADER, HeaderValue::from_static("abc-123"));
        assert_eq!(resolve_request_id(&headers), "abc-123");
    }

    #[test]
    fn resolve_generates_uuid_when_absent() {
        let id = resolve_request_id(&HeaderMap::new());
        assert_eq!(id.len(), 36); // UUID v4 hyphenated form
    }

    #[test]
    fn resolve_generates_uuid_when_empty() {
        let mut headers = HeaderMap::new();
        headers.insert(REQUEST_ID_HEADER, HeaderValue::from_static(""));
        assert_eq!(resolve_request_id(&headers).len(), 36);
    }

    #[tokio::test]
    async fn response_carries_generated_request_id() {
        let res = app()
            .oneshot(HttpRequest::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let id = res
            .headers()
            .get(REQUEST_ID_HEADER)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(id.len(), 36);
    }

    #[tokio::test]
    async fn response_echoes_incoming_request_id() {
        let res = app()
            .oneshot(
                HttpRequest::builder()
                    .uri("/")
                    .header(REQUEST_ID_HEADER, "incoming-id")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.headers().get(REQUEST_ID_HEADER).unwrap(), "incoming-id");
    }

    #[tokio::test]
    async fn request_id_extractor_sees_value() {
        let res = app()
            .oneshot(
                HttpRequest::builder()
                    .uri("/id")
                    .header(REQUEST_ID_HEADER, "extract-me")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"extract-me");
    }
}

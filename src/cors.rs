use axum::http::{header, HeaderValue, Method};
use tower_http::cors::{AllowOrigin, CorsLayer};

/// A permissive CORS layer (any origin, method, and header).
///
/// Convenient for local development; prefer [`cors_allowing`] in production so credentialed
/// requests and a known origin allow-list are handled correctly. Requires the `cors` feature.
pub fn permissive_cors() -> CorsLayer {
    CorsLayer::permissive()
}

/// Build a CORS layer that allows the given origins.
///
/// Allows the common REST methods (`GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS`), the
/// `content-type` and `authorization` request headers, and credentials. Origins that fail to
/// parse as header values are skipped. Requires the `cors` feature.
///
/// # Example
///
/// ```rust,no_run
/// use axum::{routing::get, Router};
/// use axum_api_kit::cors_allowing;
///
/// let app: Router = Router::new()
///     .route("/", get(|| async { "ok" }))
///     .layer(cors_allowing(["https://app.example.com"]));
/// ```
pub fn cors_allowing<I, S>(origins: I) -> CorsLayer
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let origins: Vec<HeaderValue> = origins
        .into_iter()
        .filter_map(|origin| HeaderValue::from_str(origin.as_ref()).ok())
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .allow_credentials(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::get, Router};
    use tower::ServiceExt;

    #[tokio::test]
    async fn allows_listed_origin() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(cors_allowing(["https://app.example.com"]));

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("origin", "https://app.example.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            res.headers().get("access-control-allow-origin").unwrap(),
            "https://app.example.com"
        );
    }

    #[tokio::test]
    async fn omits_header_for_unlisted_origin() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(cors_allowing(["https://app.example.com"]));

        let res = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("origin", "https://evil.example.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(res.headers().get("access-control-allow-origin").is_none());
    }
}

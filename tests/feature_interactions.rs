#![cfg(all(feature = "router", feature = "trace", feature = "cors"))]

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware,
    Router,
};
use axum_api_kit::{
    cors_allowing, health_routes, propagate_request_id, trace_requests, HealthResponse,
    REQUEST_ID_HEADER,
};
use tower::ServiceExt;

#[tokio::test]
async fn healthz_with_trace_and_cors_echoes_request_id_and_origin() {
    let app: Router = Router::new()
        .merge(health_routes(|| async { HealthResponse::ok() }))
        .layer(cors_allowing(["https://app.example.com"]))
        .layer(middleware::from_fn(trace_requests))
        .layer(middleware::from_fn(propagate_request_id));

    let res = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header("origin", "https://app.example.com")
                .header(REQUEST_ID_HEADER, "trace-test-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers().get(REQUEST_ID_HEADER).unwrap(), "trace-test-id");
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        "https://app.example.com"
    );
}

#[tokio::test]
async fn readyz_unhealthy_still_emits_request_id() {
    let app: Router = Router::new()
        .merge(health_routes(|| async { HealthResponse::unhealthy() }))
        .layer(middleware::from_fn(trace_requests))
        .layer(middleware::from_fn(propagate_request_id));

    let res = app
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .header(REQUEST_ID_HEADER, "unhealthy-request")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        res.headers().get(REQUEST_ID_HEADER).unwrap(),
        "unhealthy-request"
    );
}

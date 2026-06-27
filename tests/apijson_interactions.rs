#![cfg(all(feature = "extract", feature = "trace", feature = "cors"))]

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use axum_api_kit::{
    cors_allowing, propagate_request_id, trace_requests, ApiJson, REQUEST_ID_HEADER,
};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;

#[derive(Debug, Deserialize)]
struct CreateWidget {
    name: String,
}

#[derive(Debug, Serialize)]
struct CreateWidgetResponse {
    id: String,
}

async fn create_widget(ApiJson(input): ApiJson<CreateWidget>) -> impl IntoResponse {
    let id = format!("widget-{}", input.name);
    (StatusCode::CREATED, Json(CreateWidgetResponse { id }))
}

#[tokio::test]
async fn invalid_json_rejection_is_structured_and_keeps_correlation_headers() {
    let app: Router = Router::new()
        .route("/widgets", post(create_widget))
        .layer(cors_allowing(["https://app.example.com"]))
        .layer(middleware::from_fn(trace_requests))
        .layer(middleware::from_fn(propagate_request_id));

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/widgets")
                .header("origin", "https://app.example.com")
                .header(REQUEST_ID_HEADER, "req-bad-json")
                .header("content-type", "application/json")
                .body(Body::from("{not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(res.headers().get(REQUEST_ID_HEADER).unwrap(), "req-bad-json");
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        "https://app.example.com"
    );

    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["code"], "INVALID_JSON");
}

#[tokio::test]
async fn unsupported_media_type_rejection_is_structured() {
    let app: Router = Router::new().route("/widgets", post(create_widget));

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/widgets")
                .body(Body::from(r#"{"name":"alpha"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["code"], "UNSUPPORTED_MEDIA_TYPE");
}

#[tokio::test]
async fn missing_content_type_rejection_is_unsupported_media_type_with_headers() {
    let app: Router = Router::new()
        .route("/widgets", post(create_widget))
        .layer(cors_allowing(["https://app.example.com"]))
        .layer(middleware::from_fn(trace_requests))
        .layer(middleware::from_fn(propagate_request_id));

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/widgets")
                .header("origin", "https://app.example.com")
                .header(REQUEST_ID_HEADER, "req-missing-content-type")
                .body(Body::from(r#"{"name":"alpha"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(
        res.headers().get(REQUEST_ID_HEADER).unwrap(),
        "req-missing-content-type"
    );
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        "https://app.example.com"
    );

    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["code"], "UNSUPPORTED_MEDIA_TYPE");
}

#[tokio::test]
async fn wrong_shape_rejection_is_invalid_body_and_keeps_headers() {
    let app: Router = Router::new()
        .route("/widgets", post(create_widget))
        .layer(cors_allowing(["https://app.example.com"]))
        .layer(middleware::from_fn(trace_requests))
        .layer(middleware::from_fn(propagate_request_id));

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/widgets")
                .header("origin", "https://app.example.com")
                .header(REQUEST_ID_HEADER, "req-wrong-shape")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"name":123}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res.headers().get(REQUEST_ID_HEADER).unwrap(),
        "req-wrong-shape"
    );
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        "https://app.example.com"
    );

    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["code"], "INVALID_BODY");
}

#[tokio::test]
async fn wrong_content_type_rejection_is_unsupported_media_type_with_headers() {
    let app: Router = Router::new()
        .route("/widgets", post(create_widget))
        .layer(cors_allowing(["https://app.example.com"]))
        .layer(middleware::from_fn(trace_requests))
        .layer(middleware::from_fn(propagate_request_id));

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/widgets")
                .header("origin", "https://app.example.com")
                .header(REQUEST_ID_HEADER, "req-wrong-content-type")
                .header("content-type", "text/plain")
                .body(Body::from(r#"{"name":"alpha"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(
        res.headers().get(REQUEST_ID_HEADER).unwrap(),
        "req-wrong-content-type"
    );
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        "https://app.example.com"
    );

    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["code"], "UNSUPPORTED_MEDIA_TYPE");
}

#[tokio::test]
async fn json_content_type_with_charset_is_accepted_and_keeps_headers() {
    let app: Router = Router::new()
        .route("/widgets", post(create_widget))
        .layer(cors_allowing(["https://app.example.com"]))
        .layer(middleware::from_fn(trace_requests))
        .layer(middleware::from_fn(propagate_request_id));

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/widgets")
                .header("origin", "https://app.example.com")
                .header(REQUEST_ID_HEADER, "req-charset-json")
                .header("content-type", "application/json; charset=utf-8")
                .body(Body::from(r#"{"name":"alpha"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    assert_eq!(res.headers().get(REQUEST_ID_HEADER).unwrap(), "req-charset-json");
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        "https://app.example.com"
    );

    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["id"], "widget-alpha");
}

#[tokio::test]
async fn vendor_plus_json_content_type_is_accepted_and_keeps_headers() {
    let app: Router = Router::new()
        .route("/widgets", post(create_widget))
        .layer(cors_allowing(["https://app.example.com"]))
        .layer(middleware::from_fn(trace_requests))
        .layer(middleware::from_fn(propagate_request_id));

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/widgets")
                .header("origin", "https://app.example.com")
                .header(REQUEST_ID_HEADER, "req-vendor-json")
                .header("content-type", "application/vnd.api+json")
                .body(Body::from(r#"{"name":"alpha"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    assert_eq!(res.headers().get(REQUEST_ID_HEADER).unwrap(), "req-vendor-json");
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        "https://app.example.com"
    );

    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["id"], "widget-alpha");
}

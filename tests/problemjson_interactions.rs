#![cfg(all(feature = "problem", feature = "extract", feature = "validator"))]

//! End-to-end coverage for the problem-flavored extractors served from a real
//! `Router`: `ProblemJson` and `ProblemValidatedJson` rejections as RFC 9457
//! bodies, Accept-header negotiation of the rejection `Content-Type`, and the
//! success path passing through untouched.

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use axum_api_kit::{ProblemJson, ProblemValidatedJson, APPLICATION_PROBLEM_JSON};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower::ServiceExt;
use validator::Validate;

#[derive(Debug, Deserialize)]
struct CreateWidget {
    name: String,
}

#[derive(Debug, Deserialize, Validate)]
struct CreateAccount {
    #[validate(length(min = 2))]
    #[allow(dead_code)]
    name: String,
}

#[derive(Debug, Serialize)]
struct CreateWidgetResponse {
    id: String,
}

async fn create_widget(ProblemJson(input): ProblemJson<CreateWidget>) -> impl IntoResponse {
    let id = format!("widget-{}", input.name);
    (StatusCode::CREATED, Json(CreateWidgetResponse { id }))
}

async fn create_account(
    ProblemValidatedJson(_input): ProblemValidatedJson<CreateAccount>,
) -> StatusCode {
    StatusCode::CREATED
}

fn app() -> Router {
    Router::new()
        .route("/widgets", post(create_widget))
        .route("/accounts", post(create_account))
}

fn post_req(
    uri: &str,
    body: &str,
    content_type: Option<&str>,
    accept: Option<&str>,
) -> Request<Body> {
    let mut builder = Request::builder().method("POST").uri(uri);
    if let Some(value) = content_type {
        builder = builder.header(header::CONTENT_TYPE, value);
    }
    if let Some(value) = accept {
        builder = builder.header(header::ACCEPT, value);
    }
    builder.body(Body::from(body.to_owned())).unwrap()
}

async fn body_json(res: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn valid_body_passes_through_to_handler() {
    let res = app()
        .oneshot(post_req(
            "/widgets",
            r#"{"name":"alpha"}"#,
            Some("application/json"),
            None,
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let body = body_json(res).await;
    assert_eq!(body["id"], "widget-alpha");
}

#[tokio::test]
async fn malformed_json_rejects_with_problem_content_type_and_rfc_body() {
    let res = app()
        .oneshot(post_req(
            "/widgets",
            "{not json",
            Some("application/json"),
            None,
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        APPLICATION_PROBLEM_JSON
    );

    let body = body_json(res).await;
    assert_eq!(body["title"], "Bad Request");
    assert_eq!(body["status"], 400);
    assert_eq!(body["code"], "INVALID_JSON");
    assert!(body["detail"].is_string());
}

#[tokio::test]
async fn missing_content_type_rejects_with_problem_body() {
    let res = app()
        .oneshot(post_req("/widgets", r#"{"name":"alpha"}"#, None, None))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        APPLICATION_PROBLEM_JSON
    );

    let body = body_json(res).await;
    assert_eq!(body["title"], "Unsupported Media Type");
    assert_eq!(body["status"], 415);
    assert_eq!(body["code"], "UNSUPPORTED_MEDIA_TYPE");
}

#[tokio::test]
async fn rejection_negotiates_plain_json_when_client_prefers_it() {
    let res = app()
        .oneshot(post_req(
            "/widgets",
            "{not json",
            Some("application/json"),
            Some("application/json"),
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = body_json(res).await;
    assert_eq!(body["title"], "Bad Request");
    assert_eq!(body["code"], "INVALID_JSON");
}

#[tokio::test]
async fn rejection_keeps_problem_json_in_ambiguous_accept_cases() {
    for accept in [None, Some("*/*"), Some("text/html")] {
        let res = app()
            .oneshot(post_req(
                "/widgets",
                "{not json",
                Some("application/json"),
                accept,
            ))
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            res.headers().get(header::CONTENT_TYPE).unwrap(),
            APPLICATION_PROBLEM_JSON,
            "Accept: {accept:?}"
        );
    }
}

#[tokio::test]
async fn validation_failure_rejects_with_field_details_in_problem_body() {
    let res = app()
        .oneshot(post_req(
            "/accounts",
            r#"{"name":"a"}"#,
            Some("application/json"),
            None,
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        APPLICATION_PROBLEM_JSON
    );

    let body = body_json(res).await;
    assert_eq!(body["title"], "Unprocessable Entity");
    assert_eq!(body["status"], 422);
    assert_eq!(body["detail"], "validation failed");
    assert_eq!(body["code"], "VALIDATION_ERROR");
    assert_eq!(
        body["details"]["fields"]["name"],
        json!([{ "code": "length", "params": { "min": 2, "value": "a" } }])
    );
}

#[tokio::test]
async fn validation_failure_negotiates_plain_json_with_same_body() {
    let negotiated = app()
        .oneshot(post_req(
            "/accounts",
            r#"{"name":"a"}"#,
            Some("application/json"),
            Some("application/json"),
        ))
        .await
        .unwrap();
    assert_eq!(negotiated.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        negotiated.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let default = app()
        .oneshot(post_req(
            "/accounts",
            r#"{"name":"a"}"#,
            Some("application/json"),
            None,
        ))
        .await
        .unwrap();

    let negotiated_body = axum::body::to_bytes(negotiated.into_body(), usize::MAX)
        .await
        .unwrap();
    let default_body = axum::body::to_bytes(default.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        negotiated_body, default_body,
        "bodies must be byte-identical across formats"
    );
}

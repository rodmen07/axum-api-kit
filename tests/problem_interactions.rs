#![cfg(feature = "problem")]

//! End-to-end coverage for RFC 9457 `Problem` responses served from a real
//! `Router`: hand-built problems, `ApiError` bridges, and the always-on
//! Retry-After factories.

use std::time::Duration;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use axum_api_kit::{ApiError, Problem, APPLICATION_PROBLEM_JSON};
use serde_json::json;
use tower::ServiceExt;

async fn body_json(res: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn hand_built_problem_serves_full_rfc_shape() {
    async fn out_of_credit() -> impl IntoResponse {
        Problem::new(StatusCode::FORBIDDEN, "Insufficient credit")
            .with_type("https://example.com/probs/out-of-credit")
            .with_detail("Balance is 30, item costs 50")
            .with_instance("/account/12345/msgs/abc")
            .with_extension("balance", 30)
    }

    let app: Router = Router::new().route("/msgs", get(out_of_credit));

    let res = app
        .oneshot(Request::builder().uri("/msgs").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        APPLICATION_PROBLEM_JSON
    );

    let body = body_json(res).await;
    assert_eq!(
        body,
        json!({
            "type": "https://example.com/probs/out-of-credit",
            "title": "Insufficient credit",
            "status": 403,
            "detail": "Balance is 30, item costs 50",
            "instance": "/account/12345/msgs/abc",
            "balance": 30
        })
    );
}

#[tokio::test]
async fn bridged_apierror_serves_problem_wire_shape() {
    async fn missing_item() -> impl IntoResponse {
        ApiError::new("NOT_FOUND", "item 42 does not exist")
            .with_details(json!({ "id": 42 }))
            .into_problem(StatusCode::NOT_FOUND)
    }

    let app: Router = Router::new().route("/items/42", get(missing_item));

    let res = app
        .oneshot(
            Request::builder()
                .uri("/items/42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        APPLICATION_PROBLEM_JSON
    );

    let body = body_json(res).await;
    assert_eq!(
        body,
        json!({
            "title": "Not Found",
            "status": 404,
            "detail": "item 42 does not exist",
            "code": "NOT_FOUND",
            "details": { "id": 42 }
        })
    );
}

#[tokio::test]
async fn retry_after_factory_keeps_flat_apierror_shape() {
    async fn rate_limited() -> impl IntoResponse {
        ApiError::too_many_requests_with_retry_after("slow down", Duration::from_secs(30))
    }

    let app: Router = Router::new().route("/limited", get(rate_limited));

    let res = app
        .oneshot(
            Request::builder()
                .uri("/limited")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res.headers().get(header::RETRY_AFTER).unwrap(), "30");
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = body_json(res).await;
    assert_eq!(
        body,
        json!({ "code": "RATE_LIMITED", "message": "slow down" })
    );
}

#[tokio::test]
async fn problem_with_retry_after_sets_header_and_problem_content_type() {
    async fn overloaded() -> impl IntoResponse {
        Problem::new(StatusCode::SERVICE_UNAVAILABLE, "Service Unavailable")
            .with_detail("try again shortly")
            .with_retry_after(Duration::from_millis(1500))
    }

    let app: Router = Router::new().route("/overloaded", get(overloaded));

    let res = app
        .oneshot(
            Request::builder()
                .uri("/overloaded")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(res.headers().get(header::RETRY_AFTER).unwrap(), "2");
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        APPLICATION_PROBLEM_JSON
    );

    let body = body_json(res).await;
    assert_eq!(
        body,
        json!({
            "title": "Service Unavailable",
            "status": 503,
            "detail": "try again shortly"
        })
    );
}

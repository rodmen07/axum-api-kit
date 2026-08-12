#![cfg(feature = "problem")]

//! End-to-end coverage for RFC 9457 `Problem` responses served from a real
//! `Router`: hand-built problems, `ApiError` bridges, the always-on
//! Retry-After factories, and opt-in Accept-header content negotiation.

use std::time::Duration;

use axum::{
    body::Body,
    http::{header, HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use axum_api_kit::{ApiError, Problem, ProblemFormat, APPLICATION_PROBLEM_JSON};
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

/// The sibling rendering of the same value. `ApiError::details` treats an explicit JSON `null` as
/// absent and omits the key (`tests/default_response_contracts.rs`); the RFC 9457 flavour renders
/// that same `ApiError` a second time through its own bridge, so a fix applied only to the
/// `Serialize` impl would move the `null` here instead of removing it. Bound as a whole-body
/// equality, so a `"details": null` member reappearing fails on the extension SET, not on a
/// hand-listed key.
#[tokio::test]
async fn bridged_apierror_omits_a_null_details_extension() {
    async fn missing_item() -> impl IntoResponse {
        ApiError::new("NOT_FOUND", "item 42 does not exist")
            .with_details(json!(null))
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

    let body = body_json(res).await;
    assert_eq!(
        body,
        json!({
            "title": "Not Found",
            "status": 404,
            "detail": "item 42 does not exist",
            "code": "NOT_FOUND"
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

/// A router whose handler opts into negotiation via the `ProblemFormat`
/// extractor.
fn negotiating_app() -> Router {
    async fn missing_item(format: ProblemFormat) -> Response {
        Problem::new(StatusCode::NOT_FOUND, "Not Found")
            .with_detail("item 42 does not exist")
            .into_response_with(format)
    }

    Router::new().route("/items/42", get(missing_item))
}

fn get_with_accept(accept: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().uri("/items/42");
    if let Some(accept) = accept {
        builder = builder.header(header::ACCEPT, accept);
    }
    builder.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn negotiation_extractor_serves_plain_json_when_preferred() {
    let res = negotiating_app()
        .oneshot(get_with_accept(Some("application/json")))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = body_json(res).await;
    assert_eq!(
        body,
        json!({
            "title": "Not Found",
            "status": 404,
            "detail": "item 42 does not exist"
        })
    );
}

#[tokio::test]
async fn negotiation_extractor_keeps_problem_json_in_ambiguous_cases() {
    for accept in [None, Some("*/*"), Some("text/html")] {
        let res = negotiating_app()
            .oneshot(get_with_accept(accept))
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            res.headers().get(header::CONTENT_TYPE).unwrap(),
            APPLICATION_PROBLEM_JSON,
            "Accept: {accept:?}"
        );

        let body = body_json(res).await;
        assert_eq!(
            body,
            json!({
                "title": "Not Found",
                "status": 404,
                "detail": "item 42 does not exist"
            })
        );
    }
}

#[tokio::test]
async fn negotiation_from_header_map_honors_q_values() {
    async fn missing_item(headers: HeaderMap) -> Response {
        Problem::new(StatusCode::NOT_FOUND, "Not Found")
            .with_detail("item 42 does not exist")
            .into_response_for(&headers)
    }

    let app: Router = Router::new().route("/items/42", get(missing_item));

    let res = app
        .clone()
        .oneshot(get_with_accept(Some(
            "application/json;q=0.9, application/problem+json;q=0.5",
        )))
        .await
        .unwrap();
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let res = app
        .oneshot(get_with_accept(Some(
            "application/problem+json;q=0.9, application/json;q=0.5",
        )))
        .await
        .unwrap();
    assert_eq!(
        res.headers().get(header::CONTENT_TYPE).unwrap(),
        APPLICATION_PROBLEM_JSON
    );
}

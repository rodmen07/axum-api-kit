//! Response-level contracts for the three response types available with **no feature flags**.
//!
//! `ListResponse`, `CursorResponse` and `HealthResponse` are what a consumer gets from a bare
//! `axum-api-kit = "2"`, and they are the crate's oldest surfaces (all three landed in the first
//! commit, 2026-05-17). Until this file existed, none of their `IntoResponse` impls was exercised
//! anywhere in the repo under the default feature set:
//!
//! * `ListResponse` and `CursorResponse` had no response-level test at all — their unit tests call
//!   `serde_json::to_value`, which observes the `Serialize` derive and never builds a `Response`,
//!   so the status code and the `Content-Type` header were unobserved.
//! * `HealthResponse`'s status mapping was observed only through `health_routes` in `src/router.rs`
//!   and `tests/feature_interactions.rs`, both gated behind the `router` feature. That covers the
//!   CONSUMER; the PRODUCER is what `lib.rs`'s own Quick Start tells users to return directly
//!   (`async fn health() -> impl IntoResponse { HealthResponse::ok() }`) with no features enabled.
//!
//! Deliberately NOT feature-gated: this file must run under plain `cargo test`, which is the
//! `Test, Lint, Format` job's default-feature step (`.github/workflows/ci.yml`). Every other
//! integration suite in `tests/` is `#![cfg(feature = ...)]`-gated and contributes zero tests
//! there, so before this file the default-feature run exercised no response behaviour at all.
//!
//! Assertions are one clause per test on purpose: a test aborts at its first failed assertion, so
//! bundling "status is 200" with "the body bytes are X" would let the status failure stand in for
//! the body one and report nothing about it.

use axum::{
    http::{header::CONTENT_TYPE, StatusCode},
    response::{IntoResponse, Response},
};
use axum_api_kit::{CursorResponse, HealthResponse, ListResponse};
use serde::Serialize;

#[derive(Serialize)]
struct Item {
    id: String,
}

/// Render a response and return its status, `Content-Type` and body bytes.
async fn parts(res: Response) -> (StatusCode, String, String) {
    let status = res.status();
    let content_type = res
        .headers()
        .get(CONTENT_TYPE)
        .map(|v| v.to_str().expect("content-type is valid utf-8").to_owned())
        .unwrap_or_else(|| "<absent>".to_owned());
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let body = String::from_utf8(bytes.to_vec()).expect("body is valid utf-8");
    (status, content_type, body)
}

fn list() -> ListResponse<Item> {
    ListResponse {
        data: vec![Item { id: "abc".into() }],
        total: 42,
        limit: 50,
        offset: 0,
    }
}

fn cursor_more() -> CursorResponse<Item> {
    CursorResponse {
        data: vec![Item { id: "abc".into() }],
        next_cursor: Some("cursor_for_page_2".into()),
        has_more: true,
    }
}

fn cursor_last_page() -> CursorResponse<Item> {
    CursorResponse {
        data: vec![],
        next_cursor: None,
        has_more: false,
    }
}

// --- ListResponse -----------------------------------------------------------------------------

#[tokio::test]
async fn list_response_status_is_200() {
    let (status, _, _) = parts(list().into_response()).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn list_response_content_type_is_application_json() {
    let (_, content_type, _) = parts(list().into_response()).await;
    assert_eq!(content_type, "application/json");
}

#[tokio::test]
async fn list_response_body_bytes_are_locked() {
    let (_, _, body) = parts(list().into_response()).await;
    assert_eq!(
        body,
        r#"{"data":[{"id":"abc"}],"total":42,"limit":50,"offset":0}"#
    );
}

// --- CursorResponse ---------------------------------------------------------------------------

#[tokio::test]
async fn cursor_response_status_is_200() {
    let (status, _, _) = parts(cursor_more().into_response()).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn cursor_response_content_type_is_application_json() {
    let (_, content_type, _) = parts(cursor_more().into_response()).await;
    assert_eq!(content_type, "application/json");
}

#[tokio::test]
async fn cursor_response_body_bytes_are_locked_with_a_next_cursor() {
    let (_, _, body) = parts(cursor_more().into_response()).await;
    assert_eq!(
        body,
        r#"{"data":[{"id":"abc"}],"next_cursor":"cursor_for_page_2","has_more":true}"#
    );
}

/// The last page OMITS `next_cursor` rather than sending `null`.
///
/// `next_cursor` carries `#[serde(skip_serializing_if = "Option::is_none")]`, so the key is absent
/// from the wire on the final page. The type's own rustdoc claimed `"next_cursor": null` here until
/// this test was written; the doc was corrected to match these bytes rather than the bytes changed
/// to match the doc, because the response bytes of a published 2.x type are frozen.
#[tokio::test]
async fn cursor_response_body_bytes_are_locked_on_the_last_page() {
    let (_, _, body) = parts(cursor_last_page().into_response()).await;
    assert_eq!(body, r#"{"data":[],"has_more":false}"#);
}

#[tokio::test]
async fn cursor_response_last_page_does_not_carry_a_null_next_cursor() {
    let (_, _, body) = parts(cursor_last_page().into_response()).await;
    assert!(
        !body.contains("next_cursor"),
        "the last page must omit `next_cursor` entirely, got: {body}"
    );
}

// --- HealthResponse ---------------------------------------------------------------------------

#[tokio::test]
async fn health_ok_status_is_200() {
    let (status, _, _) = parts(HealthResponse::ok().into_response()).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn health_degraded_status_is_200() {
    let (status, _, _) = parts(HealthResponse::degraded().into_response()).await;
    assert_eq!(status, StatusCode::OK);
}

/// The one that matters operationally: a load balancer acts on this status, not on the body.
#[tokio::test]
async fn health_unhealthy_status_is_503() {
    let (status, _, _) = parts(HealthResponse::unhealthy().into_response()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn health_content_type_is_application_json() {
    let (_, content_type, _) = parts(HealthResponse::unhealthy().into_response()).await;
    assert_eq!(content_type, "application/json");
}

#[tokio::test]
async fn health_ok_body_bytes_are_locked() {
    let (_, _, body) = parts(HealthResponse::ok().into_response()).await;
    assert_eq!(body, r#"{"status":"ok"}"#);
}

#[tokio::test]
async fn health_degraded_body_bytes_are_locked() {
    let (_, _, body) = parts(HealthResponse::degraded().into_response()).await;
    assert_eq!(body, r#"{"status":"degraded"}"#);
}

#[tokio::test]
async fn health_unhealthy_body_bytes_are_locked() {
    let (_, _, body) = parts(HealthResponse::unhealthy().into_response()).await;
    assert_eq!(body, r#"{"status":"unhealthy"}"#);
}

/// The private `status_code` field must never reach the wire.
#[tokio::test]
async fn health_body_never_leaks_the_status_code_field() {
    let (_, _, body) = parts(HealthResponse::unhealthy().into_response()).await;
    assert!(
        !body.contains("status_code"),
        "`status_code` is `#[serde(skip)]` and must stay off the wire, got: {body}"
    );
}

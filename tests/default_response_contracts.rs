//! Response-level contracts for the response types available with **no feature flags**.
//!
//! `ListResponse`, `CursorResponse`, `HealthResponse` and `ApiError` are what a consumer gets from
//! a bare `axum-api-kit = "2"`, and they are the crate's oldest surfaces (`ListResponse`,
//! `HealthResponse` and `ApiError` all landed in the same 2026-05-17 commit, `67e1c80`). Until this
//! file existed, none of their response-level behaviour was exercised anywhere in the repo under
//! the default feature set:
//!
//! * `ListResponse` and `CursorResponse` had no response-level test at all — their unit tests call
//!   `serde_json::to_value`, which observes the `Serialize` derive and never builds a `Response`,
//!   so the status code and the `Content-Type` header were unobserved.
//! * `HealthResponse`'s status mapping was observed only through `health_routes` in `src/router.rs`
//!   and `tests/feature_interactions.rs`, both gated behind the `router` feature. That covers the
//!   CONSUMER; the PRODUCER is what `lib.rs`'s own Quick Start tells users to return directly
//!   (`async fn health() -> impl IntoResponse { HealthResponse::ok() }`) with no features enabled.
//! * `ApiError` — the fourth surface from that same first commit, added here 2026-08-10 — had no
//!   response-level test either. Its unit tests in `src/error.rs` destructure the factory tuple
//!   (`let (status, Json(body)) = ApiError::not_found(..)`), which observes the tuple and the
//!   `Serialize` derive but never renders a `Response`, so the `Content-Type` of every error this
//!   crate emits, and whether the `Retry-After` header actually reaches the wire, were unobserved.
//!   `ApiError` appeared in `tests/` only in `tests/openapi.rs` (schema level, `openapi`-gated) and
//!   `tests/problem_interactions.rs` (as an input to the `Problem` bridges, `problem`-gated).
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
    http::{
        header::{CONTENT_TYPE, RETRY_AFTER},
        HeaderName, StatusCode,
    },
    response::{IntoResponse, Response},
    Json,
};
use axum_api_kit::{ApiError, CursorResponse, HealthResponse, ListResponse};
use serde::Serialize;
use serde_json::json;
use std::time::Duration;

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

/// The value of one header on a rendered response, or `"<absent>"`.
fn header(res: &Response, name: HeaderName) -> String {
    res.headers()
        .get(&name)
        .map(|v| v.to_str().expect("header value is valid utf-8").to_owned())
        .unwrap_or_else(|| "<absent>".to_owned())
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

// --- ApiError ---------------------------------------------------------------------------------
//
// The factories return `(StatusCode, Json<ApiError>)` — or `(StatusCode, [(HeaderName,
// HeaderValue); 1], Json<ApiError>)` for the `_with_retry_after` pair — and `ApiError`'s own
// rustdoc tells handlers to return that tuple directly. The tuple is where the crate's unit tests
// stop; these render it.

#[tokio::test]
async fn api_error_not_found_status_is_404() {
    let (status, _, _) = parts(ApiError::not_found("item not found").into_response()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_error_content_type_is_application_json() {
    let (_, content_type, _) = parts(ApiError::not_found("item not found").into_response()).await;
    assert_eq!(content_type, "application/json");
}

/// The exact shape `ApiError`'s rustdoc advertises: `{ "code": ..., "message": ... }`.
#[tokio::test]
async fn api_error_body_bytes_are_locked() {
    let (_, _, body) = parts(ApiError::not_found("item not found").into_response()).await;
    assert_eq!(body, r#"{"code":"NOT_FOUND","message":"item not found"}"#);
}

/// `details` is `skip_serializing_if = "Option::is_none"`, so an error without details omits the
/// key rather than sending `null` — the property `tests/openapi.rs` pins as `nullable = false`.
#[tokio::test]
async fn api_error_body_omits_details_when_none_is_set() {
    let (_, _, body) = parts(ApiError::not_found("item not found").into_response()).await;
    assert!(
        !body.contains("details"),
        "an error with no details must omit the key entirely, got: {body}"
    );
}

#[tokio::test]
async fn api_error_body_bytes_are_locked_with_details() {
    let err =
        ApiError::new("VALIDATION_ERROR", "invalid input").with_details(json!({ "field": "name" }));
    let (_, _, body) = parts((StatusCode::UNPROCESSABLE_ENTITY, Json(err)).into_response()).await;
    assert_eq!(
        body,
        r#"{"code":"VALIDATION_ERROR","message":"invalid input","details":{"field":"name"}}"#
    );
}

#[tokio::test]
async fn api_error_too_many_requests_with_retry_after_status_is_429() {
    let res =
        ApiError::too_many_requests_with_retry_after("slow down", Duration::from_millis(1500))
            .into_response();
    let (status, _, _) = parts(res).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

/// The header the whole factory exists for. Its rustdoc promises "1500ms becomes `"2"`"; until
/// this test, nothing checked that the header survives `into_response` at all.
#[test]
fn api_error_too_many_requests_sends_a_retry_after_header_rounded_up() {
    let res =
        ApiError::too_many_requests_with_retry_after("slow down", Duration::from_millis(1500))
            .into_response();
    assert_eq!(header(&res, RETRY_AFTER), "2");
}

/// The three-element tuple must not cost the JSON content type: `Json` sets it last.
#[tokio::test]
async fn api_error_with_retry_after_content_type_is_application_json() {
    let res = ApiError::too_many_requests_with_retry_after("slow down", Duration::from_secs(30))
        .into_response();
    let (_, content_type, _) = parts(res).await;
    assert_eq!(content_type, "application/json");
}

#[tokio::test]
async fn api_error_too_many_requests_with_retry_after_body_bytes_are_locked() {
    let res = ApiError::too_many_requests_with_retry_after("slow down", Duration::from_secs(30))
        .into_response();
    let (_, _, body) = parts(res).await;
    assert_eq!(body, r#"{"code":"RATE_LIMITED","message":"slow down"}"#);
}

#[tokio::test]
async fn api_error_service_unavailable_with_retry_after_status_is_503() {
    let res = ApiError::service_unavailable_with_retry_after(
        "down for maintenance",
        Duration::from_secs(120),
    )
    .into_response();
    let (status, _, _) = parts(res).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn api_error_service_unavailable_sends_a_retry_after_header() {
    let res = ApiError::service_unavailable_with_retry_after(
        "down for maintenance",
        Duration::from_secs(120),
    )
    .into_response();
    assert_eq!(header(&res, RETRY_AFTER), "120");
}

// --- Known gaps: `with_source` is silently lossy, and `details` can serialize as `null` ---------
//
// Three characterisation tests (L-052 shape, the `known_gap_` naming this repo already uses in
// `tests/openapi.rs`). They assert what the code does TODAY, not what it should do, so the two
// filed-not-fixed defects have a mechanical existence: fixing either one MUST redden its test, and
// that red is the signal to close the backlog entry and delete the test. Both fixes change bytes a
// published 2.x consumer can already observe, which is a decision about the shipped surface rather
// than a test fix, so this QA increment files them instead of taking them.

/// `with_details` REPLACES the whole `details` value, so a `source` added before it is dropped —
/// silently, with no error and no warning. This is the exact order `with_source`'s own rustdoc
/// example demonstrated until 2026-08-10 (that example asserts nothing, so it passed while showing
/// the data loss). The doc now shows the working order; the behaviour below is unchanged.
#[tokio::test]
async fn known_gap_with_source_is_dropped_when_with_details_follows_it() {
    let err = ApiError::new("NOT_FOUND", "user not found")
        .with_source("SELECT * FROM users WHERE id = ?")
        .with_details(json!({ "user_id": 42 }));
    let (_, _, body) = parts((StatusCode::NOT_FOUND, Json(err)).into_response()).await;
    assert_eq!(
        body, r#"{"code":"NOT_FOUND","message":"user not found","details":{"user_id":42}}"#,
        "GAP CLOSED: with_source now survives a later with_details — delete this test and close \
         the backlog entry"
    );
}

/// `with_source` writes into `details` only when `details` is a JSON OBJECT (`if let
/// Value::Object`). Any other `Value` — an array, a string, a number, `null` — silently discards
/// the source instead.
#[tokio::test]
async fn known_gap_with_source_is_dropped_when_details_is_not_an_object() {
    let err = ApiError::new("DB_ERROR", "query failed")
        .with_details(json!(["row 1", "row 2"]))
        .with_source("SELECT * FROM users");
    let (_, _, body) = parts((StatusCode::INTERNAL_SERVER_ERROR, Json(err)).into_response()).await;
    assert_eq!(
        body, r#"{"code":"DB_ERROR","message":"query failed","details":["row 1","row 2"]}"#,
        "GAP CLOSED: with_source now survives a non-object `details` — delete this test and close \
         the backlog entry"
    );
}

/// `details: Option<Value>` can hold `Value::Null`, and `skip_serializing_if = "Option::is_none"`
/// does not skip it, so `details` CAN reach the wire as `null` — contradicting both the field's own
/// rustdoc and the `schema(nullable = false)` that PR #16 gave it, which
/// `tests/openapi.rs::no_registered_property_admits_null` now enforces on the document side. The
/// document and the wire disagree for a value the public API can construct.
#[tokio::test]
async fn known_gap_api_error_details_can_serialize_as_null() {
    let err = ApiError::new("NOT_FOUND", "user not found").with_details(json!(null));
    let (_, _, body) = parts((StatusCode::NOT_FOUND, Json(err)).into_response()).await;
    assert_eq!(
        body, r#"{"code":"NOT_FOUND","message":"user not found","details":null}"#,
        "GAP CLOSED: `details` no longer serializes as null — delete this test and close the \
         backlog entry"
    );
}

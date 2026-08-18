//! Response-level contracts for the response types available with **no feature flags**.
//!
//! `ListResponse`, `CursorResponse`, `HealthResponse`, `ApiError` and the success-side helpers
//! `Created` / `Accepted` / `NoContent` are what a consumer gets from a bare
//! `axum-api-kit = "2"`, and they are the crate's oldest surfaces (`ListResponse`,
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
//! * `Created`, `Accepted` and `NoContent` — `src/success.rs`, shipped in 1.1.0 on 2026-06-07 and
//!   added here 2026-08-13 — appeared in `tests/` **not at all**: a `grep -rn` for their names
//!   across the whole directory returned zero hits before this section. Their six unit tests in
//!   `src/success.rs` do render a `Response`, unlike `ApiError`'s, but they read it through
//!   `serde_json::from_slice(..).unwrap_or(Value::Null)` and assert one navigated field
//!   (`body["id"] == "1"`), so the `Content-Type` of every created resource this crate emits, the
//!   exact body bytes, and everything axum's own service stack adds on the way out were
//!   unobserved. None of the three had ever been driven through a real `Router`.
//!
//! The serialization-failure block at the end of this file locks the correct answer for a body
//! that fails to serialize — `Json`'s own `500`, passed through untouched — a defect this file's
//! first coverage found shipping as `201`/`202` (filed 2026-08-13, pinned by five `known_gap_`
//! tests, fixed 2026-08-18 with the pins deleted per their own instruction). The `ListResponse` /
//! `CursorResponse` tests beside it take the identical unserializable payload and were the
//! crate's own demonstration of that answer while the gap stood.
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
    body::Body,
    http::{
        header::{CONTENT_LENGTH, CONTENT_TYPE, LOCATION, RETRY_AFTER},
        HeaderName, Request, StatusCode,
    },
    response::{IntoResponse, Response},
    routing::{delete, post},
    Json, Router,
};
use axum_api_kit::{
    Accepted, ApiError, Created, CursorResponse, HealthResponse, ListResponse, NoContent,
};
use serde::{Serialize, Serializer};
use serde_json::json;
use std::time::Duration;
use tower::ServiceExt;

#[derive(Serialize)]
struct Item {
    id: String,
}

/// A payload whose `Serialize` impl fails, the way a domain type that validates on the way out
/// would. Nothing about it is exotic: `serde` lets any `Serialize` impl return an error, and the
/// response types below are all generic over `T: Serialize`, so this input is reachable from
/// ordinary use of the public API.
struct Unserializable;

impl Serialize for Unserializable {
    fn serialize<S: Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("payload cannot be serialized"))
    }
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

/// `details` is `skip_serializing_if = "details_is_absent"`, so an error without details omits the
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

// --- `with_source` and `with_details` compose in either order -----------------------------------
//
// Both builders write into the single `details` field, and until 2026-08-10 either one could
// silently destroy the other's value: `with_details` replaced the whole value, dropping a `source`
// set before it, and `with_source` wrote only into a JSON OBJECT, dropping the source whenever
// `details` held an array, a string, a number or `null`. Five input shapes lost data; the two the
// backlog filed are the first two below.
//
// The six tests below are the fix's byte contract. One input shape per test rather than one
// combined assertion, because a test aborts at its first failed assertion, so a single merge
// regression would otherwise be reported by whichever shape happens to run first and say nothing
// about the rest. The last two shapes were already correct before the fix and their bytes are
// locked here to prove the fix changed the wire ONLY where data was being discarded.

/// `with_details` replaces the whole `details` value, so it must carry a `source` set before it
/// across the replacement. This is the exact order `with_source`'s own rustdoc example
/// demonstrated until 2026-08-10, when the example asserted nothing and so passed while showing
/// the data loss.
#[tokio::test]
async fn with_source_survives_a_later_with_details() {
    let err = ApiError::new("NOT_FOUND", "user not found")
        .with_source("SELECT * FROM users WHERE id = ?")
        .with_details(json!({ "user_id": 42 }));
    let (_, _, body) = parts((StatusCode::NOT_FOUND, Json(err)).into_response()).await;
    assert_eq!(
        body,
        r#"{"code":"NOT_FOUND","message":"user not found","details":{"source":"SELECT * FROM users WHERE id = ?","user_id":42}}"#
    );
}

/// A JSON object is the only `details` shape that can hold both the caller's value and a
/// `"source"`, so attaching a source to a non-object value nests that value under `"details"`
/// rather than discarding either side.
#[tokio::test]
async fn with_source_preserves_a_non_object_details_by_nesting_it() {
    let err = ApiError::new("DB_ERROR", "query failed")
        .with_details(json!(["row 1", "row 2"]))
        .with_source("SELECT * FROM users");
    let (_, _, body) = parts((StatusCode::INTERNAL_SERVER_ERROR, Json(err)).into_response()).await;
    assert_eq!(
        body,
        r#"{"code":"DB_ERROR","message":"query failed","details":{"details":["row 1","row 2"],"source":"SELECT * FROM users"}}"#
    );
}

/// The composition of the two shapes above, and the one the backlog entry did not name: a source
/// set FIRST and then replaced by a non-object `details`. It has to survive both the replacement
/// and the coercion.
#[tokio::test]
async fn with_source_survives_a_later_non_object_with_details() {
    let err = ApiError::new("DB_ERROR", "query failed")
        .with_source("SELECT * FROM users")
        .with_details(json!(["row 1", "row 2"]));
    let (_, _, body) = parts((StatusCode::INTERNAL_SERVER_ERROR, Json(err)).into_response()).await;
    assert_eq!(
        body,
        r#"{"code":"DB_ERROR","message":"query failed","details":{"details":["row 1","row 2"],"source":"SELECT * FROM users"}}"#
    );
}

/// The carry is not an override: a `"source"` the caller writes inside `details` is the later
/// explicit assignment and wins. These bytes are unchanged by the fix.
#[tokio::test]
async fn an_explicit_source_in_with_details_wins_over_an_earlier_with_source() {
    let err = ApiError::new("DB_ERROR", "query failed")
        .with_source("from with_source")
        .with_details(json!({ "source": "from with_details" }));
    let (_, _, body) = parts((StatusCode::INTERNAL_SERVER_ERROR, Json(err)).into_response()).await;
    assert_eq!(
        body,
        r#"{"code":"DB_ERROR","message":"query failed","details":{"source":"from with_details"}}"#
    );
}

/// The path that always worked: an object `details` keeps its keys and gains `"source"`. Locked in
/// bytes because the fix must not move it.
#[tokio::test]
async fn with_source_merges_into_an_object_details_unchanged() {
    let err = ApiError::new("DB_ERROR", "query failed")
        .with_details(json!({ "user_id": 123 }))
        .with_source("SELECT * FROM users");
    let (_, _, body) = parts((StatusCode::INTERNAL_SERVER_ERROR, Json(err)).into_response()).await;
    assert_eq!(
        body,
        r#"{"code":"DB_ERROR","message":"query failed","details":{"source":"SELECT * FROM users","user_id":123}}"#
    );
}

/// The other path that always worked: with no `details` set, the source creates the object. Also
/// locked in bytes so the coercion above cannot alter it.
#[tokio::test]
async fn with_source_alone_creates_the_details_object_unchanged() {
    let err = ApiError::new("DB_ERROR", "query failed").with_source("SELECT * FROM users");
    let (_, _, body) = parts((StatusCode::INTERNAL_SERVER_ERROR, Json(err)).into_response()).await;
    assert_eq!(
        body,
        r#"{"code":"DB_ERROR","message":"query failed","details":{"source":"SELECT * FROM users"}}"#
    );
}

// --- An explicit `null` details is absent, in every rendering -----------------------------------
//
// These replace `known_gap_api_error_details_can_serialize_as_null`, the L-052 characterisation pin
// that asserted the OLD wrong bytes (`{"code":..,"message":..,"details":null}`). Until 2.3.0
// `skip_serializing_if = "Option::is_none"` did not skip `Some(Value::Null)`, so the wire could
// send a `null` that `schema(nullable = false)` — enforced document-side by
// `tests/openapi.rs::no_registered_property_admits_null` — says cannot occur. `details_is_absent`
// closed that, and these bind the decision at the wire rather than restating it in prose.
//
// The reach of the fix is what these cover, because the builder is not the only way in: `details`
// is a `pub` field, `with_source` coerces a non-object `details` into a nested object, and the
// `Problem` bridge renders the same value a second time (`tests/problem_interactions.rs`). A fix
// applied at only one of those moves the `null` rather than removing it.

/// The filed repro: the builder path.
#[tokio::test]
async fn api_error_body_omits_details_given_an_explicit_null() {
    let err = ApiError::new("NOT_FOUND", "user not found").with_details(json!(null));
    let (_, _, body) = parts((StatusCode::NOT_FOUND, Json(err)).into_response()).await;
    assert_eq!(body, r#"{"code":"NOT_FOUND","message":"user not found"}"#);
}

/// `details` is `pub`, so the builder is not the only way to set it, and a fix that lived in
/// `with_details` would leave this path emitting the `null` the schema forbids.
#[tokio::test]
async fn api_error_body_omits_details_given_a_null_assigned_to_the_field() {
    let mut err = ApiError::new("NOT_FOUND", "user not found");
    err.details = Some(json!(null));
    let (_, _, body) = parts((StatusCode::NOT_FOUND, Json(err)).into_response()).await;
    assert_eq!(body, r#"{"code":"NOT_FOUND","message":"user not found"}"#);
}

/// Absence is `null` ONLY. This is the over-breadth control: an empty object is a value the caller
/// chose to attach, and skipping it would be a second, quieter data loss of the `with_source` kind.
#[tokio::test]
async fn api_error_body_keeps_an_empty_object_details() {
    let err = ApiError::new("VALIDATION_ERROR", "invalid input").with_details(json!({}));
    let (_, _, body) = parts((StatusCode::UNPROCESSABLE_ENTITY, Json(err)).into_response()).await;
    assert_eq!(
        body,
        r#"{"code":"VALIDATION_ERROR","message":"invalid input","details":{}}"#
    );
}

/// The same control for a falsy scalar: `false` is not `null`.
#[tokio::test]
async fn api_error_body_keeps_a_false_details() {
    let err = ApiError::new("VALIDATION_ERROR", "invalid input").with_details(json!(false));
    let (_, _, body) = parts((StatusCode::UNPROCESSABLE_ENTITY, Json(err)).into_response()).await;
    assert_eq!(
        body,
        r#"{"code":"VALIDATION_ERROR","message":"invalid input","details":false}"#
    );
}

/// `with_source` coerces a non-object `details` under a nested `"details"` key (PR #24). A `null`
/// must not take that path: nesting it would re-emit the banned value one level down, where the
/// field's own `skip_serializing_if` can no longer see it.
#[tokio::test]
async fn a_null_details_is_not_nested_when_a_source_is_attached() {
    let err = ApiError::new("DB_ERROR", "query failed")
        .with_details(json!(null))
        .with_source("SELECT 1");
    let (_, _, body) = parts((StatusCode::INTERNAL_SERVER_ERROR, Json(err)).into_response()).await;
    assert_eq!(
        body,
        r#"{"code":"DB_ERROR","message":"query failed","details":{"source":"SELECT 1"}}"#
    );
}

/// The other order, which reaches the nesting through `with_details`' source carry-across instead.
/// PR #24's guarantee that a source survives a later `with_details` still holds for a `null` one.
#[tokio::test]
async fn a_source_survives_a_later_null_with_details() {
    let err = ApiError::new("DB_ERROR", "query failed")
        .with_source("SELECT 1")
        .with_details(json!(null));
    let (_, _, body) = parts((StatusCode::INTERNAL_SERVER_ERROR, Json(err)).into_response()).await;
    assert_eq!(
        body,
        r#"{"code":"DB_ERROR","message":"query failed","details":{"source":"SELECT 1"}}"#
    );
}

// --- Created ----------------------------------------------------------------------------------
//
// `src/success.rs`, shipped in 1.1.0 (2026-06-07). First response-level coverage, 2026-08-13.

fn created() -> Created<Item> {
    Created::new(Item { id: "42".into() })
}

#[tokio::test]
async fn created_status_is_201() {
    let (status, _, _) = parts(created().into_response()).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn created_content_type_is_application_json() {
    let (_, content_type, _) = parts(created().into_response()).await;
    assert_eq!(content_type, "application/json");
}

#[tokio::test]
async fn created_body_bytes_are_locked() {
    let (_, _, body) = parts(created().into_response()).await;
    assert_eq!(body, r#"{"id":"42"}"#);
}

/// The resource is serialized DIRECTLY, not wrapped in a `data` envelope like `ListResponse`.
/// That is the documented REST convention this type exists to follow, and it is the half of the
/// contract a `body["id"]`-style assertion cannot see: an added envelope keeps `body["id"]`
/// working only if the navigation changes with it, whereas the byte lock above moves.
#[tokio::test]
async fn created_omits_the_location_header_when_none_is_set() {
    let res = created().into_response();
    assert_eq!(header(&res, LOCATION), "<absent>");
}

#[tokio::test]
async fn created_with_location_sets_the_header() {
    let res = created().with_location("/items/42").into_response();
    assert_eq!(header(&res, LOCATION), "/items/42");
}

#[tokio::test]
async fn created_with_location_leaves_the_body_bytes_unchanged() {
    let (_, _, body) = parts(created().with_location("/items/42").into_response()).await;
    assert_eq!(body, r#"{"id":"42"}"#);
}

/// Documented behaviour, pinned so it cannot drift into a panic: a value that cannot be a header
/// (here an embedded newline) drops the header rather than aborting the response.
#[tokio::test]
async fn created_omits_a_location_that_cannot_be_a_header_value() {
    let res = created().with_location("/items/\n42").into_response();
    assert_eq!(header(&res, LOCATION), "<absent>");
}

/// The other clause of that same behaviour, split out because a test stops at its first failed
/// assertion: dropping the header must not disturb the status.
#[tokio::test]
async fn created_still_reports_201_when_the_location_is_dropped() {
    let (status, _, _) = parts(created().with_location("/items/\n42").into_response()).await;
    assert_eq!(status, StatusCode::CREATED);
}

// --- Accepted ---------------------------------------------------------------------------------

fn accepted() -> Accepted<Item> {
    Accepted::new(Item { id: "job-1".into() })
}

#[tokio::test]
async fn accepted_status_is_202() {
    let (status, _, _) = parts(accepted().into_response()).await;
    assert_eq!(status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn accepted_content_type_is_application_json() {
    let (_, content_type, _) = parts(accepted().into_response()).await;
    assert_eq!(content_type, "application/json");
}

#[tokio::test]
async fn accepted_body_bytes_are_locked() {
    let (_, _, body) = parts(accepted().into_response()).await;
    assert_eq!(body, r#"{"id":"job-1"}"#);
}

// --- NoContent --------------------------------------------------------------------------------

#[tokio::test]
async fn no_content_status_is_204() {
    let (status, _, _) = parts(NoContent.into_response()).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn no_content_body_is_zero_bytes() {
    let (_, _, body) = parts(NoContent.into_response()).await;
    assert_eq!(body, "");
}

/// RFC 9110 §15.3.5: a 204 carries no content, so there is no representation to describe. An
/// `application/json` here would advertise a body that is never sent — the same docs-vs-wire
/// class this file's `ApiError` and `CursorResponse` sections were written for.
#[tokio::test]
async fn no_content_omits_the_content_type_header() {
    let (_, content_type, _) = parts(NoContent.into_response()).await;
    assert_eq!(content_type, "<absent>");
}

// --- Through a real Router --------------------------------------------------------------------
//
// The assertions above call `into_response()` directly, which is what `src/success.rs`'s own unit
// tests do. That observes the type and not the response a client receives: axum's service stack
// adds `content-length` on the way out, and a handler returns these types by value rather than as
// a `Response`. These drive the identical values through `Router::oneshot`, the pattern
// `tests/router_fallbacks.rs` established, so both layers are bound.

const CREATED_PATH: &str = "/items";
const ACCEPTED_PATH: &str = "/jobs";
const NO_CONTENT_PATH: &str = "/items/42";

async fn create_item() -> Created<Item> {
    created().with_location("/items/42")
}

async fn enqueue_job() -> Accepted<Item> {
    accepted()
}

async fn delete_item() -> NoContent {
    NoContent
}

fn success_router() -> Router {
    Router::new()
        .route(CREATED_PATH, post(create_item))
        .route(ACCEPTED_PATH, post(enqueue_job))
        .route(NO_CONTENT_PATH, delete(delete_item))
}

/// Drive one request through `success_router` and hand back the response.
async fn routed(method: &str, path: &str) -> Response {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .body(Body::empty())
        .expect("request builds");
    success_router()
        .oneshot(request)
        .await
        .expect("router is infallible")
}

#[tokio::test]
async fn routed_created_status_is_201() {
    let (status, _, _) = parts(routed("POST", CREATED_PATH).await).await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn routed_created_body_bytes_are_locked() {
    let (_, _, body) = parts(routed("POST", CREATED_PATH).await).await;
    assert_eq!(body, r#"{"id":"42"}"#);
}

#[tokio::test]
async fn routed_created_sets_the_location_header() {
    let res = routed("POST", CREATED_PATH).await;
    assert_eq!(header(&res, LOCATION), "/items/42");
}

#[tokio::test]
async fn routed_accepted_status_is_202() {
    let (status, _, _) = parts(routed("POST", ACCEPTED_PATH).await).await;
    assert_eq!(status, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn routed_no_content_status_is_204() {
    let (status, _, _) = parts(routed("DELETE", NO_CONTENT_PATH).await).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

/// The clause that only the routed layer can observe: `into_response()` alone sets no
/// `content-length` at all, so a 204 that started carrying a body would look identical there.
#[tokio::test]
async fn routed_no_content_declares_a_zero_content_length() {
    let res = routed("DELETE", NO_CONTENT_PATH).await;
    assert_eq!(header(&res, CONTENT_LENGTH), "0");
}

#[tokio::test]
async fn routed_no_content_body_is_zero_bytes() {
    let (_, _, body) = parts(routed("DELETE", NO_CONTENT_PATH).await).await;
    assert_eq!(body, "");
}

// --- The serialization-failure contract -------------------------------------------------------
//
// `Created` and `Accepted` render through `Json` and stamp their success status ONLY over the
// `200 OK` that `Json` produces on success. When `T`'s `Serialize` impl fails, `Json`'s own
// answer — `500 Internal Server Error` with a `text/plain` serde error body — passes through
// untouched, and `Created` withholds its `Location` header: a client must never be told a
// resource was created, or be handed a `Location` to follow, by a response whose body was lost.
//
// Until 2026-08-18 both types built the response as `(StatusCode::CREATED, Json(self.data))`,
// whose tuple `IntoResponse` stamps the status unconditionally AFTER `Json` has rendered, so a
// failing body shipped under `201`/`202` with the serde error as the body — filed as a MED bug
// and pinned by five `known_gap_` tests that stood exactly here. The fix reddened all five with
// their GAP-CLOSED messages and they are deleted per their own instruction; the tests below lock
// the corrected behaviour, one clause per test.

#[tokio::test]
async fn created_reports_500_when_the_body_fails_to_serialize() {
    let (status, _, _) = parts(Created::new(Unserializable).into_response()).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn created_serialization_failure_content_type_is_text_plain() {
    let (_, content_type, _) = parts(Created::new(Unserializable).into_response()).await;
    assert_eq!(content_type, "text/plain; charset=utf-8");
}

#[tokio::test]
async fn created_serialization_failure_body_is_the_serde_error() {
    let (_, _, body) = parts(Created::new(Unserializable).into_response()).await;
    assert_eq!(body, "payload cannot be serialized");
}

/// The decision the fix implements is "mirror `Json`'s own answer", so the whole response triple
/// is asserted equal to `Json`'s: if axum ever changes what a failed serialization renders as,
/// the pass-through follows it and this stays green; if the pass-through is ever replaced by a
/// hand-built response, this is the test that names the divergence. The three absolute clauses
/// above stand beside it because a differential alone cannot pin what either side actually says.
#[tokio::test]
async fn created_serialization_failure_answer_matches_axum_jsons_own() {
    let ours = parts(Created::new(Unserializable).into_response()).await;
    let axums = parts(Json(Unserializable).into_response()).await;
    assert_eq!(ours, axums);
}

/// The sharpest clause of the old gap, inverted: a client that follows `Location` must never be
/// sent to a resource that was not created, so the header does not survive the failure.
#[tokio::test]
async fn created_sends_no_location_when_the_body_fails_to_serialize() {
    let res = Created::new(Unserializable)
        .with_location("/items/42")
        .into_response();
    assert_eq!(header(&res, LOCATION), "<absent>");
}

#[tokio::test]
async fn accepted_reports_500_when_the_body_fails_to_serialize() {
    let (status, _, _) = parts(Accepted::new(Unserializable).into_response()).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn accepted_serialization_failure_answer_matches_axum_jsons_own() {
    let ours = parts(Accepted::new(Unserializable).into_response()).await;
    let axums = parts(Json(Unserializable).into_response()).await;
    assert_eq!(ours, axums);
}

/// The same contract through a real `Router`, because that is where a consumer meets it: the
/// status is not an artefact of calling `into_response()` by hand.
#[tokio::test]
async fn routed_created_reports_500_when_the_body_fails_to_serialize() {
    async fn broken() -> Created<Unserializable> {
        Created::new(Unserializable)
    }
    let request = Request::builder()
        .method("POST")
        .uri("/broken")
        .body(Body::empty())
        .expect("request builds");
    let res = Router::new()
        .route("/broken", post(broken))
        .oneshot(request)
        .await
        .expect("router is infallible");
    let (status, _, _) = parts(res).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

// --- The contrast that scoped the gap ---------------------------------------------------------
//
// `ListResponse` and `CursorResponse` are generic over the same `T: Serialize` and take the same
// unserializable payload; they render as a bare `Json(self).into_response()` with no status
// written over it, and were the crate's own demonstration that `500` is the reachable correct
// answer while the gap above stood. They lock the same contract from the type family that never
// had the defect.

#[tokio::test]
async fn list_response_reports_500_when_the_body_fails_to_serialize() {
    let list = ListResponse {
        data: vec![Unserializable],
        total: 1,
        limit: 50,
        offset: 0,
    };
    let (status, _, _) = parts(list.into_response()).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn cursor_response_reports_500_when_the_body_fails_to_serialize() {
    let cursor = CursorResponse {
        data: vec![Unserializable],
        next_cursor: None,
        has_more: false,
    };
    let (status, _, _) = parts(cursor.into_response()).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

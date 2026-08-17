//! Response-level contracts for `Revalidated<T>` (v2.3.0 candidate C1, conditional GETs).
//!
//! Everything user-visible is asserted through a real `Router` driven by `oneshot` — status,
//! `ETag`'s exact bytes, `Content-Type`, and the exact body BYTES — never by reading the source;
//! one clause per test (L-072), matching `tests/default_response_contracts.rs`. The expected
//! entity-tags are hardcoded byte-for-byte: FNV-1a 64 is a pinned public algorithm (the module's
//! own `fnv_reference_vectors` unit test binds it to the published vectors), so the tag of a
//! known body is a constant, and a drift in the hash, the hex rendering, or the quoting shows up
//! here as a byte diff rather than as two halves of one bug agreeing with each other.
//!
//! One deliberate divergence from the ROADMAP clause as first written, measured rather than
//! inherited (L-040): the clause asked for "the absence of `Content-Type` and `Content-Length`"
//! on the routed 304, but axum's routing layer inserts `content-length: 0` into EVERY bodyless
//! response it forwards (`Route::poll` -> `set_content_length`, axum-0.8.9 `routing/route.rs`;
//! RFC 9110 §8.6 explicitly permits the header on a 304) — the same behaviour
//! `tests/default_response_contracts.rs` already binds for `NoContent`'s routed 204. So
//! `Content-Length` absence is bound at the layer the wrapper controls (`into_response()`), the
//! routed `content-length: 0` is bound as axum's documented addition, and `Content-Type` absence
//! is bound at both layers.

use axum::{
    body::Body,
    http::{
        header::{CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_NONE_MATCH},
        HeaderName, Method, Request, StatusCode,
    },
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use axum_api_kit::Revalidated;
use serde::{ser, Serialize};
use tower::ServiceExt;

/// The routed path serving `{"id":"1"}`.
const ITEM_PATH: &str = "/items/1";
/// A second routed path serving `{"id":"2"}`, for the different-body clause.
const OTHER_ITEM_PATH: &str = "/items/2";
/// A path whose handler supplies its own validator via `with_etag`.
const CUSTOM_PATH: &str = "/custom";
/// A path whose handler's `with_etag` value is not a legal entity-tag.
const INVALID_CUSTOM_PATH: &str = "/custom-invalid";
/// A path whose body fails to serialize.
const FAILING_PATH: &str = "/failing";

/// The exact bytes `ITEM_PATH` serves.
const ITEM_BODY: &str = "{\"id\":\"1\"}";
/// FNV-1a 64 of `ITEM_BODY`'s bytes, quoted as the wrapper emits it.
const ITEM_ETAG: &str = "\"f4a186431ea3452d\"";
/// FNV-1a 64 of `{"id":"2"}` — leading zero, so this also pins the 16-digit padding.
const OTHER_ITEM_ETAG: &str = "\"0f77fd432e14ef20\"";
/// The tag `CUSTOM_PATH` supplies, unquoted as `with_etag` takes it.
const CUSTOM_TAG: &str = "sha256-abc123";
/// `CUSTOM_TAG` as it appears on the wire.
const CUSTOM_ETAG: &str = "\"sha256-abc123\"";

#[derive(Serialize)]
struct Item {
    id: String,
}

/// A payload whose `Serialize` impl always fails.
struct Unserializable;

impl Serialize for Unserializable {
    fn serialize<S: ser::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(ser::Error::custom("deliberately unserializable"))
    }
}

fn item(id: &str) -> Item {
    Item { id: id.into() }
}

fn app() -> Router {
    Router::new()
        .route(
            ITEM_PATH,
            get(|headers: axum::http::HeaderMap| async move {
                Revalidated::new(item("1")).if_none_match(headers.get(IF_NONE_MATCH))
            }),
        )
        .route(
            OTHER_ITEM_PATH,
            get(|| async { Revalidated::new(item("2")) }),
        )
        .route(
            CUSTOM_PATH,
            get(|headers: axum::http::HeaderMap| async move {
                Revalidated::new(item("1"))
                    .with_etag(CUSTOM_TAG)
                    .if_none_match(headers.get(IF_NONE_MATCH))
            }),
        )
        .route(
            INVALID_CUSTOM_PATH,
            get(|headers: axum::http::HeaderMap| async move {
                Revalidated::new(item("1"))
                    .with_etag("has a \" quote")
                    .if_none_match(headers.get(IF_NONE_MATCH))
            }),
        )
        .route(
            FAILING_PATH,
            get(|| async { Revalidated::new(Unserializable) }),
        )
}

/// `GET` a path with no conditional header.
async fn get_plain(path: &str) -> Response {
    app()
        .oneshot(
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("infallible")
}

/// `GET` a path carrying an `If-None-Match` value.
async fn get_conditional(path: &str, if_none_match: &str) -> Response {
    app()
        .oneshot(
            Request::builder()
                .uri(path)
                .header(IF_NONE_MATCH, if_none_match)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("infallible")
}

/// The value of one header on a response, or `"<absent>"`.
fn header(res: &Response, name: HeaderName) -> String {
    res.headers()
        .get(&name)
        .map(|v| v.to_str().expect("header value is valid utf-8").to_owned())
        .unwrap_or_else(|| "<absent>".to_owned())
}

/// A response's body as text.
async fn body_text(res: Response) -> String {
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("body collects");
    String::from_utf8(bytes.to_vec()).expect("body is valid utf-8")
}

// --- The 200 ----------------------------------------------------------------------------------

#[tokio::test]
async fn routed_200_status_is_200() {
    let res = get_plain(ITEM_PATH).await;
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn routed_200_etag_bytes_are_locked() {
    let res = get_plain(ITEM_PATH).await;
    assert_eq!(header(&res, ETAG), ITEM_ETAG);
}

#[tokio::test]
async fn routed_200_content_type_is_application_json() {
    let res = get_plain(ITEM_PATH).await;
    assert_eq!(header(&res, CONTENT_TYPE), "application/json");
}

#[tokio::test]
async fn routed_200_body_bytes_are_locked() {
    assert_eq!(body_text(get_plain(ITEM_PATH).await).await, ITEM_BODY);
}

#[tokio::test]
async fn a_different_body_hashes_to_a_different_etag() {
    let res = get_plain(OTHER_ITEM_PATH).await;
    assert_eq!(header(&res, ETAG), OTHER_ITEM_ETAG);
}

/// axum's `get` also serves `HEAD` (stripping the body at the routing layer); the validator
/// must survive that, or a client priming its cache via `HEAD` could never revalidate.
#[tokio::test]
async fn routed_head_carries_the_same_etag() {
    let res = app()
        .oneshot(
            Request::builder()
                .method(Method::HEAD)
                .uri(ITEM_PATH)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("infallible");
    assert_eq!(header(&res, ETAG), ITEM_ETAG);
}

// --- The 304 ----------------------------------------------------------------------------------

#[tokio::test]
async fn routed_matching_if_none_match_status_is_304() {
    let res = get_conditional(ITEM_PATH, ITEM_ETAG).await;
    assert_eq!(res.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn routed_304_body_is_zero_bytes() {
    let res = get_conditional(ITEM_PATH, ITEM_ETAG).await;
    assert_eq!(body_text(res).await, "");
}

#[tokio::test]
async fn routed_304_repeats_the_same_etag() {
    let res = get_conditional(ITEM_PATH, ITEM_ETAG).await;
    assert_eq!(header(&res, ETAG), ITEM_ETAG);
}

#[tokio::test]
async fn routed_304_carries_no_content_type() {
    let res = get_conditional(ITEM_PATH, ITEM_ETAG).await;
    assert_eq!(header(&res, CONTENT_TYPE), "<absent>");
}

/// The falsified-and-remeasured clause (module doc): the wrapper emits no `Content-Length`, and
/// what a routed client sees is axum's own `content-length: 0` — the identical addition the
/// routed 204 test in `tests/default_response_contracts.rs` binds. RFC 9110 §8.6 permits it.
#[tokio::test]
async fn routed_304_content_length_is_axums_zero() {
    let res = get_conditional(ITEM_PATH, ITEM_ETAG).await;
    assert_eq!(header(&res, CONTENT_LENGTH), "0");
}

/// The layer this wrapper controls: `into_response()` alone sets no `Content-Length` on a 304.
#[tokio::test]
async fn a_304_from_into_response_carries_no_content_length() {
    let value = axum::http::HeaderValue::from_static(ITEM_ETAG);
    let res = Revalidated::new(item("1"))
        .if_none_match(Some(&value))
        .into_response();
    assert_eq!(res.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(header(&res, CONTENT_LENGTH), "<absent>");
}

// --- Matching (RFC 9110 §13.1.2 weak comparison) ----------------------------------------------

#[tokio::test]
async fn a_weak_client_tag_still_matches() {
    let res = get_conditional(ITEM_PATH, "W/\"f4a186431ea3452d\"").await;
    assert_eq!(res.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn a_list_matches_when_any_member_matches() {
    let res = get_conditional(ITEM_PATH, "\"aaa\", W/\"f4a186431ea3452d\" , \"bbb\"").await;
    assert_eq!(res.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn star_matches() {
    let res = get_conditional(ITEM_PATH, "*").await;
    assert_eq!(res.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn a_non_matching_tag_answers_200() {
    let res = get_conditional(ITEM_PATH, "\"deadbeefdeadbeef\"").await;
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_non_matching_response_still_carries_the_etag() {
    let res = get_conditional(ITEM_PATH, "\"deadbeefdeadbeef\"").await;
    assert_eq!(header(&res, ETAG), ITEM_ETAG);
}

#[tokio::test]
async fn a_malformed_field_answers_200() {
    let res = get_conditional(ITEM_PATH, "f4a186431ea3452d").await;
    assert_eq!(res.status(), StatusCode::OK);
}

// --- The caller-supplied validator (`with_etag`) ----------------------------------------------

#[tokio::test]
async fn with_etag_replaces_the_computed_tag() {
    let res = get_plain(CUSTOM_PATH).await;
    assert_eq!(header(&res, ETAG), CUSTOM_ETAG);
}

#[tokio::test]
async fn with_etag_matching_answers_304() {
    let res = get_conditional(CUSTOM_PATH, CUSTOM_ETAG).await;
    assert_eq!(res.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn an_invalid_custom_etag_degrades_to_a_200_with_no_etag() {
    let res = get_plain(INVALID_CUSTOM_PATH).await;
    assert_eq!(header(&res, ETAG), "<absent>");
}

/// The documented consequence of the degradation: with no validator there is no revalidation,
/// even for `If-None-Match: *`.
#[tokio::test]
async fn an_invalid_custom_etag_never_answers_304() {
    let res = get_conditional(INVALID_CUSTOM_PATH, "*").await;
    assert_eq!(res.status(), StatusCode::OK);
}

// --- Serialization failure --------------------------------------------------------------------
//
// The deliberate divergence from `axum::Json` (which answers a `text/plain` serde-error body),
// and from `Created`/`Accepted`'s known gap (which report their success status over it): a
// failing body here is a structured `ApiError` under the correct status. If `Unserializable`
// were ever made serializable these fail loudly (200 != 500), so none of them is vacuous.

#[tokio::test]
async fn a_failing_body_answers_500() {
    let res = get_plain(FAILING_PATH).await;
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn a_failing_body_answers_a_structured_api_error() {
    let body = body_text(get_plain(FAILING_PATH).await).await;
    assert_eq!(
        body,
        "{\"code\":\"INTERNAL_ERROR\",\"message\":\"response body could not be serialized\"}"
    );
}

#[tokio::test]
async fn a_failing_body_content_type_is_application_json() {
    let res = get_plain(FAILING_PATH).await;
    assert_eq!(header(&res, CONTENT_TYPE), "application/json");
}

#[tokio::test]
async fn a_failing_body_carries_no_etag() {
    let res = get_plain(FAILING_PATH).await;
    assert_eq!(header(&res, ETAG), "<absent>");
}

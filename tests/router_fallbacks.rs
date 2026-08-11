#![cfg(feature = "router")]
//! Response-level contracts for the `router` feature's JSON fallbacks (v2.2.0 candidate C3).
//!
//! An app built from this kit answers every HANDLED failure with an `ApiError` body — and then
//! answers a path nobody routed, or a method a path does not support, with whatever axum's own
//! defaults send. Those two responses are the only ones in a kit-built app that a client's JSON
//! error handler cannot parse. `api_fallback` / `api_method_not_allowed` (and their
//! `problem`-gated siblings) fill exactly those two cases.
//!
//! **Both halves of the behaviour difference are bound here, in one file.** The `axum_default_*`
//! tests drive the SAME routes through a router with no fallbacks wired and record what axum
//! actually sends; the `api_*` and `problem_*` tests drive the same requests through a router
//! that wires them. Without the off-state tests, an on-state assertion could pass for a reason
//! that has nothing to do with this feature (a mis-built router, a request that never reached the
//! fallback), and nothing would say so. With them, deleting the wiring turns the on-state tests
//! red rather than quietly identical.
//!
//! Everything is asserted through a real `Router` driven by `oneshot` — status, `Content-Type`,
//! the `Allow` header, and the exact body BYTES — never by reading the source. One clause per
//! test, matching `tests/default_response_contracts.rs`: a test aborts at its first failed
//! assertion, so bundling "the status is 405" with "the body bytes are X" would let the status
//! failure stand in for the body one and report nothing about it.

use axum::{
    body::Body,
    http::{
        header::{ALLOW, CONTENT_TYPE},
        HeaderName, Method, Request, StatusCode,
    },
    response::Response,
    routing::get,
    Router,
};
use axum_api_kit::{api_fallback, api_method_not_allowed};
use tower::ServiceExt;

/// The one routed path every router in this file exposes, and the only method it accepts.
const ROUTED_PATH: &str = "/items";
/// A path no router in this file routes.
const UNMATCHED_PATH: &str = "/no-such-path";

async fn list_items() -> &'static str {
    "[]"
}

/// The off state: routes, no fallbacks. What a kit consumer gets today.
fn bare_router() -> Router {
    Router::new().route(ROUTED_PATH, get(list_items))
}

/// The on state, `ApiError` flavour.
fn api_router() -> Router {
    bare_router()
        .fallback(api_fallback)
        .method_not_allowed_fallback(api_method_not_allowed)
}

/// Render a response and return its status, `Content-Type` and body bytes as text.
async fn parts(res: Response) -> (StatusCode, String, String) {
    let status = res.status();
    let content_type = header(&res, CONTENT_TYPE);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("body collects");
    let body = String::from_utf8(bytes.to_vec()).expect("body is valid utf-8");
    (status, content_type, body)
}

/// The value of one header on a response, or `"<absent>"`.
fn header(res: &Response, name: HeaderName) -> String {
    res.headers()
        .get(&name)
        .map(|v| v.to_str().expect("header value is valid utf-8").to_owned())
        .unwrap_or_else(|| "<absent>".to_owned())
}

/// `GET` a path nobody routes.
async fn get_unmatched(app: Router) -> Response {
    app.oneshot(
        Request::builder()
            .uri(UNMATCHED_PATH)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .expect("router is infallible")
}

/// `POST` the routed path, which only accepts `GET`.
async fn post_routed(app: Router) -> Response {
    app.oneshot(
        Request::builder()
            .method(Method::POST)
            .uri(ROUTED_PATH)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .expect("router is infallible")
}

// ---------------------------------------------------------------------------
// The off state: what axum sends with no fallback wired. Measured, not quoted.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn axum_default_unmatched_path_status_is_404() {
    let (status, _, _) = parts(get_unmatched(bare_router()).await).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn axum_default_unmatched_path_body_is_empty() {
    let (_, _, body) = parts(get_unmatched(bare_router()).await).await;
    assert_eq!(body, "", "this empty body is the gap api_fallback fills");
}

#[tokio::test]
async fn axum_default_unmatched_path_has_no_content_type() {
    let (_, content_type, _) = parts(get_unmatched(bare_router()).await).await;
    assert_eq!(content_type, "<absent>");
}

#[tokio::test]
async fn axum_default_wrong_method_status_is_405() {
    let (status, _, _) = parts(post_routed(bare_router()).await).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn axum_default_wrong_method_body_is_empty() {
    let (_, _, body) = parts(post_routed(bare_router()).await).await;
    assert_eq!(
        body, "",
        "this empty body is the gap api_method_not_allowed fills"
    );
}

#[tokio::test]
async fn axum_default_wrong_method_sends_the_allow_header() {
    // The baseline for the "replacing the body does not drop the Allow header" claim below:
    // the header is present BEFORE the fallback is wired, so the on-state assertion is a
    // comparison rather than a lone observation.
    let res = post_routed(bare_router()).await;
    assert_eq!(header(&res, ALLOW), "GET,HEAD");
}

// ---------------------------------------------------------------------------
// The on state, ApiError flavour.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn api_fallback_leaves_a_routed_path_alone() {
    // Vacuity guard for every assertion below: the fallbacks must not swallow real routes, and a
    // router that answered everything with 404 would satisfy the unmatched-path tests trivially.
    let res = api_router()
        .oneshot(
            Request::builder()
                .uri(ROUTED_PATH)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router is infallible");
    let (status, _, body) = parts(res).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "[]"));
}

#[tokio::test]
async fn api_fallback_unmatched_path_status_is_404() {
    let (status, _, _) = parts(get_unmatched(api_router()).await).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn api_fallback_unmatched_path_content_type_is_json() {
    let (_, content_type, _) = parts(get_unmatched(api_router()).await).await;
    assert_eq!(content_type, "application/json");
}

#[tokio::test]
async fn api_fallback_unmatched_path_body_bytes_are_locked() {
    let (_, _, body) = parts(get_unmatched(api_router()).await).await;
    assert_eq!(
        body,
        r#"{"code":"NOT_FOUND","message":"no route matches this path"}"#
    );
}

#[tokio::test]
async fn api_method_not_allowed_status_is_405() {
    let (status, _, _) = parts(post_routed(api_router()).await).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn api_method_not_allowed_content_type_is_json() {
    let (_, content_type, _) = parts(post_routed(api_router()).await).await;
    assert_eq!(content_type, "application/json");
}

#[tokio::test]
async fn api_method_not_allowed_body_bytes_are_locked() {
    let (_, _, body) = parts(post_routed(api_router()).await).await;
    assert_eq!(
        body,
        r#"{"code":"METHOD_NOT_ALLOWED","message":"this path does not support the request method"}"#
    );
}

#[tokio::test]
async fn api_method_not_allowed_still_sends_the_allow_header() {
    let res = post_routed(api_router()).await;
    assert_eq!(header(&res, ALLOW), "GET,HEAD");
}

#[tokio::test]
async fn a_route_registered_after_the_method_fallback_keeps_axums_empty_405() {
    // The documented ordering requirement, pinned so the rustdoc cannot quietly become wrong.
    // `Router::method_not_allowed_fallback` rewrites the method fallback of the routes registered
    // BEFORE the call; this is the shape in which the handler silently appears to do nothing.
    let app = Router::new()
        .method_not_allowed_fallback(api_method_not_allowed)
        .route(ROUTED_PATH, get(list_items));
    let (_, _, body) = parts(post_routed(app).await).await;
    assert_eq!(body, "");
}

// ---------------------------------------------------------------------------
// The on state, RFC 9457 flavour. Requires `router` + `problem`.
// ---------------------------------------------------------------------------

#[cfg(feature = "problem")]
mod problem_flavour {
    use super::*;
    use axum_api_kit::{problem_fallback, problem_method_not_allowed, APPLICATION_PROBLEM_JSON};

    fn problem_router() -> Router {
        bare_router()
            .fallback(problem_fallback)
            .method_not_allowed_fallback(problem_method_not_allowed)
    }

    #[tokio::test]
    async fn problem_fallback_unmatched_path_status_is_404() {
        let (status, _, _) = parts(get_unmatched(problem_router()).await).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn problem_fallback_unmatched_path_content_type_is_problem_json() {
        let (_, content_type, _) = parts(get_unmatched(problem_router()).await).await;
        assert_eq!(content_type, APPLICATION_PROBLEM_JSON);
    }

    #[tokio::test]
    async fn problem_fallback_unmatched_path_body_bytes_are_locked() {
        let (_, _, body) = parts(get_unmatched(problem_router()).await).await;
        assert_eq!(
            body,
            r#"{"title":"Not Found","status":404,"detail":"no route matches this path","code":"NOT_FOUND"}"#
        );
    }

    #[tokio::test]
    async fn problem_method_not_allowed_status_is_405() {
        let (status, _, _) = parts(post_routed(problem_router()).await).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn problem_method_not_allowed_content_type_is_problem_json() {
        let (_, content_type, _) = parts(post_routed(problem_router()).await).await;
        assert_eq!(content_type, APPLICATION_PROBLEM_JSON);
    }

    #[tokio::test]
    async fn problem_method_not_allowed_body_bytes_are_locked() {
        let (_, _, body) = parts(post_routed(problem_router()).await).await;
        assert_eq!(
            body,
            r#"{"title":"Method Not Allowed","status":405,"detail":"this path does not support the request method","code":"METHOD_NOT_ALLOWED"}"#
        );
    }

    #[tokio::test]
    async fn problem_method_not_allowed_still_sends_the_allow_header() {
        let res = post_routed(problem_router()).await;
        assert_eq!(header(&res, ALLOW), "GET,HEAD");
    }

    // The two flavours are one error definition rendered twice, not two definitions kept in step.
    // These read BOTH rendered responses rather than the source, so replacing the shared builder
    // with two independent literals reddens them.

    #[tokio::test]
    async fn both_flavours_report_the_same_code_for_an_unmatched_path() {
        let (_, _, api) = parts(get_unmatched(api_router()).await).await;
        let (_, _, problem) = parts(get_unmatched(problem_router()).await).await;
        assert_eq!(code_of(&api), code_of(&problem));
    }

    #[tokio::test]
    async fn both_flavours_report_the_same_code_for_a_wrong_method() {
        let (_, _, api) = parts(post_routed(api_router()).await).await;
        let (_, _, problem) = parts(post_routed(problem_router()).await).await;
        assert_eq!(code_of(&api), code_of(&problem));
    }

    #[tokio::test]
    async fn both_flavours_report_the_same_status_for_an_unmatched_path() {
        let (api, _, _) = parts(get_unmatched(api_router()).await).await;
        let (problem, _, _) = parts(get_unmatched(problem_router()).await).await;
        assert_eq!(api, problem);
    }

    #[tokio::test]
    async fn both_flavours_report_the_same_status_for_a_wrong_method() {
        let (api, _, _) = parts(post_routed(api_router()).await).await;
        let (problem, _, _) = parts(post_routed(problem_router()).await).await;
        assert_eq!(api, problem);
    }

    /// The `code` member of a rendered body, from whichever flavour produced it: `ApiError` puts
    /// it at `code`, `Problem` carries it as a flattened RFC 9457 extension member under the same
    /// key. Panics rather than defaulting, so a body that lost the member cannot read as "equal".
    fn code_of(body: &str) -> String {
        let value: serde_json::Value = serde_json::from_str(body).expect("body is JSON");
        value["code"]
            .as_str()
            .unwrap_or_else(|| panic!("body carries no string `code` member: {body}"))
            .to_owned()
    }
}

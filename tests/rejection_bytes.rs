#![cfg(any(feature = "extract", feature = "validator"))]

//! Byte-identity locks on the `ApiJson` and `ValidatedJson` rejection wire
//! format.
//!
//! These extractors' rejection bodies are frozen under the 1.x stability
//! promise: the addition of the problem-flavored siblings (`ProblemJson` /
//! `ProblemValidatedJson`, v1.4.0) must not change a single byte here, and
//! neither may any future change. Each test asserts the exact status,
//! `Content-Type`, and body bytes for a fixed input.
//!
//! The `message` text for the JSON rejections comes from axum's own
//! `JsonRejection::body_text()`, pinned by Cargo.lock. If an axum upgrade
//! rewords a message these locks fail deliberately, so the release notes can
//! call the wire change out; the crate-controlled parts (status, `code`
//! values, key order, `Content-Type`) must never change at all.

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    response::Response,
};
use tower::ServiceExt;

async fn response_parts(res: Response) -> (StatusCode, String, Vec<u8>) {
    let status = res.status();
    let content_type = res
        .headers()
        .get(header::CONTENT_TYPE)
        .expect("rejection must set Content-Type")
        .to_str()
        .unwrap()
        .to_owned();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, content_type, bytes.to_vec())
}

fn post(body: &str, content_type: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method("POST").uri("/widgets");
    if let Some(value) = content_type {
        builder = builder.header(header::CONTENT_TYPE, value);
    }
    builder.body(Body::from(body.to_owned())).unwrap()
}

#[cfg(feature = "extract")]
mod apijson_bytes {
    use super::*;
    use axum::{routing::post as post_route, Router};
    use axum_api_kit::ApiJson;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Input {
        #[allow(dead_code)]
        name: String,
    }

    fn app() -> Router {
        async fn create(ApiJson(_input): ApiJson<Input>) -> StatusCode {
            StatusCode::CREATED
        }
        Router::new().route("/widgets", post_route(create))
    }

    #[tokio::test]
    async fn malformed_json_bytes_are_locked() {
        let res = app()
            .oneshot(post("{not json", Some("application/json")))
            .await
            .unwrap();
        let (status, content_type, bytes) = response_parts(res).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(content_type, "application/json");
        assert_eq!(
            bytes,
            br#"{"code":"INVALID_JSON","message":"Failed to parse the request body as JSON: key must be a string at line 1 column 2"}"#
        );
    }

    #[tokio::test]
    async fn wrong_shape_bytes_are_locked() {
        let res = app()
            .oneshot(post(r#"{"name":123}"#, Some("application/json")))
            .await
            .unwrap();
        let (status, content_type, bytes) = response_parts(res).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(content_type, "application/json");
        assert_eq!(
            bytes,
            br#"{"code":"INVALID_BODY","message":"Failed to deserialize the JSON body into the target type: name: invalid type: integer `123`, expected a string at line 1 column 11"}"#
        );
    }

    #[tokio::test]
    async fn missing_content_type_bytes_are_locked() {
        let res = app()
            .oneshot(post(r#"{"name":"alpha"}"#, None))
            .await
            .unwrap();
        let (status, content_type, bytes) = response_parts(res).await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(content_type, "application/json");
        assert_eq!(
            bytes,
            br#"{"code":"UNSUPPORTED_MEDIA_TYPE","message":"Expected request with `Content-Type: application/json`"}"#
        );
    }

    #[tokio::test]
    async fn wrong_content_type_bytes_are_locked() {
        let res = app()
            .oneshot(post(r#"{"name":"alpha"}"#, Some("text/plain")))
            .await
            .unwrap();
        let (status, content_type, bytes) = response_parts(res).await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(content_type, "application/json");
        assert_eq!(
            bytes,
            br#"{"code":"UNSUPPORTED_MEDIA_TYPE","message":"Expected request with `Content-Type: application/json`"}"#
        );
    }
}

#[cfg(feature = "validator")]
mod validated_bytes {
    use super::*;
    use axum::{routing::post as post_route, Router};
    use axum_api_kit::ValidatedJson;
    use serde::Deserialize;
    use validator::Validate;

    #[derive(Debug, Deserialize, Validate)]
    struct Input {
        #[validate(length(min = 2))]
        #[allow(dead_code)]
        name: String,
    }

    fn app() -> Router {
        async fn create(ValidatedJson(_input): ValidatedJson<Input>) -> StatusCode {
            StatusCode::CREATED
        }
        Router::new().route("/widgets", post_route(create))
    }

    #[tokio::test]
    async fn validation_failure_bytes_are_locked() {
        let res = app()
            .oneshot(post(r#"{"name":"a"}"#, Some("application/json")))
            .await
            .unwrap();
        let (status, content_type, bytes) = response_parts(res).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(content_type, "application/json");
        assert_eq!(
            bytes,
            br#"{"code":"VALIDATION_ERROR","message":"validation failed","details":{"fields":{"name":[{"code":"length","params":{"min":2,"value":"a"}}]}}}"#
        );
    }

    #[tokio::test]
    async fn malformed_json_bytes_are_locked() {
        let res = app()
            .oneshot(post("{not json", Some("application/json")))
            .await
            .unwrap();
        let (status, content_type, bytes) = response_parts(res).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(content_type, "application/json");
        assert_eq!(
            bytes,
            br#"{"code":"INVALID_JSON","message":"Failed to parse the request body as JSON: key must be a string at line 1 column 2"}"#
        );
    }
}

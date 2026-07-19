//! JSON body extractors that reject with RFC 9457 [`Problem`] responses.
//!
//! [`ProblemJson`] (features `problem` + `extract`) and [`ProblemValidatedJson`]
//! (features `problem` + `validator`) are the problem-flavored siblings of
//! [`ApiJson`](crate::ApiJson) and [`ValidatedJson`](crate::ValidatedJson):
//! same deserialization (and validation) behavior, same HTTP status codes, but
//! extraction failures short-circuit with an RFC 9457 problem details body
//! instead of the flat [`ApiError`](crate::ApiError) shape.
//!
//! # Why sibling types instead of changing the existing extractors
//!
//! `ApiJson` and `ValidatedJson` rejection bodies (and their public
//! `Rejection = (StatusCode, Json<ApiError>)` type) are frozen under the 1.x
//! stability promise. Cargo unifies features across the dependency graph, so
//! if enabling the `problem` feature changed those rejections, any transitive
//! dependency turning the feature on would silently reformat error responses
//! for unrelated users. The rejection format is therefore selected in code, by
//! naming the extractor type in the handler signature, exactly like choosing
//! `ApiJson` over `axum::Json`.
//!
//! # Failure mapping
//!
//! Failures reuse the exact `JsonRejection` -> [`ApiError`](crate::ApiError)
//! mapping the siblings use, bridged through
//! `From<(StatusCode, ApiError)> for Problem`, so the problem body carries the
//! same information as the flat rejection:
//!
//! | flat rejection | `Problem` member |
//! |---|---|
//! | HTTP status | `status` (identical), plus `title` from the canonical reason |
//! | `code` | `"code"` extension member (identical) |
//! | `message` | `detail` (identical) |
//! | `details` | `"details"` extension member (identical, when present) |
//!
//! # Content-Type negotiation
//!
//! Rejections negotiate their `Content-Type` from the request's `Accept`
//! headers via [`ProblemFormat::negotiate`]: `application/problem+json` in
//! every ambiguous case, plain `application/json` (byte-identical body) only
//! when the client strictly prefers it. Negotiation is built in because the
//! rejection fires before any handler code runs, so there is no later point
//! where a caller could apply it.

use axum::{
    extract::{FromRequest, Request},
    response::{IntoResponse, Response},
    Json,
};
use serde::de::DeserializeOwned;
#[cfg(feature = "extract")]
use serde::Serialize;
#[cfg(feature = "validator")]
use validator::Validate;

use crate::problem::{Problem, ProblemFormat};
#[cfg(feature = "validator")]
use crate::ApiError;

/// The rejection produced by [`ProblemJson`] and [`ProblemValidatedJson`]: an
/// RFC 9457 [`Problem`] plus the [`ProblemFormat`] negotiated from the
/// request's `Accept` headers.
///
/// [`IntoResponse`] delegates to [`Problem::into_response_with`], so the
/// response carries the negotiated `Content-Type` with the problem body. Both
/// fields are public, making the rejection inspectable in tests and reusable
/// as the rejection type of custom extractors.
///
/// Requires the `problem` feature plus at least one of `extract` or
/// `validator`.
///
/// # Example
///
/// ```rust
/// use axum::{http::{header, StatusCode}, response::IntoResponse};
/// use axum_api_kit::{Problem, ProblemFormat, ProblemRejection};
///
/// let rejection = ProblemRejection {
///     problem: Problem::new(StatusCode::UNPROCESSABLE_ENTITY, "Unprocessable Entity")
///         .with_detail("validation failed"),
///     format: ProblemFormat::ProblemJson,
/// };
/// let res = rejection.into_response();
/// assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
/// assert_eq!(
///     res.headers().get(header::CONTENT_TYPE).unwrap(),
///     "application/problem+json"
/// );
/// ```
#[derive(Debug, Clone)]
pub struct ProblemRejection {
    /// The RFC 9457 problem details body (status, title, detail, and the
    /// `code` / `details` extension members carried over from the flat
    /// rejection mapping).
    pub problem: Problem,
    /// The response format negotiated from the request's `Accept` headers.
    pub format: ProblemFormat,
}

impl IntoResponse for ProblemRejection {
    fn into_response(self) -> Response {
        self.problem.into_response_with(self.format)
    }
}

/// Maps a `JsonRejection` onto a [`Problem`] by reusing the shared
/// `JsonRejection` -> `ApiError` mapping and the lossless
/// `From<(StatusCode, Json<ApiError>)>` bridge, keeping the status and the
/// stable `code` identical to the sibling extractors.
fn json_rejection_to_problem(rejection: axum::extract::rejection::JsonRejection) -> Problem {
    Problem::from(crate::error::json_rejection_to_api_error(rejection))
}

/// The problem-flavored sibling of [`ApiJson`](crate::ApiJson): deserializes a
/// JSON request body exactly like `axum::Json`, but extraction failures reject
/// with an RFC 9457 [`Problem`] body via [`ProblemRejection`].
///
/// Status codes and the stable `code` extension member match
/// [`ApiJson`](crate::ApiJson)'s rejections exactly:
///
/// - malformed JSON -> `400 Bad Request`, code `INVALID_JSON`
/// - well-formed JSON of the wrong shape -> `422 Unprocessable Entity`, code
///   `INVALID_BODY`
/// - missing or incorrect `Content-Type` -> `415 Unsupported Media Type`, code
///   `UNSUPPORTED_MEDIA_TYPE`
///
/// The problem body carries the same information the flat rejection would:
/// `detail` is the same human-readable message, the stable `code` becomes the
/// `"code"` extension member, and `title` comes from the status's canonical
/// reason. The `Content-Type` is negotiated from the request's `Accept`
/// headers via [`ProblemFormat::negotiate`]. It also implements
/// [`IntoResponse`] (serializing as `200 OK` plain JSON), so it can be used
/// as a handler return type just like `ApiJson`.
///
/// Requires the `problem` and `extract` features.
///
/// # Example
///
/// ```rust,no_run
/// use axum_api_kit::ProblemJson;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct CreateUser {
///     name: String,
/// }
///
/// // Bad input short-circuits with an RFC 9457 problem details body.
/// async fn create_user(ProblemJson(user): ProblemJson<CreateUser>) {
///     let _ = user.name;
/// }
/// ```
#[cfg(feature = "extract")]
#[derive(Debug, Clone)]
pub struct ProblemJson<T>(pub T);

#[cfg(feature = "extract")]
impl<T, S> FromRequest<S> for ProblemJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ProblemRejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let format = ProblemFormat::negotiate(req.headers());
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(ProblemJson(value)),
            Err(rejection) => Err(ProblemRejection {
                problem: json_rejection_to_problem(rejection),
                format,
            }),
        }
    }
}

#[cfg(feature = "extract")]
impl<T: Serialize> IntoResponse for ProblemJson<T> {
    fn into_response(self) -> Response {
        Json(self.0).into_response()
    }
}

/// The problem-flavored sibling of [`ValidatedJson`](crate::ValidatedJson):
/// deserializes a JSON request body, validates it with
/// [`validator`](https://docs.rs/validator), and rejects with an RFC 9457
/// [`Problem`] body via [`ProblemRejection`] on failure.
///
/// Status codes and the stable `code` extension member match
/// [`ValidatedJson`](crate::ValidatedJson)'s rejections exactly:
///
/// - malformed JSON -> `400 Bad Request`, code `INVALID_JSON`
/// - well-formed JSON of the wrong shape -> `422 Unprocessable Entity`, code
///   `INVALID_BODY`
/// - missing or incorrect `Content-Type` -> `415 Unsupported Media Type`, code
///   `UNSUPPORTED_MEDIA_TYPE`
/// - validation failure -> `422 Unprocessable Entity`, code `VALIDATION_ERROR`
///
/// A validation failure carries the same field-level map the flat rejection
/// exposes, under the `"details"` extension member:
///
/// ```json
/// {
///   "title": "Unprocessable Entity",
///   "status": 422,
///   "detail": "validation failed",
///   "code": "VALIDATION_ERROR",
///   "details": { "fields": { "name": [{ "code": "length", "params": { "min": 2 } }] } }
/// }
/// ```
///
/// As with [`ProblemJson`](crate::ProblemJson), `detail` carries the flat
/// rejection's message, `code` becomes the `"code"` extension member, and the
/// `Content-Type` is negotiated from the request's `Accept` headers via
/// [`ProblemFormat::negotiate`].
///
/// Requires the `problem` and `validator` features.
///
/// # Example
///
/// ```rust,no_run
/// use axum_api_kit::ProblemValidatedJson;
/// use serde::Deserialize;
/// use validator::Validate;
///
/// #[derive(Deserialize, Validate)]
/// struct CreateUser {
///     #[validate(length(min = 1, max = 100))]
///     name: String,
///     #[validate(email)]
///     email: String,
/// }
///
/// // The body is deserialized and validated before the handler body runs;
/// // failures become RFC 9457 problem details responses.
/// async fn create_user(ProblemValidatedJson(user): ProblemValidatedJson<CreateUser>) {
///     let _ = (user.name, user.email);
/// }
/// ```
#[cfg(feature = "validator")]
#[derive(Debug, Clone)]
pub struct ProblemValidatedJson<T>(pub T);

#[cfg(feature = "validator")]
impl<T, S> FromRequest<S> for ProblemValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = ProblemRejection;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let format = ProblemFormat::negotiate(req.headers());
        let value = match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => value,
            Err(rejection) => {
                return Err(ProblemRejection {
                    problem: json_rejection_to_problem(rejection),
                    format,
                })
            }
        };
        if let Err(errors) = value.validate() {
            return Err(ProblemRejection {
                problem: Problem::from((
                    axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                    ApiError::from(errors),
                )),
                format,
            });
        }
        Ok(ProblemValidatedJson(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{header, StatusCode};

    #[tokio::test]
    async fn problem_rejection_into_response_respects_format_with_identical_bodies() {
        let rejection = |format| ProblemRejection {
            problem: Problem::new(StatusCode::BAD_REQUEST, "Bad Request").with_detail("nope"),
            format,
        };

        let default = rejection(ProblemFormat::ProblemJson).into_response();
        assert_eq!(default.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            default.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );

        let negotiated = rejection(ProblemFormat::Json).into_response();
        assert_eq!(negotiated.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            negotiated.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );

        let default_body = axum::body::to_bytes(default.into_body(), usize::MAX)
            .await
            .unwrap();
        let negotiated_body = axum::body::to_bytes(negotiated.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            default_body, negotiated_body,
            "bodies must be byte-identical across formats"
        );
    }

    #[cfg(feature = "extract")]
    mod problem_json {
        use super::*;
        use axum::body::Body;
        use axum::http::{header::ACCEPT, header::CONTENT_TYPE, Request};
        use serde::Deserialize;

        #[derive(Debug, Deserialize)]
        struct Input {
            name: String,
        }

        fn request(body: &str, content_type: Option<&str>, accept: Option<&str>) -> Request<Body> {
            let mut builder = Request::builder().method("POST").uri("/");
            if let Some(value) = content_type {
                builder = builder.header(CONTENT_TYPE, value);
            }
            if let Some(value) = accept {
                builder = builder.header(ACCEPT, value);
            }
            builder.body(Body::from(body.to_owned())).unwrap()
        }

        async fn extract(
            body: &str,
            content_type: Option<&str>,
        ) -> Result<Input, ProblemRejection> {
            ProblemJson::<Input>::from_request(request(body, content_type, None), &())
                .await
                .map(|ProblemJson(v)| v)
        }

        #[tokio::test]
        async fn valid_body_extracts() {
            let input = extract(r#"{"name":"abc"}"#, Some("application/json"))
                .await
                .unwrap();
            assert_eq!(input.name, "abc");
        }

        #[tokio::test]
        async fn malformed_json_is_invalid_json_problem() {
            let rejection = extract("{not json", Some("application/json"))
                .await
                .unwrap_err();
            assert_eq!(rejection.problem.status, 400);
            assert_eq!(rejection.problem.title, "Bad Request");
            assert_eq!(rejection.problem.extensions["code"], "INVALID_JSON");
            assert!(rejection.problem.detail.is_some());
            assert_eq!(rejection.format, ProblemFormat::ProblemJson);
        }

        #[tokio::test]
        async fn wrong_shape_is_invalid_body_problem() {
            let rejection = extract(r#"{"name":123}"#, Some("application/json"))
                .await
                .unwrap_err();
            assert_eq!(rejection.problem.status, 422);
            assert_eq!(rejection.problem.title, "Unprocessable Entity");
            assert_eq!(rejection.problem.extensions["code"], "INVALID_BODY");
        }

        #[tokio::test]
        async fn missing_content_type_is_unsupported_media_type_problem() {
            let rejection = extract(r#"{"name":"abc"}"#, None).await.unwrap_err();
            assert_eq!(rejection.problem.status, 415);
            assert_eq!(rejection.problem.title, "Unsupported Media Type");
            assert_eq!(
                rejection.problem.extensions["code"],
                "UNSUPPORTED_MEDIA_TYPE"
            );
        }

        #[tokio::test]
        async fn wrong_content_type_is_unsupported_media_type_problem() {
            let rejection = extract(r#"{"name":"abc"}"#, Some("text/plain"))
                .await
                .unwrap_err();
            assert_eq!(rejection.problem.status, 415);
            assert_eq!(
                rejection.problem.extensions["code"],
                "UNSUPPORTED_MEDIA_TYPE"
            );
        }

        #[tokio::test]
        async fn json_with_charset_is_accepted() {
            let input = extract(r#"{"name":"abc"}"#, Some("application/json; charset=utf-8"))
                .await
                .unwrap();
            assert_eq!(input.name, "abc");
        }

        #[tokio::test]
        async fn vendor_plus_json_is_accepted() {
            let input = extract(r#"{"name":"abc"}"#, Some("application/vnd.api+json"))
                .await
                .unwrap();
            assert_eq!(input.name, "abc");
        }

        #[tokio::test]
        async fn rejection_carries_same_information_as_apijson() {
            use crate::ApiJson;

            for (body, content_type) in [
                ("{not json", Some("application/json")),
                (r#"{"name":123}"#, Some("application/json")),
                (r#"{"name":"abc"}"#, None),
            ] {
                let problem_rejection =
                    ProblemJson::<Input>::from_request(request(body, content_type, None), &())
                        .await
                        .unwrap_err();
                let (status, Json(api_error)) =
                    ApiJson::<Input>::from_request(request(body, content_type, None), &())
                        .await
                        .unwrap_err();

                let problem = &problem_rejection.problem;
                assert_eq!(problem.status, status.as_u16(), "body {body:?}");
                assert_eq!(problem.extensions["code"], api_error.code, "body {body:?}");
                assert_eq!(
                    problem.detail.as_deref(),
                    Some(api_error.message.as_str()),
                    "body {body:?}"
                );
                assert_eq!(
                    problem.extensions.get("details"),
                    api_error.details.as_ref(),
                    "body {body:?}"
                );
            }
        }

        #[tokio::test]
        async fn rejection_negotiates_format_from_accept_header() {
            let rejection = ProblemJson::<Input>::from_request(
                request(
                    "{not json",
                    Some("application/json"),
                    Some("application/json"),
                ),
                &(),
            )
            .await
            .unwrap_err();
            assert_eq!(rejection.format, ProblemFormat::Json);
            let res = rejection.into_response();
            assert_eq!(res.status(), StatusCode::BAD_REQUEST);
            assert_eq!(
                res.headers().get(header::CONTENT_TYPE).unwrap(),
                "application/json"
            );

            for accept in [None, Some("*/*"), Some("text/html")] {
                let rejection = ProblemJson::<Input>::from_request(
                    request("{not json", Some("application/json"), accept),
                    &(),
                )
                .await
                .unwrap_err();
                assert_eq!(
                    rejection.format,
                    ProblemFormat::ProblemJson,
                    "Accept: {accept:?}"
                );
            }
        }

        #[tokio::test]
        async fn rejection_response_serves_problem_wire_shape() {
            let res = extract("{not json", Some("application/json"))
                .await
                .unwrap_err()
                .into_response();
            assert_eq!(res.status(), StatusCode::BAD_REQUEST);
            assert_eq!(
                res.headers().get(header::CONTENT_TYPE).unwrap(),
                "application/problem+json"
            );
            let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(body["title"], "Bad Request");
            assert_eq!(body["status"], 400);
            assert_eq!(body["code"], "INVALID_JSON");
            assert!(body["detail"].is_string());
        }

        #[tokio::test]
        async fn serializes_as_response() {
            use serde::Serialize;

            #[derive(Serialize)]
            struct Out {
                id: u32,
            }
            let res = ProblemJson(Out { id: 7 }).into_response();
            assert_eq!(res.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap();
            let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(v["id"], 7);
        }
    }

    #[cfg(feature = "validator")]
    mod problem_validated_json {
        use super::*;
        use axum::body::Body;
        use axum::http::{header::ACCEPT, header::CONTENT_TYPE, Request};
        use serde::Deserialize;

        #[derive(Debug, Deserialize, Validate)]
        struct Input {
            #[validate(length(min = 2))]
            name: String,
        }

        fn request(body: &str, content_type: Option<&str>, accept: Option<&str>) -> Request<Body> {
            let mut builder = Request::builder().method("POST").uri("/");
            if let Some(value) = content_type {
                builder = builder.header(CONTENT_TYPE, value);
            }
            if let Some(value) = accept {
                builder = builder.header(ACCEPT, value);
            }
            builder.body(Body::from(body.to_owned())).unwrap()
        }

        async fn extract(
            body: &str,
            content_type: Option<&str>,
        ) -> Result<Input, ProblemRejection> {
            ProblemValidatedJson::<Input>::from_request(request(body, content_type, None), &())
                .await
                .map(|ProblemValidatedJson(v)| v)
        }

        #[tokio::test]
        async fn valid_body_extracts() {
            let input = extract(r#"{"name":"abcd"}"#, Some("application/json"))
                .await
                .unwrap();
            assert_eq!(input.name, "abcd");
        }

        #[tokio::test]
        async fn malformed_json_is_invalid_json_problem() {
            let rejection = extract("{not json", Some("application/json"))
                .await
                .unwrap_err();
            assert_eq!(rejection.problem.status, 400);
            assert_eq!(rejection.problem.extensions["code"], "INVALID_JSON");
        }

        #[tokio::test]
        async fn wrong_shape_is_invalid_body_problem() {
            let rejection = extract(r#"{"name":123}"#, Some("application/json"))
                .await
                .unwrap_err();
            assert_eq!(rejection.problem.status, 422);
            assert_eq!(rejection.problem.extensions["code"], "INVALID_BODY");
        }

        #[tokio::test]
        async fn missing_content_type_is_unsupported_media_type_problem() {
            let rejection = extract(r#"{"name":"abcd"}"#, None).await.unwrap_err();
            assert_eq!(rejection.problem.status, 415);
            assert_eq!(
                rejection.problem.extensions["code"],
                "UNSUPPORTED_MEDIA_TYPE"
            );
        }

        #[tokio::test]
        async fn validation_failure_is_validation_error_problem_with_field_details() {
            let rejection = extract(r#"{"name":"a"}"#, Some("application/json"))
                .await
                .unwrap_err();
            assert_eq!(rejection.problem.status, 422);
            assert_eq!(rejection.problem.title, "Unprocessable Entity");
            assert_eq!(rejection.problem.extensions["code"], "VALIDATION_ERROR");
            assert_eq!(
                rejection.problem.detail.as_deref(),
                Some("validation failed")
            );
            assert!(rejection.problem.extensions["details"]["fields"]["name"].is_array());
        }

        #[tokio::test]
        async fn validation_failure_carries_same_information_as_validated_json() {
            use crate::ValidatedJson;

            let body = r#"{"name":"a"}"#;
            let problem_rejection = ProblemValidatedJson::<Input>::from_request(
                request(body, Some("application/json"), None),
                &(),
            )
            .await
            .unwrap_err();
            let (status, Json(api_error)) = ValidatedJson::<Input>::from_request(
                request(body, Some("application/json"), None),
                &(),
            )
            .await
            .unwrap_err();

            let problem = &problem_rejection.problem;
            assert_eq!(problem.status, status.as_u16());
            assert_eq!(problem.extensions["code"], api_error.code);
            assert_eq!(problem.detail.as_deref(), Some(api_error.message.as_str()));
            assert_eq!(
                problem.extensions.get("details"),
                api_error.details.as_ref()
            );
        }

        #[tokio::test]
        async fn validation_rejection_negotiates_format_from_accept_header() {
            let rejection = ProblemValidatedJson::<Input>::from_request(
                request(
                    r#"{"name":"a"}"#,
                    Some("application/json"),
                    Some("application/json"),
                ),
                &(),
            )
            .await
            .unwrap_err();
            assert_eq!(rejection.format, ProblemFormat::Json);
            let res = rejection.into_response();
            assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(
                res.headers().get(header::CONTENT_TYPE).unwrap(),
                "application/json"
            );
        }
    }
}

use axum::{
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{de::DeserializeOwned, Serialize};

use crate::error::json_rejection_to_api_error;
use crate::ApiError;

/// A drop-in replacement for [`axum::Json`] whose extraction failures reject with an
/// [`ApiError`] JSON body instead of Axum's default plain-text response.
///
/// As an extractor it deserializes the request body exactly like `axum::Json`, but on
/// failure it short-circuits the handler with `(StatusCode, Json<ApiError>)`:
///
/// - malformed JSON -> `400 Bad Request`, code `INVALID_JSON`
/// - well-formed JSON of the wrong shape -> `422 Unprocessable Entity`, code `INVALID_BODY`
/// - missing or incorrect `Content-Type` -> `415 Unsupported Media Type`, code
///   `UNSUPPORTED_MEDIA_TYPE`
///
/// The HTTP status is taken from Axum's own rejection, so it stays correct as Axum evolves.
/// It also implements [`IntoResponse`] (serializing as `200 OK` JSON), so it can be used as a
/// handler return type just like `axum::Json`.
///
/// Unlike [`ValidatedJson`](crate::ValidatedJson), `ApiJson` performs no validation and so
/// needs only the `extract` feature, not `validator`. Reach for `ValidatedJson` when you want
/// `validator`-based field validation as well.
///
/// Requires the `extract` feature.
///
/// # Example
///
/// ```rust,no_run
/// use axum_api_kit::ApiJson;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct CreateUser {
///     name: String,
/// }
///
/// // The body is deserialized before the handler runs; bad input becomes an ApiError body.
/// async fn create_user(ApiJson(user): ApiJson<CreateUser>) {
///     let _ = user.name;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ApiJson<T>(pub T);

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ApiError>);

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(json_rejection_to_api_error)?;
        Ok(ApiJson(value))
    }
}

impl<T: Serialize> IntoResponse for ApiJson<T> {
    fn into_response(self) -> Response {
        Json(self.0).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::header::CONTENT_TYPE, http::Request};
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Input {
        name: String,
    }

    async fn extract(
        body: &str,
        content_type: Option<&str>,
    ) -> Result<Input, (StatusCode, ApiError)> {
        let mut builder = Request::builder().method("POST").uri("/");
        if let Some(value) = content_type {
            builder = builder.header(CONTENT_TYPE, value);
        }
        let req = builder.body(Body::from(body.to_owned())).unwrap();
        ApiJson::<Input>::from_request(req, &())
            .await
            .map(|ApiJson(v)| v)
            .map_err(|(status, Json(err))| (status, err))
    }

    #[tokio::test]
    async fn valid_body_extracts() {
        let input = extract(r#"{"name":"abc"}"#, Some("application/json"))
            .await
            .unwrap();
        assert_eq!(input.name, "abc");
    }

    #[tokio::test]
    async fn malformed_json_is_invalid_json() {
        let (status, err) = extract("{not json", Some("application/json"))
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(err.code, "INVALID_JSON");
    }

    #[tokio::test]
    async fn wrong_shape_is_invalid_body() {
        let (status, err) = extract(r#"{"name":123}"#, Some("application/json"))
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.code, "INVALID_BODY");
    }

    #[tokio::test]
    async fn missing_content_type_is_unsupported_media_type() {
        let (status, err) = extract(r#"{"name":"abc"}"#, None).await.unwrap_err();
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(err.code, "UNSUPPORTED_MEDIA_TYPE");
    }

    #[tokio::test]
    async fn json_with_charset_is_accepted() {
        let input = extract(
            r#"{"name":"abc"}"#,
            Some("application/json; charset=utf-8"),
        )
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
    async fn serializes_as_response() {
        #[derive(Serialize)]
        struct Out {
            id: u32,
        }
        let res = ApiJson(Out { id: 7 }).into_response();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["id"], 7);
    }
}

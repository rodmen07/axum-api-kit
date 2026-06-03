use axum::{
    extract::{rejection::JsonRejection, FromRequest, Request},
    http::StatusCode,
    Json,
};
use serde::de::DeserializeOwned;
use validator::Validate;

use crate::ApiError;

/// An Axum extractor that deserializes a JSON request body and validates it with
/// [`validator`](https://docs.rs/validator).
///
/// On success it yields `ValidatedJson(value)`. On failure it short-circuits the handler
/// with a `(StatusCode, Json<ApiError>)` response:
///
/// - malformed JSON -> `400 Bad Request`, code `INVALID_JSON`
/// - well-formed JSON of the wrong shape -> `422 Unprocessable Entity`, code `INVALID_BODY`
/// - missing or incorrect `Content-Type` -> `415 Unsupported Media Type`, code
///   `UNSUPPORTED_MEDIA_TYPE`
/// - validation failure -> `422 Unprocessable Entity`, code `VALIDATION_ERROR` with
///   field-level `details` (see [`ApiError`]'s `From<validator::ValidationErrors>` impl)
///
/// Requires the `validator` feature.
///
/// # Example
///
/// ```rust,no_run
/// use axum_api_kit::ValidatedJson;
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
/// // The body is deserialized and validated before the handler body runs.
/// async fn create_user(ValidatedJson(user): ValidatedJson<CreateUser>) {
///     let _ = (user.name, user.email);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ValidatedJson<T>(pub T);

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ApiError>);

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(json_rejection_to_api_error)?;

        value.validate().map_err(|errors| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ApiError::from(errors)),
            )
        })?;

        Ok(ValidatedJson(value))
    }
}

/// Map an Axum [`JsonRejection`] onto an [`ApiError`] with a stable machine-readable code.
///
/// The HTTP status is taken from the rejection itself so it stays in sync with Axum.
fn json_rejection_to_api_error(rejection: JsonRejection) -> (StatusCode, Json<ApiError>) {
    let code = match &rejection {
        JsonRejection::JsonSyntaxError(_) => "INVALID_JSON",
        JsonRejection::JsonDataError(_) => "INVALID_BODY",
        JsonRejection::MissingJsonContentType(_) => "UNSUPPORTED_MEDIA_TYPE",
        _ => "BAD_REQUEST",
    };
    (
        rejection.status(),
        Json(ApiError::new(code, rejection.body_text())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::header::CONTENT_TYPE, http::Request};
    use serde::Deserialize;

    #[derive(Debug, Deserialize, Validate)]
    struct Input {
        #[validate(length(min = 2))]
        name: String,
    }

    async fn extract(body: &str, with_content_type: bool) -> Result<Input, (StatusCode, ApiError)> {
        let mut builder = Request::builder().method("POST").uri("/");
        if with_content_type {
            builder = builder.header(CONTENT_TYPE, "application/json");
        }
        let req = builder.body(Body::from(body.to_owned())).unwrap();
        ValidatedJson::<Input>::from_request(req, &())
            .await
            .map(|ValidatedJson(v)| v)
            .map_err(|(status, Json(err))| (status, err))
    }

    #[tokio::test]
    async fn valid_body_extracts() {
        let input = extract(r#"{"name":"abcd"}"#, true).await.unwrap();
        assert_eq!(input.name, "abcd");
    }

    #[tokio::test]
    async fn malformed_json_is_invalid_json() {
        let (status, err) = extract("{not json", true).await.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(err.code, "INVALID_JSON");
    }

    #[tokio::test]
    async fn wrong_shape_is_invalid_body() {
        let (status, err) = extract(r#"{"name":123}"#, true).await.unwrap_err();
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.code, "INVALID_BODY");
    }

    #[tokio::test]
    async fn missing_content_type_is_unsupported_media_type() {
        let (status, err) = extract(r#"{"name":"abcd"}"#, false).await.unwrap_err();
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(err.code, "UNSUPPORTED_MEDIA_TYPE");
    }

    #[tokio::test]
    async fn validation_failure_is_validation_error_with_fields() {
        let (status, err) = extract(r#"{"name":"a"}"#, true).await.unwrap_err();
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.code, "VALIDATION_ERROR");
        let v = serde_json::to_value(&err).unwrap();
        assert!(v["details"]["fields"]["name"].is_array());
    }
}

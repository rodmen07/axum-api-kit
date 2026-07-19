use axum::{
    http::{header::RETRY_AFTER, HeaderName, HeaderValue, StatusCode},
    Json,
};
use serde::Serialize;
use serde_json::Value;
use std::fmt;

/// A machine-readable JSON error body.
///
/// Serializes as:
/// ```json
/// { "code": "NOT_FOUND", "message": "item not found" }
/// { "code": "VALIDATION_ERROR", "message": "invalid input", "details": { "field": "name" } }
/// ```
///
/// Use the factory methods to get a `(StatusCode, Json<ApiError>)` tuple, which implements
/// [`IntoResponse`](axum::response::IntoResponse) and can be returned directly from Axum
/// handlers.
///
/// # Example
///
/// ```rust
/// use axum::response::IntoResponse;
/// use axum_api_kit::ApiError;
///
/// async fn handler() -> impl IntoResponse {
///     ApiError::not_found("item not found")
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiError {
    /// A short, stable, machine-readable error identifier. Use `SCREAMING_SNAKE_CASE`.
    pub code: String,
    /// A human-readable description of the error.
    pub message: String,
    /// Optional structured details (field-level validation errors, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<Object>))]
    pub details: Option<Value>,
}

impl ApiError {
    /// Construct a bare `ApiError` without a bundled status code.
    ///
    /// Prefer the factory methods ([`not_found`](Self::not_found), etc.) when returning
    /// responses directly from handlers.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    /// Attach structured details to this error.
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    // --- Factory helpers ---
    // Each returns (StatusCode, Json<ApiError>) which implements IntoResponse.

    /// `400 Bad Request` with the provided `code` and `message`.
    pub fn bad_request(
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> (StatusCode, Json<Self>) {
        (StatusCode::BAD_REQUEST, Json(Self::new(code, message)))
    }

    /// `401 Unauthorized` - `code` defaults to `"AUTH_REQUIRED"`.
    pub fn unauthorized(message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (
            StatusCode::UNAUTHORIZED,
            Json(Self::new("AUTH_REQUIRED", message)),
        )
    }

    /// `403 Forbidden` - `code` defaults to `"FORBIDDEN"`.
    pub fn forbidden(message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (StatusCode::FORBIDDEN, Json(Self::new("FORBIDDEN", message)))
    }

    /// `404 Not Found` - `code` defaults to `"NOT_FOUND"`.
    pub fn not_found(message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (StatusCode::NOT_FOUND, Json(Self::new("NOT_FOUND", message)))
    }

    /// `409 Conflict` - `code` defaults to `"CONFLICT"`.
    pub fn conflict(message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (StatusCode::CONFLICT, Json(Self::new("CONFLICT", message)))
    }

    /// `422 Unprocessable Entity` - `code` defaults to `"VALIDATION_ERROR"`.
    pub fn unprocessable_entity(message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(Self::new("VALIDATION_ERROR", message)),
        )
    }

    /// `500 Internal Server Error` - `code` defaults to `"INTERNAL_ERROR"`.
    pub fn internal(message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(Self::new("INTERNAL_ERROR", message)),
        )
    }

    /// `500 Internal Server Error` for database failures - `code` is `"DB_ERROR"`.
    pub fn db_error() -> (StatusCode, Json<Self>) {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(Self::new("DB_ERROR", "database error")),
        )
    }

    /// `429 Too Many Requests` - `code` defaults to `"RATE_LIMITED"`.
    pub fn too_many_requests(message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(Self::new("RATE_LIMITED", message)),
        )
    }

    /// `429 Too Many Requests` with a delay-seconds `Retry-After` header - `code` defaults
    /// to `"RATE_LIMITED"`.
    ///
    /// The tuple return type implements [`IntoResponse`](axum::response::IntoResponse), so
    /// handlers can return it directly. The body is the standard `{ "code", "message" }`
    /// shape; `retry_after` is rounded up to whole seconds (1500ms becomes `"2"`).
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum::response::IntoResponse;
    /// use axum_api_kit::ApiError;
    /// use std::time::Duration;
    ///
    /// async fn handler() -> impl IntoResponse {
    ///     ApiError::too_many_requests_with_retry_after("slow down", Duration::from_secs(30))
    /// }
    /// ```
    pub fn too_many_requests_with_retry_after(
        message: impl Into<String>,
        retry_after: std::time::Duration,
    ) -> (StatusCode, [(HeaderName, HeaderValue); 1], Json<Self>) {
        (
            StatusCode::TOO_MANY_REQUESTS,
            [(RETRY_AFTER, HeaderValue::from(ceil_secs(retry_after)))],
            Json(Self::new("RATE_LIMITED", message)),
        )
    }

    /// `503 Service Unavailable` - `code` defaults to `"SERVICE_UNAVAILABLE"`.
    pub fn service_unavailable(message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(Self::new("SERVICE_UNAVAILABLE", message)),
        )
    }

    /// `503 Service Unavailable` with a delay-seconds `Retry-After` header - `code` defaults
    /// to `"SERVICE_UNAVAILABLE"`.
    ///
    /// The tuple return type implements [`IntoResponse`](axum::response::IntoResponse), so
    /// handlers can return it directly. The body is the standard `{ "code", "message" }`
    /// shape; `retry_after` is rounded up to whole seconds (1500ms becomes `"2"`).
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum::response::IntoResponse;
    /// use axum_api_kit::ApiError;
    /// use std::time::Duration;
    ///
    /// async fn handler() -> impl IntoResponse {
    ///     ApiError::service_unavailable_with_retry_after(
    ///         "down for maintenance",
    ///         Duration::from_secs(120),
    ///     )
    /// }
    /// ```
    pub fn service_unavailable_with_retry_after(
        message: impl Into<String>,
        retry_after: std::time::Duration,
    ) -> (StatusCode, [(HeaderName, HeaderValue); 1], Json<Self>) {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            [(RETRY_AFTER, HeaderValue::from(ceil_secs(retry_after)))],
            Json(Self::new("SERVICE_UNAVAILABLE", message)),
        )
    }

    /// `501 Not Implemented` - `code` defaults to `"NOT_IMPLEMENTED"`.
    pub fn not_implemented(message: impl Into<String>) -> (StatusCode, Json<Self>) {
        (
            StatusCode::NOT_IMPLEMENTED,
            Json(Self::new("NOT_IMPLEMENTED", message)),
        )
    }

    /// Attach a source error message to this error.
    ///
    /// Stores the source in the details field under the `"source"` key.
    /// Can be chained with other builder methods.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum_api_kit::ApiError;
    ///
    /// let err = ApiError::new("NOT_FOUND", "user not found")
    ///     .with_source("SELECT * FROM users WHERE id = ?")
    ///     .with_details(serde_json::json!({ "user_id": 42 }));
    /// ```
    pub fn with_source(mut self, source: &str) -> Self {
        let mut details = self.details.take().unwrap_or_else(|| serde_json::json!({}));
        if let serde_json::Value::Object(ref mut map) = details {
            map.insert(
                "source".to_string(),
                serde_json::Value::String(source.to_string()),
            );
        }
        self.details = Some(details);
        self
    }

    /// Convert this error into an RFC 9457 [`Problem`](crate::Problem) response for the
    /// given status.
    ///
    /// Requires the `problem` feature flag.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum::http::StatusCode;
    /// use axum_api_kit::ApiError;
    /// use serde_json::json;
    ///
    /// let problem = ApiError::new("NOT_FOUND", "item 42 does not exist")
    ///     .with_details(json!({ "id": 42 }))
    ///     .into_problem(StatusCode::NOT_FOUND);
    ///
    /// assert_eq!(
    ///     serde_json::to_value(&problem).unwrap(),
    ///     json!({
    ///         "title": "Not Found",
    ///         "status": 404,
    ///         "detail": "item 42 does not exist",
    ///         "code": "NOT_FOUND",
    ///         "details": { "id": 42 }
    ///     })
    /// );
    /// ```
    #[cfg(feature = "problem")]
    pub fn into_problem(self, status: StatusCode) -> crate::Problem {
        crate::Problem::from((status, self))
    }
}

/// Round a [`Duration`](std::time::Duration) up to whole seconds for delay-seconds
/// `Retry-After` header values.
///
/// Shared by the `_with_retry_after` factory helpers and by
/// `Problem::into_response` (feature `problem`).
pub(crate) fn ceil_secs(d: std::time::Duration) -> u64 {
    d.as_secs() + u64::from(d.subsec_nanos() > 0)
}

/// Convert `std::io::Error` to `ApiError` with HTTP 500.
///
/// Maps `std::io::Error` to `ApiError::internal()` with the error message.
/// Enables using the `?` operator in handlers:
///
/// ```rust,ignore
/// async fn handler() -> impl IntoResponse {
///     let content = std::fs::read_to_string("/data.txt")?;  // auto-converts to ApiError
///     Ok((StatusCode::OK, content))
/// }
/// ```
impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        Self::new("IO_ERROR", format!("IO error: {}", err))
    }
}

/// Convert `serde_json::Error` to `ApiError` with HTTP 500.
///
/// Maps JSON errors to `ApiError::internal()` with the error message.
impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        Self::new("JSON_ERROR", format!("JSON error: {}", err))
    }
}

/// Convert `sqlx::Error` to an `ApiError` with a semantically appropriate HTTP status.
///
/// Requires the `sqlx` feature flag.
///
/// | `sqlx::Error` variant | `code` | HTTP |
/// |---|---|---|
/// | `RowNotFound` | `NOT_FOUND` | 404 |
/// | `Database` (unique/FK violation) | `CONFLICT` | 409 |
/// | `Database` (check violation) | `VALIDATION_ERROR` | 422 |
/// | `Database` (other) | `DB_ERROR` | 500 |
/// | `PoolTimedOut` / `PoolClosed` / `WorkerCrashed` | `SERVICE_UNAVAILABLE` | 503 |
/// | everything else | `DB_ERROR` | 500 |
#[cfg(feature = "sqlx")]
impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self::new("NOT_FOUND", "record not found"),
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::WorkerCrashed => {
                Self::new("SERVICE_UNAVAILABLE", "database unavailable")
            }
            sqlx::Error::Database(db_err) => {
                if db_err.is_unique_violation() || db_err.is_foreign_key_violation() {
                    Self::new("CONFLICT", db_err.message().to_string())
                } else if db_err.is_check_violation() {
                    Self::new("VALIDATION_ERROR", db_err.message().to_string())
                } else {
                    Self::new("DB_ERROR", db_err.message().to_string())
                }
            }
            _ => Self::new("DB_ERROR", format!("database error: {}", err)),
        }
    }
}

#[cfg(feature = "validator")]
fn collect_validation_errors(
    prefix: Option<&str>,
    errors: &validator::ValidationErrors,
    out: &mut serde_json::Map<String, serde_json::Value>,
) {
    use validator::ValidationErrorsKind;

    for (field, kind) in errors.errors() {
        let base = if let Some(prefix) = prefix {
            format!("{}.{}", prefix, field)
        } else {
            field.to_string()
        };

        match kind {
            ValidationErrorsKind::Field(field_errors) => {
                let items = field_errors
                    .iter()
                    .map(|err| {
                        let mut obj = serde_json::Map::new();
                        obj.insert(
                            "code".to_string(),
                            serde_json::Value::String(err.code.to_string()),
                        );
                        if let Some(message) = &err.message {
                            obj.insert(
                                "message".to_string(),
                                serde_json::Value::String(message.to_string()),
                            );
                        }
                        if !err.params.is_empty() {
                            let params = match serde_json::to_value(&err.params) {
                                Ok(v) => v,
                                Err(_) => serde_json::Value::Null,
                            };
                            obj.insert("params".to_string(), params);
                        }
                        serde_json::Value::Object(obj)
                    })
                    .collect::<Vec<_>>();
                out.insert(base, serde_json::Value::Array(items));
            }
            ValidationErrorsKind::Struct(nested) => {
                collect_validation_errors(Some(&base), nested, out);
            }
            ValidationErrorsKind::List(items) => {
                for (index, nested) in items {
                    let indexed = format!("{}[{}]", base, index);
                    collect_validation_errors(Some(&indexed), nested, out);
                }
            }
        }
    }
}

#[cfg(feature = "validator")]
impl From<validator::ValidationErrors> for ApiError {
    fn from(errors: validator::ValidationErrors) -> Self {
        let mut fields = serde_json::Map::new();
        collect_validation_errors(None, &errors, &mut fields);

        Self::new("VALIDATION_ERROR", "validation failed").with_details(serde_json::json!({
            "fields": fields
        }))
    }
}

/// Map an Axum [`JsonRejection`](axum::extract::rejection::JsonRejection) onto a
/// `(StatusCode, Json<ApiError>)` with a stable machine-readable code, preserving the
/// rejection's HTTP status so it stays in sync with Axum.
///
/// Shared by the `ValidatedJson` (feature `validator`) and `ApiJson` (feature `extract`)
/// extractors. Compiled only when at least one of those features is enabled.
#[cfg(any(feature = "validator", feature = "extract"))]
pub(crate) fn json_rejection_to_api_error(
    rejection: axum::extract::rejection::JsonRejection,
) -> (StatusCode, Json<ApiError>) {
    use axum::extract::rejection::JsonRejection;
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

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn new_sets_fields() {
        let err = ApiError::new("MY_CODE", "my message");
        assert_eq!(err.code, "MY_CODE");
        assert_eq!(err.message, "my message");
        assert!(err.details.is_none());
    }

    #[test]
    fn with_details_sets_details() {
        let err = ApiError::new("CODE", "msg").with_details(json!({ "field": "name" }));
        assert_eq!(err.details.unwrap()["field"], "name");
    }

    #[test]
    fn serializes_without_details() {
        let err = ApiError::new("NOT_FOUND", "item not found");
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["code"], "NOT_FOUND");
        assert_eq!(v["message"], "item not found");
        assert!(v.get("details").is_none());
    }

    #[test]
    fn serializes_with_details() {
        let err = ApiError::new("VALIDATION_ERROR", "invalid").with_details(json!({ "x": 1 }));
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["details"]["x"], 1);
    }

    #[test]
    fn display_formats_code_and_message() {
        let err = ApiError::new("NOT_FOUND", "item not found");
        assert_eq!(err.to_string(), "NOT_FOUND: item not found");
    }

    #[test]
    fn implements_std_error() {
        let err = ApiError::new("ERR", "something failed");
        let _: &dyn std::error::Error = &err;
    }

    macro_rules! assert_factory {
        ($method:expr, $expected_status:expr, $expected_code:expr) => {{
            let (status, Json(body)) = $method;
            assert_eq!(status, $expected_status);
            assert_eq!(body.code, $expected_code);
        }};
    }

    #[test]
    fn bad_request_status_and_code() {
        assert_factory!(
            ApiError::bad_request("INVALID_FIELD", "bad"),
            StatusCode::BAD_REQUEST,
            "INVALID_FIELD"
        );
    }

    #[test]
    fn unauthorized_status_and_code() {
        assert_factory!(
            ApiError::unauthorized("please log in"),
            StatusCode::UNAUTHORIZED,
            "AUTH_REQUIRED"
        );
    }

    #[test]
    fn forbidden_status_and_code() {
        assert_factory!(
            ApiError::forbidden("no access"),
            StatusCode::FORBIDDEN,
            "FORBIDDEN"
        );
    }

    #[test]
    fn not_found_status_and_code() {
        assert_factory!(
            ApiError::not_found("missing"),
            StatusCode::NOT_FOUND,
            "NOT_FOUND"
        );
    }

    #[test]
    fn conflict_status_and_code() {
        assert_factory!(
            ApiError::conflict("already exists"),
            StatusCode::CONFLICT,
            "CONFLICT"
        );
    }

    #[test]
    fn unprocessable_entity_status_and_code() {
        assert_factory!(
            ApiError::unprocessable_entity("invalid input"),
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALIDATION_ERROR"
        );
    }

    #[test]
    fn internal_status_and_code() {
        assert_factory!(
            ApiError::internal("oops"),
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR"
        );
    }

    #[test]
    fn db_error_status_and_code() {
        assert_factory!(
            ApiError::db_error(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "DB_ERROR"
        );
    }

    #[test]
    fn too_many_requests_status_and_code() {
        assert_factory!(
            ApiError::too_many_requests("slow down"),
            StatusCode::TOO_MANY_REQUESTS,
            "RATE_LIMITED"
        );
    }

    #[test]
    fn service_unavailable_status_and_code() {
        assert_factory!(
            ApiError::service_unavailable("down for maintenance"),
            StatusCode::SERVICE_UNAVAILABLE,
            "SERVICE_UNAVAILABLE"
        );
    }

    #[test]
    fn not_implemented_status_and_code() {
        assert_factory!(
            ApiError::not_implemented("coming soon"),
            StatusCode::NOT_IMPLEMENTED,
            "NOT_IMPLEMENTED"
        );
    }

    #[test]
    fn ceil_secs_rounds_up_to_whole_seconds() {
        use std::time::Duration;
        assert_eq!(ceil_secs(Duration::from_secs(0)), 0);
        assert_eq!(ceil_secs(Duration::from_secs(2)), 2);
        assert_eq!(ceil_secs(Duration::from_millis(1500)), 2);
    }

    #[test]
    fn too_many_requests_with_retry_after_status_header_and_body() {
        let (status, [(name, value)], Json(body)) = ApiError::too_many_requests_with_retry_after(
            "slow down",
            std::time::Duration::from_secs(30),
        );
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(name, RETRY_AFTER);
        assert_eq!(value, "30");
        assert_eq!(
            serde_json::to_value(&body).unwrap(),
            json!({ "code": "RATE_LIMITED", "message": "slow down" })
        );
    }

    #[test]
    fn service_unavailable_with_retry_after_status_header_and_body() {
        let (status, [(name, value)], Json(body)) = ApiError::service_unavailable_with_retry_after(
            "down for maintenance",
            std::time::Duration::from_secs(30),
        );
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(name, RETRY_AFTER);
        assert_eq!(value, "30");
        assert_eq!(
            serde_json::to_value(&body).unwrap(),
            json!({ "code": "SERVICE_UNAVAILABLE", "message": "down for maintenance" })
        );
    }

    #[cfg(feature = "problem")]
    #[test]
    fn into_problem_reproduces_reference_wire_shape() {
        let problem = ApiError::new("NOT_FOUND", "item 42 does not exist")
            .with_details(json!({ "id": 42 }))
            .into_problem(StatusCode::NOT_FOUND);
        assert_eq!(
            serde_json::to_value(&problem).unwrap(),
            json!({
                "title": "Not Found",
                "status": 404,
                "detail": "item 42 does not exist",
                "code": "NOT_FOUND",
                "details": { "id": 42 }
            })
        );
    }

    #[test]
    fn with_source_adds_source_to_details() {
        let err = ApiError::new("NOT_FOUND", "missing").with_source("db query");
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["details"]["source"], "db query");
        assert_eq!(v["code"], "NOT_FOUND");
    }

    #[test]
    fn with_source_and_with_details_both_present() {
        let err = ApiError::new("ERROR", "msg")
            .with_details(json!({ "user_id": 123 }))
            .with_source("from somewhere");
        let v = serde_json::to_value(&err).unwrap();
        assert_eq!(v["details"]["source"], "from somewhere");
        assert_eq!(v["details"]["user_id"], 123);
    }

    #[test]
    fn from_io_error_creates_io_error_code() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let api_err: ApiError = io_err.into();
        assert_eq!(api_err.code, "IO_ERROR");
        assert!(api_err.message.contains("IO error"));
    }

    #[test]
    fn from_serde_json_error_creates_json_error_code() {
        let json_str = "{ invalid json }";
        let json_err: Result<serde_json::Value, _> = serde_json::from_str(json_str);
        let api_err: ApiError = json_err.unwrap_err().into();
        assert_eq!(api_err.code, "JSON_ERROR");
        assert!(api_err.message.contains("JSON error"));
    }

    #[test]
    fn io_error_conversion_captures_kind() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied");
        let api_err: ApiError = io_err.into();
        assert!(api_err.message.contains("permission denied"));
    }

    #[cfg(feature = "validator")]
    #[test]
    fn from_validation_errors_single_field() {
        use std::borrow::Cow;
        use validator::{ValidationError, ValidationErrors};

        let mut errors = ValidationErrors::new();
        let mut email = ValidationError::new("email");
        email.message = Some(Cow::Borrowed("invalid email"));
        errors.add("email", email);

        let api_err: ApiError = errors.into();
        let v = serde_json::to_value(api_err).unwrap();

        assert_eq!(v["code"], "VALIDATION_ERROR");
        assert_eq!(v["message"], "validation failed");
        assert_eq!(v["details"]["fields"]["email"][0]["code"], "email");
        assert_eq!(
            v["details"]["fields"]["email"][0]["message"],
            "invalid email"
        );
    }

    #[cfg(feature = "validator")]
    #[test]
    fn from_validation_errors_multiple_fields_with_params() {
        use std::borrow::Cow;
        use validator::{ValidationError, ValidationErrors};

        let mut errors = ValidationErrors::new();

        let mut username = ValidationError::new("length");
        username.message = Some(Cow::Borrowed("username too short"));
        username.add_param(Cow::Borrowed("min"), &3);
        errors.add("username", username);

        let mut age = ValidationError::new("range");
        age.add_param(Cow::Borrowed("min"), &18);
        errors.add("age", age);

        let api_err: ApiError = errors.into();
        let v = serde_json::to_value(api_err).unwrap();

        assert_eq!(v["details"]["fields"]["username"][0]["code"], "length");
        assert_eq!(v["details"]["fields"]["username"][0]["params"]["min"], 3);
        assert_eq!(v["details"]["fields"]["age"][0]["code"], "range");
        assert_eq!(v["details"]["fields"]["age"][0]["params"]["min"], 18);
    }

    #[cfg(feature = "sqlx")]
    #[test]
    fn sqlx_row_not_found_maps_to_not_found() {
        let api_err: ApiError = sqlx::Error::RowNotFound.into();
        assert_eq!(api_err.code, "NOT_FOUND");
        assert_eq!(api_err.message, "record not found");
    }

    #[cfg(feature = "sqlx")]
    #[test]
    fn sqlx_pool_timed_out_maps_to_service_unavailable() {
        let api_err: ApiError = sqlx::Error::PoolTimedOut.into();
        assert_eq!(api_err.code, "SERVICE_UNAVAILABLE");
    }

    #[cfg(feature = "sqlx")]
    #[test]
    fn sqlx_pool_closed_maps_to_service_unavailable() {
        let api_err: ApiError = sqlx::Error::PoolClosed.into();
        assert_eq!(api_err.code, "SERVICE_UNAVAILABLE");
    }

    #[cfg(feature = "sqlx")]
    #[test]
    fn sqlx_unknown_variant_maps_to_db_error() {
        // Protocol is a non-pool, non-database variant that hits the catch-all arm.
        let api_err: ApiError = sqlx::Error::Protocol("unexpected packet".into()).into();
        assert_eq!(api_err.code, "DB_ERROR");
        assert!(api_err.message.contains("database error"));
    }
}

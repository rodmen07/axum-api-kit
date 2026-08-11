//! RFC 9457 `application/problem+json` error responses.
//!
//! [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457) (Problem Details for HTTP
//! APIs) defines a standard JSON error shape - `type`, `title`, `status`,
//! `detail`, `instance`, plus arbitrary extension members - served with the
//! `application/problem+json` media type. [`Problem`] implements that shape as
//! a chainable builder that converts into an Axum response.
//!
//! # `Problem` vs [`ApiError`](crate::ApiError)
//!
//! [`ApiError`](crate::ApiError)'s flat `{ code, message, details }` body is
//! the kit's default and remains the right choice for services that own both
//! ends of the wire. Reach for `Problem` when the error format needs to
//! interoperate: API gateways that understand problem+json, OpenAPI tooling
//! that expects the RFC 9457 members, or polyglot clients standardizing on the
//! RFC across services.
//!
//! A separate type exists because `ApiError`'s serialization and factory
//! tuples are frozen under the 1.x stability promise, and `axum::Json` can
//! only emit `Content-Type: application/json`; `Problem` builds its own
//! response so it can send `application/problem+json`. The `From` impls in
//! this module bridge an existing `ApiError` (or a factory tuple) into a
//! `Problem` losslessly.
//!
//! # Content negotiation (opt-in)
//!
//! [`Problem`]'s plain [`IntoResponse`] impl always emits
//! `Content-Type: application/problem+json`; that behavior is frozen and does
//! not change for existing users. Handlers can opt into Accept-header
//! negotiation, serving the same body bytes as plain `application/json` when
//! the client strictly prefers it: extract [`ProblemFormat`] and finish with
//! [`Problem::into_response_with`], or call [`Problem::into_response_for`]
//! with the request's [`HeaderMap`]. The exact (deliberately minimal) rules
//! live on [`ProblemFormat::negotiate`]; every ambiguous case, including no
//! `Accept` header and `*/*`, stays `application/problem+json`.
//!
//! # problem+json extractor rejections (opt-in)
//!
//! The [`ProblemJson`](crate::ProblemJson) (features `problem` + `extract`)
//! and [`ProblemValidatedJson`](crate::ProblemValidatedJson) (features
//! `problem` + `validator`) extractors are the problem-flavored siblings of
//! [`ApiJson`](crate::ApiJson) and [`ValidatedJson`](crate::ValidatedJson):
//! same deserialization, validation, and status codes, but failures reject
//! with an RFC 9457 body (`Content-Type` negotiated via [`ProblemFormat`]).
//! The existing extractors' rejection bodies are frozen and never change; the
//! format is chosen by naming the extractor in the handler signature.
//!
//! # Out of scope
//!
//! - HTTP-date `Retry-After` values (only delay-seconds are emitted).
//!
//! A candidate for a future minor release.

use axum::{
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use crate::ApiError;

mod negotiation;

pub use negotiation::ProblemFormat;

/// The `application/problem+json` media type from RFC 9457.
///
/// # Example
///
/// ```rust
/// use axum_api_kit::APPLICATION_PROBLEM_JSON;
///
/// assert_eq!(APPLICATION_PROBLEM_JSON, "application/problem+json");
/// ```
pub const APPLICATION_PROBLEM_JSON: &str = "application/problem+json";

/// An RFC 9457 problem details response body.
///
/// Serializes as:
/// ```json
/// { "title": "Not Found", "status": 404 }
/// { "type": "https://example.com/probs/out-of-credit", "title": "Insufficient credit",
///   "status": 403, "detail": "Balance is 30, item costs 50",
///   "instance": "/account/12345/msgs/abc", "balance": 30 }
/// ```
///
/// Implements [`IntoResponse`] with `Content-Type: application/problem+json`
/// and an optional delay-seconds `Retry-After` header.
///
/// # Example
///
/// ```rust
/// use axum::{http::StatusCode, response::IntoResponse};
/// use axum_api_kit::Problem;
///
/// async fn handler() -> impl IntoResponse {
///     Problem::new(StatusCode::FORBIDDEN, "Insufficient credit")
///         .with_type("https://example.com/probs/out-of-credit")
///         .with_detail("Balance is 30, item costs 50")
///         .with_instance("/account/12345/msgs/abc")
///         .with_extension("balance", 30)
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Problem {
    /// A URI reference identifying the problem type. Absent means "about:blank" per RFC 9457.
    ///
    /// Absent means absent: the key is omitted from the body, never sent as `null`, so the
    /// schema declares `nullable = false` (see [`CursorResponse::next_cursor`] for the reasoning).
    ///
    /// [`CursorResponse::next_cursor`]: crate::CursorResponse::next_cursor
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(nullable = false))]
    pub type_uri: Option<String>,
    /// A short, human-readable summary of the problem type. Stable per problem type.
    pub title: String,
    /// The HTTP status code, duplicated in the body per RFC 9457.
    pub status: u16,
    /// A human-readable explanation specific to this occurrence. Omitted when absent, never `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(nullable = false))]
    pub detail: Option<String>,
    /// A URI reference identifying this specific occurrence. Omitted when absent, never `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "openapi", schema(nullable = false))]
    pub instance: Option<String>,
    /// RFC 9457 extension members, flattened to top-level JSON keys.
    #[serde(flatten)]
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub extensions: serde_json::Map<String, serde_json::Value>,
    /// Optional Retry-After header delay. Header-only; never serialized in the body.
    #[serde(skip)]
    #[cfg_attr(feature = "openapi", schema(ignore))]
    pub retry_after: Option<std::time::Duration>,
}

impl Problem {
    /// Builds a minimal `Problem` with the given status and title.
    ///
    /// `type`, `detail`, and `instance` start absent, extensions start empty,
    /// and no `Retry-After` header is emitted.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum::http::StatusCode;
    /// use axum_api_kit::Problem;
    ///
    /// let problem = Problem::new(StatusCode::NOT_FOUND, "Not Found");
    /// assert_eq!(
    ///     serde_json::to_value(&problem).unwrap(),
    ///     serde_json::json!({ "title": "Not Found", "status": 404 })
    /// );
    /// ```
    pub fn new(status: StatusCode, title: impl Into<String>) -> Self {
        Self {
            type_uri: None,
            title: title.into(),
            status: status.as_u16(),
            detail: None,
            instance: None,
            extensions: serde_json::Map::new(),
            retry_after: None,
        }
    }

    /// Sets the `type` member: a URI reference identifying the problem type.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum::http::StatusCode;
    /// use axum_api_kit::Problem;
    ///
    /// let problem = Problem::new(StatusCode::FORBIDDEN, "Insufficient credit")
    ///     .with_type("https://example.com/probs/out-of-credit");
    /// let v = serde_json::to_value(&problem).unwrap();
    /// assert_eq!(v["type"], "https://example.com/probs/out-of-credit");
    /// ```
    pub fn with_type(mut self, type_uri: impl Into<String>) -> Self {
        self.type_uri = Some(type_uri.into());
        self
    }

    /// Sets the `detail` member: a human-readable explanation specific to this
    /// occurrence.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum::http::StatusCode;
    /// use axum_api_kit::Problem;
    ///
    /// let problem = Problem::new(StatusCode::FORBIDDEN, "Insufficient credit")
    ///     .with_detail("Balance is 30, item costs 50");
    /// let v = serde_json::to_value(&problem).unwrap();
    /// assert_eq!(v["detail"], "Balance is 30, item costs 50");
    /// ```
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Sets the `instance` member: a URI reference identifying this specific
    /// occurrence.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum::http::StatusCode;
    /// use axum_api_kit::Problem;
    ///
    /// let problem = Problem::new(StatusCode::FORBIDDEN, "Insufficient credit")
    ///     .with_instance("/account/12345/msgs/abc");
    /// let v = serde_json::to_value(&problem).unwrap();
    /// assert_eq!(v["instance"], "/account/12345/msgs/abc");
    /// ```
    pub fn with_instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    /// Adds an RFC 9457 extension member, serialized as a top-level JSON key.
    ///
    /// If `key` is one of the reserved members (`"type"`, `"title"`,
    /// `"status"`, `"detail"`, `"instance"`), the call is a silent no-op so
    /// the flattened extensions can never emit duplicate JSON keys — the
    /// reserved member already carries that value, so nothing is lost.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum::http::StatusCode;
    /// use axum_api_kit::Problem;
    ///
    /// let problem = Problem::new(StatusCode::FORBIDDEN, "Insufficient credit")
    ///     .with_extension("balance", 30)
    ///     .with_extension("status", 999); // reserved key: ignored
    /// let v = serde_json::to_value(&problem).unwrap();
    /// assert_eq!(v["balance"], 30);
    /// assert_eq!(v["status"], 403);
    /// ```
    pub fn with_extension(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        let key = key.into();
        if matches!(
            key.as_str(),
            "type" | "title" | "status" | "detail" | "instance"
        ) {
            return self;
        }
        self.extensions.insert(key, value.into());
        self
    }

    /// Emits a delay-seconds `Retry-After` header on the response, rounded up
    /// to whole seconds (1500ms becomes `"2"`).
    ///
    /// The delay never appears in the JSON body; add it via
    /// [`with_extension`](Self::with_extension) if body presence is wanted.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum::{http::StatusCode, response::IntoResponse};
    /// use axum_api_kit::Problem;
    /// use std::time::Duration;
    ///
    /// let res = Problem::new(StatusCode::TOO_MANY_REQUESTS, "Too Many Requests")
    ///     .with_retry_after(Duration::from_secs(30))
    ///     .into_response();
    /// assert_eq!(res.headers().get("retry-after").unwrap(), "30");
    /// ```
    pub fn with_retry_after(mut self, delay: std::time::Duration) -> Self {
        self.retry_after = Some(delay);
        self
    }

    /// Returns the `status` field as a [`StatusCode`], falling back to
    /// `500 Internal Server Error` when it is not a valid status.
    ///
    /// `StatusCode::from_u16` accepts `100..=999`, so the fallback only
    /// triggers for a hand-set `pub status` outside that range.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum::http::StatusCode;
    /// use axum_api_kit::Problem;
    ///
    /// let problem = Problem::new(StatusCode::NOT_FOUND, "Not Found");
    /// assert_eq!(problem.status_code(), StatusCode::NOT_FOUND);
    /// ```
    pub fn status_code(&self) -> StatusCode {
        StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }

    /// Converts into a response, choosing the `Content-Type` by negotiating
    /// against the request's `Accept` headers.
    ///
    /// Shorthand for `self.into_response_with(ProblemFormat::negotiate(headers))`;
    /// see [`ProblemFormat::negotiate`] for the exact rules. Plain
    /// `application/json` is served only when the client strictly prefers it;
    /// every ambiguous case (no `Accept` header, `*/*`, ties, unparseable
    /// values) serves `application/problem+json`. The body bytes and any
    /// `Retry-After` header are identical either way, and the plain
    /// [`IntoResponse`] behavior is untouched.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
    /// use axum_api_kit::Problem;
    ///
    /// let mut headers = HeaderMap::new();
    /// headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    ///
    /// let res = Problem::new(StatusCode::NOT_FOUND, "Not Found").into_response_for(&headers);
    /// assert_eq!(res.headers().get(header::CONTENT_TYPE).unwrap(), "application/json");
    ///
    /// let res = Problem::new(StatusCode::NOT_FOUND, "Not Found").into_response_for(&HeaderMap::new());
    /// assert_eq!(
    ///     res.headers().get(header::CONTENT_TYPE).unwrap(),
    ///     "application/problem+json"
    /// );
    /// ```
    pub fn into_response_for(self, headers: &HeaderMap) -> Response {
        self.into_response_with(ProblemFormat::negotiate(headers))
    }

    /// Converts into a response served as the given [`ProblemFormat`].
    ///
    /// Pairs with the [`ProblemFormat`] extractor, which negotiates the format
    /// from the request's `Accept` headers before the handler runs. The JSON
    /// body bytes and any `Retry-After` header are identical for both formats;
    /// only the `Content-Type` header differs.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum::http::{header, StatusCode};
    /// use axum_api_kit::{Problem, ProblemFormat};
    ///
    /// let res = Problem::new(StatusCode::NOT_FOUND, "Not Found")
    ///     .into_response_with(ProblemFormat::Json);
    /// assert_eq!(res.headers().get(header::CONTENT_TYPE).unwrap(), "application/json");
    /// ```
    pub fn into_response_with(self, format: ProblemFormat) -> Response {
        // axum::Json hardcodes Content-Type: application/json, so the response
        // is built via the (StatusCode, [(HeaderName, HeaderValue); 1], Vec<u8>)
        // tuple instead; the header part's insert overrides the
        // application/octet-stream default that Vec<u8> alone would set. The
        // same tuple pattern is used by Created::with_location in success.rs.
        // On the practically unreachable serialization failure, both the status
        // line and the body say 500 so they stay consistent.
        let retry = self.retry_after;
        let (status, body) = match serde_json::to_vec(&self) {
            Ok(bytes) => (self.status_code(), bytes),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                br#"{"title":"Internal Server Error","status":500}"#.to_vec(),
            ),
        };
        let mut res = (
            status,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(format.content_type()),
            )],
            body,
        )
            .into_response();
        if let Some(d) = retry {
            res.headers_mut().insert(
                header::RETRY_AFTER,
                HeaderValue::from(crate::error::ceil_secs(d)),
            );
        }
        res
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        // The frozen 1.x default: always application/problem+json. Content
        // negotiation is opt-in via into_response_for / into_response_with.
        self.into_response_with(ProblemFormat::ProblemJson)
    }
}

/// Convert a `(StatusCode, ApiError)` pair into a [`Problem`], losslessly.
///
/// | source | `Problem` member |
/// |---|---|
/// | status | `status` |
/// | status canonical reason | `title` (falls back to `code` for nonstandard statuses; cosmetic) |
/// | `message` | `detail` |
/// | `code` | `"code"` extension member |
/// | `details` (when present) | `"details"` extension member, verbatim |
///
/// `details` is kept under the single `"details"` key, never flattened, so
/// validation field maps cannot collide with reserved members. `type` and
/// `instance` are left absent, and no `Retry-After` header is set.
impl From<(StatusCode, ApiError)> for Problem {
    fn from((status, err): (StatusCode, ApiError)) -> Self {
        let title = status
            .canonical_reason()
            .map(str::to_owned)
            .unwrap_or_else(|| err.code.clone());
        let mut extensions = serde_json::Map::new();
        extensions.insert("code".to_owned(), serde_json::Value::String(err.code));
        if let Some(details) = err.details {
            extensions.insert("details".to_owned(), details);
        }
        Self {
            type_uri: None,
            title,
            status: status.as_u16(),
            detail: Some(err.message),
            instance: None,
            extensions,
            retry_after: None,
        }
    }
}

/// Convert a `(StatusCode, Json<ApiError>)` factory tuple into a [`Problem`].
///
/// Unwraps the [`Json`] and delegates to [`From<(StatusCode, ApiError)>`], so
/// existing factory results convert directly:
/// `Problem::from(ApiError::not_found("nope"))`.
impl From<(StatusCode, Json<ApiError>)> for Problem {
    fn from((status, Json(err)): (StatusCode, Json<ApiError>)) -> Self {
        Problem::from((status, err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn minimal_serialization_omits_optional_members() {
        let problem = Problem::new(StatusCode::NOT_FOUND, "Not Found");
        let v = serde_json::to_value(&problem).unwrap();
        assert_eq!(v, json!({ "title": "Not Found", "status": 404 }));
        assert!(v.get("type").is_none());
        assert!(v.get("detail").is_none());
        assert!(v.get("instance").is_none());
    }

    #[test]
    fn full_shape_serializes_all_rfc_members() {
        let problem = Problem::new(StatusCode::FORBIDDEN, "Insufficient credit")
            .with_type("https://example.com/probs/out-of-credit")
            .with_detail("Balance is 30, item costs 50")
            .with_instance("/account/12345/msgs/abc");
        let v = serde_json::to_value(&problem).unwrap();
        assert_eq!(v["type"], "https://example.com/probs/out-of-credit");
        assert_eq!(v["title"], "Insufficient credit");
        assert_eq!(v["status"], 403);
        assert_eq!(v["detail"], "Balance is 30, item costs 50");
        assert_eq!(v["instance"], "/account/12345/msgs/abc");
        assert!(v.get("type_uri").is_none());
    }

    #[test]
    fn extensions_flatten_to_top_level() {
        let problem = Problem::new(StatusCode::FORBIDDEN, "Insufficient credit")
            .with_extension("balance", 30);
        let v = serde_json::to_value(&problem).unwrap();
        assert_eq!(v["balance"], 30);
        assert!(v.get("extensions").is_none());
    }

    #[test]
    fn with_extension_ignores_reserved_keys() {
        let problem = Problem::new(StatusCode::NOT_FOUND, "Not Found")
            .with_extension("status", 999)
            .with_extension("type", "https://example.com/overridden")
            .with_extension("title", "Overridden")
            .with_extension("detail", "overridden")
            .with_extension("instance", "/overridden");
        let v = serde_json::to_value(&problem).unwrap();
        assert_eq!(v, json!({ "title": "Not Found", "status": 404 }));
    }

    #[test]
    fn retry_after_never_serialized() {
        let problem = Problem::new(StatusCode::TOO_MANY_REQUESTS, "Too Many Requests")
            .with_retry_after(Duration::from_secs(30));
        let v = serde_json::to_value(&problem).unwrap();
        assert!(v.get("retry_after").is_none());
    }

    #[tokio::test]
    async fn into_response_status_content_type_body() {
        let res = Problem::new(StatusCode::FORBIDDEN, "Insufficient credit")
            .with_type("https://example.com/probs/out-of-credit")
            .with_detail("Balance is 30, item costs 50")
            .with_instance("/account/12345/msgs/abc")
            .with_extension("balance", 30)
            .into_response();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            res.headers().get(header::CONTENT_TYPE).unwrap(),
            APPLICATION_PROBLEM_JSON
        );
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            body,
            json!({
                "type": "https://example.com/probs/out-of-credit",
                "title": "Insufficient credit",
                "status": 403,
                "detail": "Balance is 30, item costs 50",
                "instance": "/account/12345/msgs/abc",
                "balance": 30
            })
        );
    }

    #[tokio::test]
    async fn retry_after_header_seconds() {
        let res = Problem::new(StatusCode::TOO_MANY_REQUESTS, "Too Many Requests")
            .with_retry_after(Duration::from_secs(30))
            .into_response();
        assert_eq!(res.headers().get(header::RETRY_AFTER).unwrap(), "30");

        let res = Problem::new(StatusCode::TOO_MANY_REQUESTS, "Too Many Requests")
            .with_retry_after(Duration::from_millis(1500))
            .into_response();
        assert_eq!(res.headers().get(header::RETRY_AFTER).unwrap(), "2");
    }

    #[test]
    fn from_status_apierror_maps_fields() {
        let err =
            ApiError::new("NOT_FOUND", "item 42 does not exist").with_details(json!({ "id": 42 }));
        let problem = Problem::from((StatusCode::NOT_FOUND, err));
        assert_eq!(problem.title, "Not Found");
        assert_eq!(problem.status, 404);
        assert_eq!(problem.detail.as_deref(), Some("item 42 does not exist"));
        assert_eq!(problem.extensions["code"], "NOT_FOUND");
        assert_eq!(problem.extensions["details"], json!({ "id": 42 }));

        let no_details = Problem::from((StatusCode::NOT_FOUND, ApiError::new("NOT_FOUND", "nope")));
        assert!(!no_details.extensions.contains_key("details"));
    }

    #[test]
    fn from_status_json_apierror_unwraps() {
        let via_factory = Problem::from(ApiError::not_found("nope"));
        let direct = Problem::from((StatusCode::NOT_FOUND, ApiError::new("NOT_FOUND", "nope")));
        assert_eq!(
            serde_json::to_value(&via_factory).unwrap(),
            serde_json::to_value(&direct).unwrap()
        );
    }

    #[test]
    fn status_code_falls_back_to_500() {
        // StatusCode::from_u16 accepts 100..=999, so use a value below 100.
        let problem = Problem {
            type_uri: None,
            title: "weird".to_owned(),
            status: 42,
            detail: None,
            instance: None,
            extensions: serde_json::Map::new(),
            retry_after: None,
        };
        assert_eq!(problem.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    fn accept(value: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, HeaderValue::from_static(value));
        headers
    }

    #[tokio::test]
    async fn into_response_with_json_changes_only_content_type() {
        let problem = || {
            Problem::new(StatusCode::FORBIDDEN, "Insufficient credit")
                .with_detail("Balance is 30, item costs 50")
                .with_extension("balance", 30)
                .with_retry_after(Duration::from_secs(30))
        };
        let default = problem().into_response();
        let negotiated = problem().into_response_with(ProblemFormat::Json);

        assert_eq!(negotiated.status(), default.status());
        assert_eq!(
            negotiated.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        assert_eq!(negotiated.headers().get(header::RETRY_AFTER).unwrap(), "30");

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

    #[tokio::test]
    async fn into_response_for_negotiates_from_headers() {
        let res = Problem::new(StatusCode::NOT_FOUND, "Not Found")
            .into_response_for(&accept("application/json"));
        assert_eq!(
            res.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );

        let res =
            Problem::new(StatusCode::NOT_FOUND, "Not Found").into_response_for(&HeaderMap::new());
        assert_eq!(
            res.headers().get(header::CONTENT_TYPE).unwrap(),
            APPLICATION_PROBLEM_JSON
        );
    }

    #[tokio::test]
    async fn plain_into_response_is_unchanged_by_negotiation_support() {
        // Locks the frozen 1.x behavior byte-for-byte: exactly the status,
        // header set, and body bytes Problem::into_response emitted in 1.3.0,
        // regardless of the negotiation machinery added alongside it.
        let res = Problem::new(StatusCode::NOT_FOUND, "Not Found")
            .with_retry_after(Duration::from_secs(30))
            .into_response();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(res.headers().len(), 2);
        assert_eq!(
            res.headers().get(header::CONTENT_TYPE).unwrap(),
            APPLICATION_PROBLEM_JSON
        );
        assert_eq!(res.headers().get(header::RETRY_AFTER).unwrap(), "30");
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&bytes[..], br#"{"title":"Not Found","status":404}"#);

        // And the minimal case without Retry-After.
        let res = Problem::new(StatusCode::NOT_FOUND, "Not Found").into_response();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(res.headers().len(), 1);
        assert_eq!(
            res.headers().get(header::CONTENT_TYPE).unwrap(),
            APPLICATION_PROBLEM_JSON
        );
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&bytes[..], br#"{"title":"Not Found","status":404}"#);
    }
}

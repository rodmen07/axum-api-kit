//! Success-side response helpers for the parts of the HTTP lifecycle that
//! [`ListResponse`](crate::ListResponse) and [`ApiError`](crate::ApiError) don't
//! cover: resource creation, asynchronous acceptance, and empty responses.
//!
//! - [`Created<T>`] - `201 Created` carrying the new resource as a JSON body,
//!   with an optional `Location` header.
//! - [`Accepted<T>`] - `202 Accepted` carrying a JSON body (e.g. a job handle)
//!   for work that will complete asynchronously.
//! - [`NoContent`] - `204 No Content` with an empty body, for deletes and
//!   updates that return nothing.

use axum::{
    http::{header::LOCATION, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// A `201 Created` response carrying the newly created resource as a JSON body.
///
/// The body is the resource itself (serialized directly, not wrapped), matching
/// the common REST convention for `POST` create endpoints. Optionally sets a
/// `Location` header pointing at the new resource via [`Created::with_location`].
///
/// # Example
///
/// ```rust
/// use axum::response::IntoResponse;
/// use axum_api_kit::Created;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct User { id: String, name: String }
///
/// async fn create_user() -> impl IntoResponse {
///     let user = User { id: "42".into(), name: "Ada".into() };
///     Created::new(user).with_location("/users/42")
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Created<T: Serialize> {
    /// The created resource, serialized as the JSON response body.
    pub data: T,
    location: Option<String>,
}

impl<T: Serialize> Created<T> {
    /// Builds a `201 Created` response for `data` with no `Location` header.
    pub fn new(data: T) -> Self {
        Self {
            data,
            location: None,
        }
    }

    /// Sets the `Location` header to point at the new resource.
    ///
    /// If the value cannot be represented as a valid header (e.g. it contains
    /// control characters), the header is omitted rather than panicking.
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }
}

impl<T: Serialize> IntoResponse for Created<T> {
    fn into_response(self) -> Response {
        let header = self
            .location
            .as_deref()
            .and_then(|l| HeaderValue::from_str(l).ok());
        match header {
            Some(loc) => (StatusCode::CREATED, [(LOCATION, loc)], Json(self.data)).into_response(),
            None => (StatusCode::CREATED, Json(self.data)).into_response(),
        }
    }
}

/// A `202 Accepted` response carrying a JSON body.
///
/// Use for requests that have been accepted for processing but will complete
/// asynchronously - the body typically describes how to track the work (e.g. a
/// job id or a status URL).
///
/// # Example
///
/// ```rust
/// use axum::response::IntoResponse;
/// use axum_api_kit::Accepted;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Job { id: String }
///
/// async fn enqueue() -> impl IntoResponse {
///     Accepted::new(Job { id: "job-1".into() })
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Accepted<T: Serialize> {
    /// The body describing the accepted work, serialized as JSON.
    pub data: T,
}

impl<T: Serialize> Accepted<T> {
    /// Builds a `202 Accepted` response for `data`.
    pub fn new(data: T) -> Self {
        Self { data }
    }
}

impl<T: Serialize> IntoResponse for Accepted<T> {
    fn into_response(self) -> Response {
        (StatusCode::ACCEPTED, Json(self.data)).into_response()
    }
}

/// A `204 No Content` response with an empty body.
///
/// Use for `DELETE` and for `PUT`/`PATCH` handlers that return nothing.
///
/// # Example
///
/// ```rust
/// use axum::response::IntoResponse;
/// use axum_api_kit::NoContent;
///
/// async fn delete_user() -> impl IntoResponse {
///     NoContent
/// }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct NoContent;

impl NoContent {
    /// Builds a `204 No Content` response.
    pub fn new() -> Self {
        Self
    }
}

impl IntoResponse for NoContent {
    fn into_response(self) -> Response {
        StatusCode::NO_CONTENT.into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Item {
        id: String,
    }

    async fn body_json(res: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    }

    #[tokio::test]
    async fn created_has_201_and_serializes_resource() {
        let res = Created::new(Item { id: "1".into() }).into_response();
        assert_eq!(res.status(), StatusCode::CREATED);
        assert!(res.headers().get(LOCATION).is_none());
        let body = body_json(res).await;
        assert_eq!(body["id"], "1");
    }

    #[tokio::test]
    async fn created_with_location_sets_header() {
        let res = Created::new(Item { id: "1".into() })
            .with_location("/items/1")
            .into_response();
        assert_eq!(res.status(), StatusCode::CREATED);
        assert_eq!(res.headers().get(LOCATION).unwrap(), "/items/1");
        let body = body_json(res).await;
        assert_eq!(body["id"], "1");
    }

    #[tokio::test]
    async fn created_drops_invalid_location() {
        // A newline cannot be a header value; the header should be omitted.
        let res = Created::new(Item { id: "1".into() })
            .with_location("/items/\n1")
            .into_response();
        assert_eq!(res.status(), StatusCode::CREATED);
        assert!(res.headers().get(LOCATION).is_none());
    }

    #[tokio::test]
    async fn accepted_has_202_and_serializes_body() {
        let res = Accepted::new(Item { id: "job-1".into() }).into_response();
        assert_eq!(res.status(), StatusCode::ACCEPTED);
        let body = body_json(res).await;
        assert_eq!(body["id"], "job-1");
    }

    #[tokio::test]
    async fn no_content_has_204_and_empty_body() {
        let res = NoContent.into_response();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(bytes.is_empty());
    }

    #[test]
    fn no_content_new_matches_unit() {
        // Both construction styles compile and are equivalent.
        let _ = NoContent::new();
        let _ = NoContent;
    }
}

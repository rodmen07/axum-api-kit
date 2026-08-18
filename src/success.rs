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

/// Renders `data` as JSON and stamps `status` over the `200 OK` that [`Json`]
/// produces on success — and only over that.
///
/// On a serialization failure, `Json`'s own answer (`500 Internal Server
/// Error` with a `text/plain` serde error body) is returned untouched, so a
/// failing body is never reported under a success status. This
/// overwrite-only-on-success rule is what distinguishes the helper from the
/// plain `(StatusCode, Json(data))` tuple, whose `IntoResponse` stamps the
/// status unconditionally after `Json` has already rendered.
fn json_with_status<T: Serialize>(status: StatusCode, data: T) -> Response {
    let mut res = Json(data).into_response();
    if res.status() == StatusCode::OK {
        *res.status_mut() = status;
    }
    res
}

/// A `201 Created` response carrying the newly created resource as a JSON body.
///
/// The body is the resource itself (serialized directly, not wrapped), matching
/// the common REST convention for `POST` create endpoints. Optionally sets a
/// `Location` header pointing at the new resource via [`Created::with_location`].
///
/// # Serialization failure
///
/// If `T`'s `Serialize` impl returns an error, the response is exactly what
/// axum's own [`Json`] answers for that case — **`500 Internal Server Error`**
/// with a `text/plain` serde error message as the body — passed through
/// untouched, and the `Location` header is **not** attached: a client must
/// never be invited to follow a `Location` to a resource whose creation
/// response was lost. The `201` is only ever stamped over the `200 OK` that
/// `Json` produces on success. (2.2.0 and earlier applied the status after
/// `Json` had already rendered, so a failing body shipped under `201` with the
/// `Location` still set; the corrected behaviour is locked by
/// `tests/default_response_contracts.rs`.)
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
        let mut res = json_with_status(StatusCode::CREATED, self.data);
        // The Location header is attached only to a real 201: on a
        // serialization failure there is no resource to point at, and a
        // client that follows Location must never be sent to one that was
        // not created.
        if res.status() == StatusCode::CREATED {
            let header = self
                .location
                .as_deref()
                .and_then(|l| HeaderValue::from_str(l).ok());
            if let Some(loc) = header {
                res.headers_mut().insert(LOCATION, loc);
            }
        }
        res
    }
}

/// A `202 Accepted` response carrying a JSON body.
///
/// Use for requests that have been accepted for processing but will complete
/// asynchronously - the body typically describes how to track the work (e.g. a
/// job id or a status URL).
///
/// # Serialization failure
///
/// Same shape as [`Created`]: a failing `Serialize` impl gets [`Json`]'s own
/// answer — **`500 Internal Server Error`** with a `text/plain` serde error
/// body — passed through untouched; the `202` is only ever stamped over a
/// successful `200 OK`. (2.2.0 and earlier reported `202` with the serde
/// error as the body; the corrected behaviour is locked by
/// `tests/default_response_contracts.rs`.)
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
        json_with_status(StatusCode::ACCEPTED, self.data)
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

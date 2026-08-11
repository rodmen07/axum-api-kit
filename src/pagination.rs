use axum::{
    extract::{rejection::QueryRejection, FromRequestParts, Query},
    http::{request::Parts, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{ApiError, CursorResponse, ListResponse};

/// Offset/limit pagination parameters parsed from the query string.
///
/// Reads the `limit` and `offset` query parameters. A missing `limit` falls back to
/// [`Pagination::DEFAULT_LIMIT`] and a missing `offset` to `0`. `limit` is clamped to
/// `1..=`[`Pagination::MAX_LIMIT`]. A non-numeric value rejects the request with a
/// `400 Bad Request` carrying code `INVALID_QUERY`.
///
/// Requires the `extract` feature.
///
/// With the `openapi` feature also enabled, this type derives [`utoipa::IntoParams`], so a
/// handler documented with `#[utoipa::path(..., params(Pagination))]` contributes `limit` and
/// `offset` as query parameters carrying the same defaults and bounds enforced below. The
/// derive is gated on `openapi` **and** `extract` jointly rather than on a flag of its own.
///
/// # Example
///
/// ```rust,no_run
/// use axum_api_kit::{ListResponse, Pagination};
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Item {
///     id: u64,
/// }
///
/// async fn list(page: Pagination) -> ListResponse<Item> {
///     // Query your store using page.limit / page.offset...
///     let items = vec![Item { id: 1 }];
///     page.list_response(items, 1)
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
// `parameter_in` has NO default in the derive: `IntoParams::into_params` falls back to
// `ParameterIn::default()`, which is `Path`. Omitting this line would document both fields as
// path parameters. `tests/openapi_params.rs` asserts `in: query` for exactly that reason.
#[cfg_attr(feature = "openapi", into_params(parameter_in = Query))]
pub struct Pagination {
    /// Maximum number of items to return (clamped to `1..=`[`Pagination::MAX_LIMIT`]).
    //
    // The bounds below are numeric LITERALS because they must be: utoipa's `minimum`/`maximum`
    // parse a literal token and reject a constant path. Duplicating the values is therefore
    // unavoidable, and duplicating them deliberately is what makes the drift detectable —
    // `tests/openapi_params.rs` reads the generated document on one side and
    // `Pagination::DEFAULT_LIMIT` / `MAX_LIMIT` on the other, so a change to either constant
    // that is not mirrored here fails the suite. Naming the constants in the attribute instead
    // would make that guard compare a value against itself.
    #[cfg_attr(
        feature = "openapi",
        param(required = false, default = 50, minimum = 1, maximum = 100)
    )]
    pub limit: u32,
    /// Zero-based offset of the first item in the page.
    #[cfg_attr(feature = "openapi", param(required = false, default = 0))]
    pub offset: u32,
}

impl Pagination {
    /// Page size used when the `limit` query parameter is absent.
    pub const DEFAULT_LIMIT: u32 = 50;
    /// Largest page size accepted; larger requests are clamped down to this value.
    pub const MAX_LIMIT: u32 = 100;

    /// Build a [`ListResponse`] for this page from its items and the total match count.
    pub fn list_response<T: Serialize>(&self, data: Vec<T>, total: i64) -> ListResponse<T> {
        ListResponse {
            data,
            total,
            limit: self.limit,
            offset: self.offset,
        }
    }
}

#[derive(Deserialize)]
struct PaginationParams {
    limit: Option<u32>,
    offset: Option<u32>,
}

impl<S> FromRequestParts<S> for Pagination
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ApiError>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(params) = Query::<PaginationParams>::from_request_parts(parts, state)
            .await
            .map_err(query_rejection_to_api_error)?;

        Ok(Pagination {
            limit: clamp_limit(params.limit),
            offset: params.offset.unwrap_or(0),
        })
    }
}

/// Cursor-based pagination parameters parsed from the query string.
///
/// Reads an opaque `cursor` token (absent on the first page) and a `limit` that is clamped
/// the same way as [`Pagination`]. Requires the `extract` feature.
///
/// With the `openapi` feature also enabled, this type derives [`utoipa::IntoParams`] on the same
/// joint gate as [`Pagination`], contributing `cursor` and `limit` as query parameters.
///
/// # Example
///
/// ```rust,no_run
/// use axum_api_kit::{CursorPagination, CursorResponse};
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Item {
///     id: u64,
/// }
///
/// async fn feed(page: CursorPagination) -> CursorResponse<Item> {
///     // Decode page.cursor, fetch page.limit + 1 rows, derive the next token...
///     let items = vec![Item { id: 1 }];
///     page.cursor_response(items, Some("next".into()))
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::IntoParams))]
#[cfg_attr(feature = "openapi", into_params(parameter_in = Query))]
pub struct CursorPagination {
    /// Opaque cursor token for the requested page. `None` on the first page.
    //
    // `nullable = false` for the reason the response types carry it (see `CursorResponse`):
    // utoipa derives nullability from `Option<String>`, but the first page OMITS the parameter
    // rather than sending `cursor=null`. Optionality is carried by `required: false`.
    #[cfg_attr(feature = "openapi", param(required = false, nullable = false))]
    pub cursor: Option<String>,
    /// Maximum number of items to return (clamped to `1..=`[`Pagination::MAX_LIMIT`]).
    // Literals, and the same drift guard, as `Pagination::limit` above.
    #[cfg_attr(
        feature = "openapi",
        param(required = false, default = 50, minimum = 1, maximum = 100)
    )]
    pub limit: u32,
}

impl CursorPagination {
    /// Build a [`CursorResponse`] from this page's items and the next-page cursor.
    ///
    /// `has_more` is set to `next_cursor.is_some()`.
    pub fn cursor_response<T: Serialize>(
        &self,
        data: Vec<T>,
        next_cursor: Option<String>,
    ) -> CursorResponse<T> {
        CursorResponse {
            has_more: next_cursor.is_some(),
            next_cursor,
            data,
        }
    }
}

#[derive(Deserialize)]
struct CursorParams {
    cursor: Option<String>,
    limit: Option<u32>,
}

impl<S> FromRequestParts<S> for CursorPagination
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ApiError>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(params) = Query::<CursorParams>::from_request_parts(parts, state)
            .await
            .map_err(query_rejection_to_api_error)?;

        Ok(CursorPagination {
            cursor: params.cursor,
            limit: clamp_limit(params.limit),
        })
    }
}

/// Apply the default and the `1..=MAX_LIMIT` clamp shared by both extractors.
fn clamp_limit(limit: Option<u32>) -> u32 {
    limit
        .unwrap_or(Pagination::DEFAULT_LIMIT)
        .clamp(1, Pagination::MAX_LIMIT)
}

/// Map an Axum [`QueryRejection`] onto an [`ApiError`], preserving its HTTP status.
fn query_rejection_to_api_error(rejection: QueryRejection) -> (StatusCode, Json<ApiError>) {
    (
        rejection.status(),
        Json(ApiError::new("INVALID_QUERY", rejection.body_text())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};

    async fn pagination(uri: &str) -> Result<Pagination, (StatusCode, ApiError)> {
        let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        let (mut parts, _) = req.into_parts();
        Pagination::from_request_parts(&mut parts, &())
            .await
            .map_err(|(status, Json(err))| (status, err))
    }

    async fn cursor(uri: &str) -> Result<CursorPagination, (StatusCode, ApiError)> {
        let req = Request::builder().uri(uri).body(Body::empty()).unwrap();
        let (mut parts, _) = req.into_parts();
        CursorPagination::from_request_parts(&mut parts, &())
            .await
            .map_err(|(status, Json(err))| (status, err))
    }

    #[tokio::test]
    async fn pagination_defaults_when_absent() {
        let p = pagination("/items").await.unwrap();
        assert_eq!(p.limit, Pagination::DEFAULT_LIMIT);
        assert_eq!(p.offset, 0);
    }

    #[tokio::test]
    async fn pagination_parses_limit_and_offset() {
        let p = pagination("/items?limit=10&offset=20").await.unwrap();
        assert_eq!(p.limit, 10);
        assert_eq!(p.offset, 20);
    }

    #[tokio::test]
    async fn pagination_clamps_limit_to_max() {
        let p = pagination("/items?limit=100000").await.unwrap();
        assert_eq!(p.limit, Pagination::MAX_LIMIT);
    }

    #[tokio::test]
    async fn pagination_clamps_zero_limit_to_one() {
        let p = pagination("/items?limit=0").await.unwrap();
        assert_eq!(p.limit, 1);
    }

    #[tokio::test]
    async fn pagination_rejects_non_numeric_limit() {
        let (status, err) = pagination("/items?limit=abc").await.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(err.code, "INVALID_QUERY");
    }

    #[tokio::test]
    async fn pagination_builds_list_response() {
        let p = pagination("/items?limit=5&offset=15").await.unwrap();
        let resp = p.list_response(vec![1, 2, 3], 42);
        assert_eq!(resp.limit, 5);
        assert_eq!(resp.offset, 15);
        assert_eq!(resp.total, 42);
        assert_eq!(resp.data.len(), 3);
    }

    #[tokio::test]
    async fn cursor_defaults_when_absent() {
        let c = cursor("/feed").await.unwrap();
        assert_eq!(c.cursor, None);
        assert_eq!(c.limit, Pagination::DEFAULT_LIMIT);
    }

    #[tokio::test]
    async fn cursor_parses_cursor_and_limit() {
        let c = cursor("/feed?cursor=abc123&limit=5").await.unwrap();
        assert_eq!(c.cursor.as_deref(), Some("abc123"));
        assert_eq!(c.limit, 5);
    }

    #[tokio::test]
    async fn cursor_response_sets_has_more_from_next_cursor() {
        let c = cursor("/feed").await.unwrap();
        let more = c.cursor_response(vec![1], Some("next".into()));
        assert!(more.has_more);
        assert_eq!(more.next_cursor.as_deref(), Some("next"));

        let done = c.cursor_response(vec![1], None);
        assert!(!done.has_more);
        assert_eq!(done.next_cursor, None);
    }
}

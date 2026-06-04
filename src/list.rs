use axum::{
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// A generic paginated collection response.
///
/// Serializes as:
/// ```json
/// { "data": [...], "total": 42, "limit": 50, "offset": 0 }
/// ```
///
/// # Example
///
/// ```rust
/// use axum::response::IntoResponse;
/// use axum_api_kit::ListResponse;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Item { id: String }
///
/// async fn list_items() -> impl IntoResponse {
///     ListResponse {
///         data: vec![Item { id: "1".into() }],
///         total: 1,
///         limit: 50,
///         offset: 0,
///     }
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ListResponse<T: Serialize> {
    /// The items in this page.
    pub data: Vec<T>,
    /// Total number of matching items across all pages.
    pub total: i64,
    /// Maximum number of items per page (as requested).
    pub limit: u32,
    /// Zero-based offset of the first item in this page.
    pub offset: u32,
}

impl<T: Serialize> IntoResponse for ListResponse<T> {
    fn into_response(self) -> Response {
        Json(self).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Serialize)]
    struct Item {
        id: String,
    }

    #[test]
    fn serializes_fields_correctly() {
        let resp = ListResponse {
            data: vec![Item { id: "abc".into() }],
            total: 10,
            limit: 25,
            offset: 0,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["total"], 10);
        assert_eq!(v["limit"], 25);
        assert_eq!(v["offset"], 0);
        assert_eq!(v["data"][0]["id"], "abc");
    }

    #[test]
    fn empty_data_serializes() {
        let resp: ListResponse<Item> = ListResponse {
            data: vec![],
            total: 0,
            limit: 50,
            offset: 0,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["data"], json!([]));
        assert_eq!(v["total"], 0);
    }

    #[test]
    fn offset_pagination_fields() {
        let resp: ListResponse<Item> = ListResponse {
            data: vec![],
            total: 100,
            limit: 10,
            offset: 30,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["limit"], 10);
        assert_eq!(v["offset"], 30);
    }
}

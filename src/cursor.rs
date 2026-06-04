use axum::{
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// A cursor-based paginated collection response.
///
/// Unlike [`ListResponse`](crate::ListResponse) which uses offset/limit pagination,
/// `CursorResponse` uses opaque cursor tokens for keyset-based pagination.
/// This is ideal for large or streaming datasets where total count is expensive to compute.
///
/// Serializes as:
/// ```json
/// { "data": [...], "next_cursor": "abc123", "has_more": true }
/// { "data": [...], "next_cursor": null, "has_more": false }
/// ```
///
/// # Example
///
/// ```rust
/// use axum::response::IntoResponse;
/// use axum_api_kit::CursorResponse;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Item { id: String }
///
/// async fn list_items(cursor: Option<String>) -> impl IntoResponse {
///     CursorResponse {
///         data: vec![Item { id: "1".into() }],
///         next_cursor: Some("cursor_for_page_2".into()),
///         has_more: true,
///     }
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CursorResponse<T: Serialize> {
    /// The items in this page.
    pub data: Vec<T>,
    /// Opaque cursor token for fetching the next page.
    /// `None` indicates this is the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Whether more items exist after this page.
    pub has_more: bool,
}

impl<T: Serialize> IntoResponse for CursorResponse<T> {
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
    fn serializes_with_next_cursor() {
        let resp = CursorResponse {
            data: vec![Item { id: "1".into() }],
            next_cursor: Some("cursor_abc".into()),
            has_more: true,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["data"][0]["id"], "1");
        assert_eq!(v["next_cursor"], "cursor_abc");
        assert_eq!(v["has_more"], true);
    }

    #[test]
    fn serializes_without_next_cursor_when_none() {
        let resp: CursorResponse<Item> = CursorResponse {
            data: vec![],
            next_cursor: None,
            has_more: false,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert!(v.get("next_cursor").is_none());
        assert_eq!(v["has_more"], false);
    }

    #[test]
    fn has_more_false_at_end() {
        let resp = CursorResponse {
            data: vec![Item { id: "final".into() }],
            next_cursor: None,
            has_more: false,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["has_more"], false);
        assert!(v.get("next_cursor").is_none());
    }

    #[test]
    fn has_more_true_with_cursor() {
        let resp = CursorResponse {
            data: vec![Item { id: "x".into() }],
            next_cursor: Some("token".into()),
            has_more: true,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["has_more"], true);
        assert_eq!(v["next_cursor"], "token");
    }

    #[test]
    fn empty_data_with_no_cursor() {
        let resp: CursorResponse<Item> = CursorResponse {
            data: vec![],
            next_cursor: None,
            has_more: false,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["data"], json!([]));
        assert_eq!(v["has_more"], false);
    }

    #[test]
    fn multiple_items() {
        let resp = CursorResponse {
            data: vec![
                Item { id: "1".into() },
                Item { id: "2".into() },
                Item { id: "3".into() },
            ],
            next_cursor: Some("next".into()),
            has_more: true,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["data"].as_array().unwrap().len(), 3);
        assert_eq!(v["data"][1]["id"], "2");
    }
}

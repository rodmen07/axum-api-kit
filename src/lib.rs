//! Shared response types for Axum JSON APIs.
//!
//! Provides building blocks that every Axum CRUD service needs but always
//! re-defines from scratch:
//!
//! - [`ApiError`] - a machine-readable JSON error body with `code`, `message`, and optional
//!   `details`, plus factory helpers that return `(StatusCode, Json<ApiError>)` tuples ready
//!   for use with Axum's [`IntoResponse`](axum::response::IntoResponse). Supports `From`
//!   conversions for common error types. With the optional `validator` feature enabled,
//!   also supports converting `validator::ValidationErrors` into structured field errors.
//!   With the optional `sqlx` feature enabled, also supports converting `sqlx::Error` into
//!   semantically correct HTTP status codes (404, 409, 422, 503, 500).
//! - [`ListResponse<T>`] - a generic offset/limit paginated collection response with `data`,
//!   `total`, `limit`, and `offset` fields.
//! - [`CursorResponse<T>`] - a generic cursor-based paginated collection response for large
//!   datasets or feeds, with `data`, `next_cursor`, and `has_more` fields.
//! - [`HealthResponse`] - a health-check response with `status` field supporting `ok`,
//!   `degraded`, and `unhealthy` states.
//!
//! With optional feature flags enabled, the kit also provides request extractors that
//! reject with an [`ApiError`] body on failure:
//!
//! - `ValidatedJson<T>` (feature `validator`) - deserializes a JSON body and runs
//!   `validator` validation before the handler runs.
//! - `Pagination` and `CursorPagination` (feature `extract`) - parse `limit`/`offset` and
//!   `cursor`/`limit` query parameters into typed values, with `list_response` /
//!   `cursor_response` helpers that build the matching response type.
//!
//! It also ships observability middleware:
//!
//! - `propagate_request_id` and `trace_requests` (feature `trace`) - assign an
//!   `x-request-id` correlation id (extractable via `RequestId`) and emit a structured
//!   `tracing` event with method, path, status, and latency for each request.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use axum::{Json, http::StatusCode, response::IntoResponse};
//! use axum_api_kit::{ApiError, ListResponse, CursorResponse, HealthResponse};
//! use serde::Serialize;
//!
//! #[derive(Serialize)]
//! struct Item { id: String }
//!
//! async fn list_items() -> impl IntoResponse {
//!     let items = vec![Item { id: "1".into() }];
//!     Json(ListResponse { data: items, total: 1, limit: 50, offset: 0 })
//! }
//!
//! async fn feed_items(cursor: Option<String>) -> impl IntoResponse {
//!     let items = vec![Item { id: "1".into() }];
//!     CursorResponse { data: items, next_cursor: Some("abc".into()), has_more: true }
//! }
//!
//! async fn get_item() -> impl IntoResponse {
//!     ApiError::not_found("item not found")
//! }
//!
//! async fn health() -> impl IntoResponse {
//!     HealthResponse::ok()
//! }
//! ```

mod cursor;
mod error;
mod health;
mod list;
#[cfg(feature = "extract")]
mod pagination;
#[cfg(feature = "trace")]
mod trace;
#[cfg(feature = "validator")]
mod validated;

pub use cursor::CursorResponse;
pub use error::ApiError;
pub use health::HealthResponse;
pub use list::ListResponse;
#[cfg(feature = "extract")]
pub use pagination::{CursorPagination, Pagination};
#[cfg(feature = "trace")]
pub use trace::{propagate_request_id, trace_requests, RequestId, REQUEST_ID_HEADER};
#[cfg(feature = "validator")]
pub use validated::ValidatedJson;

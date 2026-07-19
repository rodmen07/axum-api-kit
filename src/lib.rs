//! Shared response types for Axum JSON APIs.
//!
//! Provides building blocks that every Axum CRUD service needs but always
//! re-defines from scratch:
//!
//! - [`ApiError`] - a machine-readable JSON error body with `code`, `message`, and optional
//!   `details`, plus factory helpers that return `(StatusCode, Json<ApiError>)` tuples ready
//!   for use with Axum's [`IntoResponse`](axum::response::IntoResponse). The
//!   `too_many_requests_with_retry_after` and `service_unavailable_with_retry_after`
//!   factories additionally emit a delay-seconds `Retry-After` header. Supports `From`
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
//! - [`Created<T>`], [`Accepted<T>`], and [`NoContent`] - success-side responses for the
//!   rest of the CRUD lifecycle: `201 Created` (with an optional `Location` header),
//!   `202 Accepted`, and `204 No Content`.
//!
//! With optional feature flags enabled, the kit also provides request extractors that
//! reject with an [`ApiError`] body on failure:
//!
//! - `ValidatedJson<T>` (feature `validator`) - deserializes a JSON body and runs
//!   `validator` validation before the handler runs.
//! - `Pagination` and `CursorPagination` (feature `extract`) - parse `limit`/`offset` and
//!   `cursor`/`limit` query parameters into typed values, with `list_response` /
//!   `cursor_response` helpers that build the matching response type.
//! - `ApiJson<T>` (feature `extract`) - a drop-in replacement for `axum::Json` whose
//!   extraction failures reject with an `ApiError` body instead of Axum's plain-text default.
//!
//! It also ships observability middleware:
//!
//! - `propagate_request_id` and `trace_requests` (feature `trace`) - assign an
//!   `x-request-id` correlation id (extractable via `RequestId`) and emit a structured
//!   `tracing` event with method, path, status, and latency for each request.
//!
//! And service-wiring helpers:
//!
//! - `health_routes` and `liveness` (feature `router`) - a `Router` exposing `/healthz` and
//!   `/readyz` probes backed by `HealthResponse`.
//! - `cors_allowing` and `cors_permissive` (feature `cors`) - build a `tower_http`
//!   `CorsLayer` with sensible defaults.
//!
//! And an RFC 9457 error format:
//!
//! - `Problem` (feature `problem`) - an RFC 9457 problem+json response type with builder
//!   methods, lossless bridges from [`ApiError`], and Retry-After support. Emits
//!   `Content-Type: application/problem+json`, with opt-in Accept-header content
//!   negotiation (the `ProblemFormat` extractor plus `Problem::into_response_for` /
//!   `Problem::into_response_with`) that serves the same body as plain `application/json`
//!   only when the client strictly prefers it.
//!
//! With the `openapi` feature, all four response types derive `utoipa::ToSchema` so they
//! can be referenced from a `utoipa` `OpenApi` document and appear in generated specs.
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

#[cfg(feature = "extract")]
mod apijson;
#[cfg(feature = "cors")]
mod cors;
mod cursor;
mod error;
mod health;
mod list;
#[cfg(feature = "extract")]
mod pagination;
#[cfg(feature = "problem")]
mod problem;
#[cfg(feature = "router")]
mod router;
mod success;
#[cfg(feature = "trace")]
mod trace;
#[cfg(feature = "validator")]
mod validated;

#[cfg(feature = "extract")]
pub use apijson::ApiJson;
#[cfg(feature = "cors")]
pub use cors::{cors_allowing, cors_permissive};
pub use cursor::CursorResponse;
pub use error::ApiError;
pub use health::HealthResponse;
pub use list::ListResponse;
#[cfg(feature = "extract")]
pub use pagination::{CursorPagination, Pagination};
#[cfg(feature = "problem")]
pub use problem::{Problem, ProblemFormat, APPLICATION_PROBLEM_JSON};
#[cfg(feature = "router")]
pub use router::{health_routes, liveness};
pub use success::{Accepted, Created, NoContent};
#[cfg(feature = "trace")]
pub use trace::{propagate_request_id, trace_requests, RequestId, REQUEST_ID_HEADER};
#[cfg(feature = "validator")]
pub use validated::ValidatedJson;

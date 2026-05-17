//! Shared response types for Axum JSON APIs.
//!
//! Provides three building blocks that every Axum CRUD service needs but always
//! re-defines from scratch:
//!
//! - [`ApiError`] - a machine-readable JSON error body with `code`, `message`, and optional
//!   `details`, plus factory helpers that return `(StatusCode, Json<ApiError>)` tuples ready
//!   for use with Axum's [`IntoResponse`](axum::response::IntoResponse).
//! - [`ListResponse<T>`] - a generic paginated collection response with `data`, `total`,
//!   `limit`, and `offset` fields.
//! - [`HealthResponse`] - a minimal `{ "status": "ok" }` response for health-check endpoints.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use axum::{Json, http::StatusCode, response::IntoResponse};
//! use axum_api_kit::{ApiError, ListResponse, HealthResponse};
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
//! async fn get_item() -> impl IntoResponse {
//!     ApiError::not_found("item not found")
//! }
//!
//! async fn health() -> impl IntoResponse {
//!     HealthResponse::ok()
//! }
//! ```

mod error;
mod health;
mod list;

pub use error::ApiError;
pub use health::HealthResponse;
pub use list::ListResponse;

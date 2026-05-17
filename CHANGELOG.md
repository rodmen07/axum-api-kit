# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-05-17

### Added

- Optional `validator` feature flag with `validator` crate integration.
- `From<validator::ValidationErrors> for ApiError` (when `validator` feature is enabled).
- Deterministic validation error details shape under `details.fields` for field-level errors.
- Feature-gated tests for validator conversion paths and serialized payload structure.

## [0.3.0] - 2026-05-17

### Added

- `From<std::io::Error>` impl on `ApiError` - enables using `?` operator in handlers for I/O operations, automatically converting to HTTP 500.
- `From<serde_json::Error>` impl on `ApiError` - enables using `?` operator for JSON parsing errors.
- `ApiError::with_source(source_msg)` method - attach error source/context to details field for debugging chains: `ApiError::not_found("user").with_source("SELECT * FROM users")`.
- `CursorResponse<T>` - new cursor-based pagination type for large datasets and feeds, with `data`, `next_cursor: Option<String>`, and `has_more` fields.
- Unit tests for all new error conversion and cursor pagination features.

## [0.2.0] - 2026-05-17

### Added

- `ApiError` now implements `std::fmt::Display` and `std::error::Error`, enabling `?`-propagation in handlers.
- New `ApiError` factory methods: `too_many_requests` (429), `service_unavailable` (503), `not_implemented` (501).
- `HealthResponse::degraded()` - returns `{ "status": "degraded" }` with HTTP 200, for services operating in a reduced capacity.
- `HealthResponse::unhealthy()` - returns `{ "status": "unhealthy" }` with HTTP 503, for services unable to fulfill requests.
- GitHub Actions CI workflow: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` on push and PR.
- Unit test suites in all three modules (`error`, `health`, `list`).

### Changed

- `HealthResponse` now stores an internal (non-serialized) `StatusCode` field so that `degraded` and `unhealthy` variants can return the correct HTTP status code via `IntoResponse`. No change to the serialized shape.

## [0.1.0] - 2026-01-01

### Added

- `ApiError` with factory methods for 400, 401, 403, 404, 409, 422, 500 (generic and db).
- `ListResponse<T>` for offset/limit paginated collections.
- `HealthResponse` with a single `ok()` constructor.

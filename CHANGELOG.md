# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

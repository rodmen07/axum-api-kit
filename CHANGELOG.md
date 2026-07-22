# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.0.0] - 2026-07-22

### Changed

- **BREAKING (opt-in `validator` feature only):** the optional `validator`
  dependency moves from `0.18` to `0.20`. The `Validate` trait bound on
  `ValidatedJson<T>` is version-coupled, so any consumer that enables the
  `validator` feature must move its own `validator` dependency (and any
  `#[derive(Validate)]` attribute syntax) to `0.20` as well. Consumers who do
  NOT enable the `validator` feature are unaffected.

### Security

- Clears two advisories that reached the crate only through `validator 0.18`'s
  transitive dependencies: **RUSTSEC-2024-0421** (`idna` 0.5 Punycode) and
  **RUSTSEC-2024-0370** (`proc-macro-error` unmaintained). `validator 0.20`
  pulls `idna` 1.x and `proc-macro-error3`. The CI `Security audit` job no
  longer needs to `--ignore` either advisory.

### Unchanged (verified)

- This crate's own public API and the byte-for-byte response output of every
  type — including `ApiError`, the `ValidatedJson` validation-failure body, and
  all extractor rejection bodies — are unchanged. The existing byte-identity
  tests (`tests/rejection_bytes.rs`, `validation_failure_bytes_are_locked` et
  al.) pass without modification against `validator 0.20`. The major version
  reflects the version-coupled `validator` dependency only.

## [1.4.0] - 2026-07-19

### Added

- Accept-header content negotiation for `Problem` responses (opt-in, `problem`
  feature): `ProblemFormat` with `negotiate(&HeaderMap)`, `content_type()`, and
  an infallible `FromRequestParts` extractor, plus
  `Problem::into_response_for(&HeaderMap)` and
  `Problem::into_response_with(ProblemFormat)`. Plain `application/json` is
  served only when the client strictly prefers it; every ambiguous case (no
  `Accept`, `*/*`, ties, malformed q-values) stays `application/problem+json`.
  Bodies are byte-identical across both formats and `Retry-After` is preserved.
  The existing `IntoResponse for Problem` output is unchanged and locked by a
  byte-identity test. The minimal Accept parser is dependency-free and
  documented as not a full RFC 9110 implementation.
- RFC 9457 extractor rejections (opt-in): new sibling extractors
  `ProblemJson<T>` (`problem` + `extract`) and `ProblemValidatedJson<T>`
  (`problem` + `validator`) whose `ProblemRejection` emits `Problem` bodies
  with the same status, stable `code`, and field-level `details` as the
  existing extractors (carried as extension members), negotiated per request
  via `ProblemFormat`. `ApiJson`/`ValidatedJson` rejections are unchanged and
  locked byte-identical by a new rejection-bytes test suite; sibling types were
  chosen so cargo feature unification can never silently reformat error
  responses for users of the existing extractors.

## [1.3.0] - 2026-07-18

### Added

- New `problem` feature (no new dependencies): RFC 9457 `Problem` response type emitting
  `Content-Type: application/problem+json`, with chainable builders (`with_type`,
  `with_detail`, `with_instance`, `with_extension`, `with_retry_after`), the
  `APPLICATION_PROBLEM_JSON` constant, lossless `From<(StatusCode, ApiError)>` and
  `From<(StatusCode, Json<ApiError>)>` bridges, and `ApiError::into_problem`.
  Accept-header content negotiation and problem+json extractor rejections are explicitly
  out of scope for this release.
- Always-on `ApiError::too_many_requests_with_retry_after` and
  `ApiError::service_unavailable_with_retry_after` factories emitting a delay-seconds
  `Retry-After` header with the standard `{code, message}` JSON body.

## [1.2.7] - 2026-06-27

### Added

- Added extractor-level ApiJson tests that lock in acceptance of
  `application/json; charset=utf-8` and vendor `+json` media types.

## [1.2.6] - 2026-06-27

### Added

- New ApiJson interaction coverage proving vendor `+json` media types (for
  example `application/vnd.api+json`) are accepted while preserving request-id
  and CORS headers under layered trace + CORS middleware.

## [1.2.5] - 2026-06-27

### Added

- New ApiJson interaction coverage proving requests with missing `Content-Type`
  reject as `415 UNSUPPORTED_MEDIA_TYPE` and still preserve request-id and
  CORS headers under layered trace + CORS middleware.

## [1.2.4] - 2026-06-27

### Added

- New ApiJson interaction coverage proving `application/json; charset=utf-8`
  requests are accepted through layered trace + CORS middleware while
  preserving request-id and CORS headers on successful responses.

## [1.2.3] - 2026-06-27

### Added

- Additional ApiJson interaction coverage for wrong `Content-Type` requests,
  verifying `415 UNSUPPORTED_MEDIA_TYPE` rejections preserve request-id and
  CORS headers when trace and CORS middleware are layered.

## [1.2.2] - 2026-06-27

### Added

- New ApiJson integration tests that validate structured rejection payloads for
  malformed JSON and unsupported media type inputs.
- Cross-feature interaction coverage proving ApiJson rejections preserve
  correlation and CORS headers when composed with trace and CORS middleware.
- Additional ApiJson interaction coverage for wrong-shape JSON rejections
  (`422 INVALID_BODY`) while preserving request-id and CORS headers.

## [1.2.1] - 2026-06-27

### Added

- New cross-feature integration tests behind `router + trace + cors` in
  `tests/feature_interactions.rs`, covering:
  - request-id propagation in health endpoints when tracing middleware is active
  - CORS allow-origin behavior when health routes are merged and layered
  - unhealthy readiness responses retaining correlation headers

### Changed

- Internal formatting-only cleanup to keep the repository green under strict
  `cargo fmt --check` CI gates.

## [1.2.0] - 2026-06-07

### Added

- `ApiJson<T>` (feature `extract`) - a drop-in replacement for `axum::Json` whose extraction
  failures reject with an `ApiError` JSON body instead of Axum's default plain-text response
  (malformed JSON -> 400 `INVALID_JSON`, wrong shape -> 422 `INVALID_BODY`, missing/incorrect
  `Content-Type` -> 415 `UNSUPPORTED_MEDIA_TYPE`). It also implements `IntoResponse`, so it
  works as a handler return type like `axum::Json`. Unlike `ValidatedJson`, it needs no
  `validator` dependency.

### Internal

- The `JsonRejection` -> `ApiError` mapping is now a single shared helper used by both
  `ValidatedJson` and `ApiJson` (no behavior change to `ValidatedJson`).

## [1.1.0] - 2026-06-07

### Added

- Success-side response helpers that round out the CRUD response lifecycle:
  - `Created<T>` - `201 Created` carrying the new resource as a JSON body, with an
    optional `Location` header via `.with_location(...)` (an invalid header value is
    omitted rather than panicking).
  - `Accepted<T>` - `202 Accepted` carrying a JSON body (e.g. a job handle) for work
    that completes asynchronously.
  - `NoContent` - `204 No Content` with an empty body, for deletes and empty updates.
- All three implement `IntoResponse` and are exported from the crate root.

These are purely additive; no existing API changed.

## [1.0.0] - 2026-06-03

First stable release. The public API is now covered by semantic versioning: breaking
changes will only ship in a future major version. No new functionality versus 0.10.0.

### Changed (breaking)

- Renamed `permissive_cors()` to `cors_permissive()` so both CORS helpers share a `cors_` prefix.
- Renamed `ApiError::unprocessable()` to `ApiError::unprocessable_entity()` to match the HTTP 422 status name and the other factory methods.

### Migration

- `permissive_cors()` -> `cors_permissive()`
- `ApiError::unprocessable(msg)` -> `ApiError::unprocessable_entity(msg)`

## [0.10.0] - 2026-06-03

### Added

- New `openapi` feature flag (pulls `utoipa` 5): `ApiError`, `ListResponse<T>`, `CursorResponse<T>`, and `HealthResponse` derive `utoipa::ToSchema`, so they can be referenced from a `utoipa` `OpenApi` document and appear in generated specs.
- Integration test verifying the response types register as OpenAPI components.

### Notes

- `ApiError::details` is mapped to an optional `Object` schema; `HealthResponse::status` is mapped to a `String`. Schema derivation only affects builds with the `openapi` feature enabled.

## [0.9.0] - 2026-06-03

### Added

- New `router` feature flag with service-wiring helpers:
  - `health_routes(readiness)` - builds a state-generic `Router` with `/healthz` (liveness, always `ok`) and `/readyz` (readiness, runs the supplied async check and returns its `HealthResponse`, so `unhealthy` yields HTTP 503). Mergeable into a stateful app via `app.merge(...)`.
  - `liveness` - a reusable liveness handler returning `HealthResponse::ok()`.
- New `cors` feature flag (pulls `tower-http`) with CORS helpers:
  - `cors_allowing(origins)` - a `CorsLayer` for a known origin allow-list with the common REST methods, `content-type`/`authorization` headers, and credentials enabled.
  - `permissive_cors()` - a permissive `CorsLayer` for local development.
- Router-based tests for the health probes and CORS layer (behind the respective features).

## [0.8.0] - 2026-06-03

### Added

- New `trace` feature flag with observability middleware:
  - `propagate_request_id` - Axum middleware that reuses an incoming `x-request-id` header (or generates a UUID v4), stores it in request extensions, and echoes it on the response.
  - `RequestId` - newtype stored in request extensions, with a `FromRequestParts` impl so handlers can extract the current request's correlation id.
  - `trace_requests` - Axum middleware that emits an `info`-level `tracing` event per request with `method`, `path`, `status`, `latency_ms`, and `request_id`.
  - `REQUEST_ID_HEADER` constant (`"x-request-id"`).
- Router-based integration tests for the middleware (behind the `trace` feature).

## [0.7.0] - 2026-06-03

### Added

- New `extract` feature flag with request extractors:
  - `Pagination` - parses `limit`/`offset` query parameters; `limit` defaults to 50 and is clamped to `1..=100` (`Pagination::DEFAULT_LIMIT` / `Pagination::MAX_LIMIT`). Includes a `list_response(data, total)` helper that builds a `ListResponse`.
  - `CursorPagination` - parses an optional `cursor` token plus a clamped `limit`. Includes a `cursor_response(data, next_cursor)` helper that builds a `CursorResponse` and derives `has_more` from the cursor.
  - Invalid query strings reject with `400 Bad Request` and code `INVALID_QUERY`.
- `ValidatedJson<T>` extractor (requires the `validator` feature) - deserializes a JSON body and runs `validator` validation before the handler runs. Rejects with semantic `ApiError` bodies: `INVALID_JSON` (400), `INVALID_BODY` (422), `UNSUPPORTED_MEDIA_TYPE` (415), and `VALIDATION_ERROR` (422) with field-level details.
- Async extractor unit tests behind the relevant features.

### Changed

- The `validator` feature now also enables `validator/derive`, so `#[derive(Validate)]` is available wherever the feature is used.
- CI now runs clippy and the test suite with `--all-features` (and the default-feature test run) so feature-gated code is exercised.

## [0.6.0] - 2026-06-03

### Added

- First public release on crates.io.
- `LICENSE` file with the MIT license text.
- Package metadata: `authors`, `homepage`, `documentation`, and `rust-version` (MSRV 1.75).
- `[package.metadata.docs.rs]` with `all-features = true` so optional features render on docs.rs.

No API changes from 0.5.0.

## [0.5.0] - 2026-05-17

### Added

- Optional `sqlx` feature flag with sqlx 0.8 integration.
- `From<sqlx::Error> for ApiError` (when `sqlx` feature is enabled) with semantic HTTP status mapping: `RowNotFound` -> 404, unique/FK violations -> 409, check violations -> 422, pool errors -> 503, all others -> 500.
- Feature-gated unit tests for sqlx error conversion paths.

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

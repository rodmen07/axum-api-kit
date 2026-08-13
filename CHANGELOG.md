# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- First response-level test coverage for the success-side helpers `Created`,
  `Accepted` and `NoContent` (`src/success.rs`, shipped in 1.1.0 on 2026-06-07),
  the last no-feature-flag response types with none. They appeared nowhere in
  `tests/` at all: their unit tests do render a `Response`, but read it through
  `serde_json::from_slice(..).unwrap_or(Value::Null)` and assert one navigated
  field, so the `Content-Type` of every created resource this crate emits, the
  exact body bytes, and everything axum's service stack adds on the way out were
  unobserved, and none of the three had ever been driven through a real
  `Router`. `tests/default_response_contracts.rs` now locks status,
  `Content-Type`, body bytes and the `Location` header for each, both directly
  and through `Router::oneshot` — including that a 204 carries no
  `Content-Type` and declares `content-length: 0`.

### Changed

- **`Created` and `Accepted` now document what they emit when the body fails to
  serialize, which is not what a reader would assume.** Both build their
  response as `(StatusCode::CREATED, Json(self.data))`, and the tuple applies
  the status *after* `Json` has rendered its own `500 Internal Server Error`, so
  a failing `Serialize` impl produces **`201 Created`** (or `202 Accepted`) with
  a `text/plain` serde error message as the body — and, for `Created`, with the
  `Location` header still attached. Behaviour is unchanged: fixing it changes
  bytes a published 2.x consumer can already observe, so it is filed as a bug
  and pinned by `known_gap_` tests rather than silently altered. The rustdoc now
  says so, and names `ListResponse` / `CursorResponse` — generic over the same
  `T`, rendered as a bare `Json(self).into_response()` — as the in-crate
  demonstration that `500` is the reachable correct answer.

### Fixed

- **An explicit JSON `null` in `ApiError::details` is now treated as "no details" everywhere, so
  the wire can no longer contradict the schema.** `details` is documented and generated as
  `nullable = false` — absence is expressed by omitting the key, never by sending `null` — but
  `#[serde(skip_serializing_if = "Option::is_none")]` skips only `None`, and `details` is the one
  such field whose inner type (`serde_json::Value`) can itself BE null. So
  `ApiError::new(..).with_details(serde_json::json!(null))` emitted
  `{"code":..,"message":..,"details":null}`: a body a client generated from this crate's own
  OpenAPI document may reject. The skip predicate is now `details_is_absent`, which treats `None`
  and `Some(Value::Null)` alike, and two other renderings of the same value defer to it rather than
  re-deciding — `with_source`'s coercion no longer nests a `null` under the `"details"` key
  (which would have re-emitted the banned value one level down), and the `Problem` bridge no
  longer copies a `null` into a `"details"` extension member. Only `null` is affected: `{}`,
  `false`, `0` and `""` are values a caller attached and are still sent. Response bytes change
  only for inputs that were emitting a `null` carrying no information the omitted key does not,
  so this is classified a PATCH-level bug fix under this crate's Semver policy; no public API,
  signature or feature flag moved (`Semver compatibility` green). The field can still HOLD
  `Some(Value::Null)` — the fix is about what is rendered, not about rewriting the caller's value.

## [2.2.0] - 2026-08-12

### Added

- **`router`: unmatched routes stop speaking plain text.** New `api_fallback` (404) and
  `api_method_not_allowed` (405) handlers, wired with `Router::fallback` and
  `Router::method_not_allowed_fallback`, give the two responses axum otherwise sends with an
  **empty body** an `ApiError` JSON body like every other failure the kit covers —
  `{"code":"NOT_FOUND","message":"no route matches this path"}` and
  `{"code":"METHOD_NOT_ALLOWED","message":"this path does not support the request method"}`,
  both as `application/json`. Measured, not assumed: with no fallback wired axum answers an
  unmatched path with `404`, no `Content-Type` and a zero-byte body, and a wrong method with
  `405` and a zero-byte body — the one pair of responses in a kit-built app that a client's JSON
  error handler cannot parse. The `Allow` header axum computes for a `405` is still sent, because
  axum sets it on the way out rather than in the fallback. With `problem` also enabled,
  `problem_fallback` and `problem_method_not_allowed` render the same status and the same `code`
  as RFC 9457 `application/problem+json`; the flavour is chosen by naming the handler, so
  enabling `problem` changes nothing about what `api_fallback` emits. Both flavours are built
  from one shared error definition, so they cannot drift apart on status or `code`. No new
  feature flag and no new dependency; `health_routes` is untouched. Note the axum ordering rule
  the rustdoc and `tests/router_fallbacks.rs` both pin: `method_not_allowed_fallback` only
  rewrites routes registered *before* the call, so wire it after the last `route(..)`.
- **`openapi` + `extract`: the pagination extractors' query parameters now enter the
  generated document.** `Pagination` and `CursorPagination` derive
  `utoipa::IntoParams`, so a handler documented with
  `#[utoipa::path(..., params(Pagination))]` contributes `limit`/`offset` (and
  `cursor`/`limit`) as `in: query` parameters carrying the defaults and bounds the
  extractors already enforce — `limit` with `default: 50`, `minimum: 1`,
  `maximum: 100`, `offset` with `default: 0`, and every one of them
  `required: false`. Previously the kit enforced these rules while the spec it
  generated said nothing about them, so a generated client had to be told by hand.
  Gated on `openapi` **and** `extract` jointly rather than behind a new feature flag;
  a consumer with only one of the two sees no change, and no dependency was added
  (`utoipa` is already the `openapi` dependency). `CursorPagination::cursor` needs no
  nullability opt-out, unlike the response types: `IntoParams` emits `Option<String>`
  as `{"type": "string"}` with `required: false`, where `ToSchema` would have produced
  a `string | null` union. That is measured on the generated document and guarded
  class-wide by `no_contributed_parameter_admits_null`. The declared bounds are pinned to
  `Pagination::MAX_LIMIT` / `DEFAULT_LIMIT` by `tests/openapi_params.rs`, so the
  document cannot drift away from what the code enforces.

### Fixed

- **`ApiError::with_source` no longer silently discards the source.** Both
  builders write into the single `details` field, and five input shapes lost
  data: `with_details` replaced the whole value, dropping a `source` set before
  it, and `with_source` wrote only into a JSON object, dropping the source
  whenever `details` held an array, a string, a number or `null`. Neither
  reported anything — the diagnostic an operator deliberately attached simply
  never reached the response. Now `with_details` carries an existing `"source"`
  across the replacement (a `"source"` key inside the new `details` still wins,
  being the later explicit assignment), and `with_source` coerces a non-object
  `details` to an object, preserving the caller's value under a `"details"` key.
  **This changes response bytes only for inputs that previously discarded data;**
  the two shapes that already worked (an object `details`, or no `details` at
  all) are byte-identical and locked as such in
  `tests/default_response_contracts.rs`. `with_source`'s documentation example
  chained the builders in the order that used to lose the source, and asserted
  nothing, so it compiled and passed as a doctest while demonstrating the loss;
  both examples now assert their JSON.

- `ApiError::details`'s documentation claimed the field is "never sent as
  `null`". `skip_serializing_if = "Option::is_none"` does not skip
  `Some(Value::Null)`, so `with_details(serde_json::json!(null))` emits
  `"details": null` — which the `schema(nullable = false)` this field carries
  says cannot happen. The doc now describes what the field actually emits and
  names the gap; the wire bytes and the schema are unchanged, and the
  contradiction is pinned by
  `known_gap_api_error_details_can_serialize_as_null`.

- **`openapi` feature: five optional fields were typed as nullable unions in the
  generated document while the wire omits the key entirely.** `utoipa` derives
  nullability from `Option<T>`, so `ApiError::details`, `CursorResponse::next_cursor`,
  and `Problem`'s `type`, `detail`, and `instance` were emitted as
  `"type": ["string", "null"]` (`["object", "null"]` for `details`) — but every one of
  them carries `#[serde(skip_serializing_if = "Option::is_none")]`, so absent means the
  key is missing, never `null`. A client generated from that document got a
  `T | null` union whose `null` arm is unreachable. Each field now declares
  `schema(nullable = false)`; absence is carried by staying out of `required`, which is
  unchanged. **No response bytes change** and no Rust API changes — this affects only
  the document the `openapi` feature emits. Consumers who hand-wrote a `null` branch
  against the old schema can delete it; it was never reachable.
  A class guard, `tests/openapi.rs::no_registered_property_admits_null`, now walks every
  property the document declares (descending into the `allOf` branches that `Problem`'s
  flattened `extensions` map produces) and fails if any admits `null`, so a newly added
  `Option` field is covered the day it lands.

- `CursorResponse`'s documentation described the last page as
  `{ "data": [...], "next_cursor": null, "has_more": false }`. It has never
  serialized that way: `next_cursor` carries
  `#[serde(skip_serializing_if = "Option::is_none")]`, so on the last page the
  key is **omitted from the body entirely** rather than sent as `null`. The
  docs now describe the bytes the type has always emitted — the bytes are
  unchanged, and are now locked by `tests/default_response_contracts.rs`.
  Clients should branch on `has_more`, or on the presence of the key, rather
  than comparing `next_cursor` against `null`. The same correction reaches
  `openapi`-feature users, since `utoipa` copies this rustdoc into the
  generated schema's `description`.

### Added

- First response-level test coverage for `ApiError`, the fourth type from the
  crate's first commit and the one the earlier sweep below left out: status
  code, `Content-Type` and body bytes for the `(StatusCode, Json<ApiError>)`
  factories, and — for `too_many_requests_with_retry_after` and
  `service_unavailable_with_retry_after` — proof that the `Retry-After` header
  survives `into_response` with the documented rounding (1500ms becomes `"2"`)
  and does not displace the JSON content type. Nothing had ever rendered an
  `ApiError` into a `Response` under the default feature set; the unit tests
  destructure the factory tuple instead.

- First response-level test coverage for the three types available with no
  feature flags (`ListResponse`, `CursorResponse`, `HealthResponse`): status
  code, `Content-Type`, and body bytes are now locked by
  `tests/default_response_contracts.rs`. Their `IntoResponse` impls were
  previously unexercised under the default feature set — `HealthResponse`'s
  503 mapping was observed only through `health_routes`, behind the `router`
  feature.

### Changed

- **`cors` feature: the `tower-http` requirement is widened from `0.6` to
  `>=0.6, <0.8`, accepting the new tower-http 0.7.** This is a widen, not a
  bump, because `cors_allowing`/`cors_permissive` publicly return
  `tower_http::cors::CorsLayer`: a hard bump to `0.7` would change that
  public type's identity out from under a consumer whose own `tower-http = "0.6"`
  names it, while the widened range lets cargo unify with either side. A fresh
  resolution now picks tower-http 0.7.0 (rustc floor 1.65, under this crate's
  1.81 MSRV; still tower-layer 0.3 and http 1, so `Router::layer` compatibility
  is unchanged). Behavioral note when your graph resolves 0.7: upstream relaxed
  the cors `Vary` header defaults (tower-http #674); tower-http 0.7's breaking
  changes are confined to modules this crate does not use (compression,
  follow-redirect, trace/classify, fs). The test suite passes identically under
  a 0.6.11-pinned and a 0.7.0 resolution.
- `validator` stays at `0.20`, deliberately: `validator 0.21.0` declares
  `rust-version = "1.88"` — above this crate's 1.81 MSRV floor — and the
  `Validate` bound is version-coupled into the public API for
  `validator`-feature users (the route that made 2.0.0 a major). A 0.21 bump
  is therefore a candidate 3.0 driver plus an MSRV raise, deferred until a
  major is warranted; assessed 2026-08-08, recorded in `ROADMAP.md`'s
  dependency watch.
- `sqlx` stays at `0.8`, deliberately: `sqlx::Error` is version-coupled into
  the public API for `sqlx`-feature users (`impl From<sqlx::Error> for ApiError`),
  so a hard `0.9` bump is a semver major for them — and unlike tower-http, the
  widen instrument is also blocked, because `sqlx 0.9.0` declares
  `rust-version = "1.94.0"`, far above this crate's 1.81 MSRV floor: a widened
  range would hand fresh-resolving 1.81-1.93 consumers an unbuildable resolver
  match, the false-declared-MSRV failure 2.1.0 corrected. A 0.9 adoption is a
  candidate 3.0 driver, or rides a future MSRV raise to >=1.94; assessed
  2026-08-08, recorded in `ROADMAP.md`'s dependency watch.

## [2.1.0] - 2026-08-08

### Changed

- MSRV raised 1.75 -> 1.81, and now enforced by a dedicated `MSRV 1.81` CI job.
  This corrects a declaration that was already fiction: v2.0.0's
  `validator 0.18 -> 0.20` bump made 1.75 unbuildable with all features
  (`validator 0.20.0`, the earliest 0.20, hard-requires rustc 1.81), so no
  consumer on 1.75 could have built 2.0.0 anyway. Note for 1.81-1.84
  toolchains: pin `cargo update validator_derive --precise 0.20.0`
  (`validator_derive 0.20.1` is edition2024, which cargo < 1.85 cannot parse).

### Security

- Every GitHub Actions workflow now declares an explicit workflow-level
  `permissions: contents: read`, so no job's `GITHUB_TOKEN` scope is inherited
  from the repository-default settings toggle. This closes the last two
  unscoped surfaces: `ci.yml`'s `Test, Lint, Format` job (the only one of four
  without a block) and **`publish.yml`, the workflow that holds the crates.io
  publish path**, which declared no block at any level. Measured on main run
  30701775073: the unscoped job ran with `Contents: read, Metadata: read,
  Packages: read` while the scoped `Security audit` job ran with exactly
  `Contents: read, Metadata: read`. Pinned by `tests/workflow_permissions.rs`,
  which glob-discovers every workflow file and parses it with a real YAML
  parser, so a new workflow enters the contract the moment it is committed.
  No change to this crate's API, response bytes, or runtime dependencies
  (`serde_norway` is a dev-dependency of the guard only).

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

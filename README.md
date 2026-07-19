# axum-api-kit

Shared response types for Axum JSON APIs.

Every Axum CRUD service defines the same `ApiError`, `HealthResponse`, and paginated list types. This crate provides one canonical implementation.

## Stability

As of `1.0.0` the public API is stable and follows [semantic versioning](https://semver.org):
breaking changes will only ship in a new major version. Optional features (`validator`,
`sqlx`, `extract`, `trace`, `router`, `cors`, `openapi`, `problem`) track their upstream
crates and may update those bounds in a minor release.

See [ROADMAP.md](ROADMAP.md) for the full post-1.0 maintenance policy (MSRV, dependency tracking, 2.0 triggers).

## Installation

```toml
axum-api-kit = "1"
```

Optional integrations are gated behind feature flags:

```toml
# request extractors (Pagination, CursorPagination, ApiJson)
axum-api-kit = { version = "1", features = ["extract"] }

# JSON validation (ValidatedJson, From<ValidationErrors>)
axum-api-kit = { version = "1", features = ["validator"] }

# observability middleware (request id + tracing)
axum-api-kit = { version = "1", features = ["trace"] }

# health-probe router (/healthz, /readyz)
axum-api-kit = { version = "1", features = ["router"] }

# CORS layer helper (tower-http)
axum-api-kit = { version = "1", features = ["cors"] }

# OpenAPI schemas (utoipa ToSchema on the response types)
axum-api-kit = { version = "1", features = ["openapi"] }

# RFC 9457 problem+json responses (Problem)
axum-api-kit = { version = "1", features = ["problem"] }
```

## Types

### `ApiError`

A machine-readable JSON error body with `code`, `message`, and optional `details`.

```rust
use axum::response::IntoResponse;
use axum_api_kit::ApiError;

// Factory helpers return (StatusCode, Json<ApiError>) which implement IntoResponse
async fn handler() -> impl IntoResponse {
    ApiError::not_found("account not found")
}

// Use too_many_requests directly
async fn rate_limited() -> impl IntoResponse {
    use serde_json::json;
    ApiError::too_many_requests("slow down")
        // attach structured details to any factory result by rebuilding:
        // not needed here, but ApiError::new + with_details works too
}

// Or build manually for fully custom status codes
async fn custom() -> impl IntoResponse {
    use axum::{Json, http::StatusCode};
    use serde_json::json;
    let err = ApiError::new("PAYMENT_REQUIRED", "subscribe to continue")
        .with_details(json!({ "plan_url": "/pricing" }));
    (StatusCode::PAYMENT_REQUIRED, Json(err))
}

// ApiError implements Display and std::error::Error
async fn propagate() -> Result<(), ApiError> {
    Err(ApiError::new("GONE", "resource deleted"))
}

// Chain error sources with with_source()
async fn chained() -> impl IntoResponse {
    use serde_json::json;
    let err = ApiError::new("NOT_FOUND", "user not found")
        .with_source("SELECT * FROM users WHERE id = 42")
        .with_details(json!({ "user_id": 42 }));
    (axum::http::StatusCode::NOT_FOUND, axum::Json(err))
}

// Use ? operator in handlers with From<std::io::Error> and From<serde_json::Error>
async fn read_file() -> Result<impl IntoResponse, ApiError> {
    let content = std::fs::read_to_string("config.json")?;  // auto-converts io::Error
    let cfg: serde_json::Value = serde_json::from_str(&content)?;  // auto-converts JSON error
    Ok((axum::http::StatusCode::OK, content))
}
```

Available factory methods:

| Method | Status |
|---|---|
| `ApiError::bad_request(code, msg)` | 400 |
| `ApiError::unauthorized(msg)` | 401 |
| `ApiError::forbidden(msg)` | 403 |
| `ApiError::not_found(msg)` | 404 |
| `ApiError::conflict(msg)` | 409 |
| `ApiError::unprocessable_entity(msg)` | 422 |
| `ApiError::too_many_requests(msg)` | 429 |
| `ApiError::internal(msg)` | 500 |
| `ApiError::not_implemented(msg)` | 501 |
| `ApiError::db_error()` | 500 |
| `ApiError::service_unavailable(msg)` | 503 |
| `ApiError::too_many_requests_with_retry_after(msg, duration)` | 429 + `Retry-After` |
| `ApiError::service_unavailable_with_retry_after(msg, duration)` | 503 + `Retry-After` |

The `_with_retry_after` variants of `too_many_requests` and `service_unavailable` also emit
a delay-seconds `Retry-After` header (the duration is rounded up to whole seconds, so
1500ms becomes `"2"`) while keeping the same `{ "code", "message" }` JSON body:

```rust
use axum::response::IntoResponse;
use axum_api_kit::ApiError;
use std::time::Duration;

async fn rate_limited_with_header() -> impl IntoResponse {
    // 429 + Retry-After: 30 + {"code":"RATE_LIMITED","message":"slow down"}
    ApiError::too_many_requests_with_retry_after("slow down", Duration::from_secs(30))
}
```

### `ListResponse<T>`

Generic paginated collection response.

```rust
use axum::response::IntoResponse;
use axum_api_kit::ListResponse;
use serde::Serialize;

#[derive(Serialize)]
struct Item { id: String }

async fn list() -> impl IntoResponse {
    ListResponse {
        data: vec![Item { id: "abc".into() }],
        total: 1,
        limit: 50,
        offset: 0,
    }
}
```

### `HealthResponse`

| Constructor | `status` | HTTP |
|---|---|---|
| `HealthResponse::ok()` | `"ok"` | 200 |
| `HealthResponse::degraded()` | `"degraded"` | 200 |
| `HealthResponse::unhealthy()` | `"unhealthy"` | 503 |

```rust
use axum::response::IntoResponse;
use axum_api_kit::HealthResponse;

async fn health() -> impl IntoResponse {
    // Check dependencies and return the appropriate state
    if db_reachable() {
        HealthResponse::ok()
    } else if cache_reachable() {
        HealthResponse::degraded()  // 200, but something is wrong
    } else {
        HealthResponse::unhealthy() // 503
    }
}
```

### `CursorResponse<T>`

Cursor-based paginated response for large datasets or feeds. Use instead of `ListResponse` when:
- Total count is expensive to compute
- Data is streamed or unbounded
- You're building a feed (social media, notifications, etc.)
- You need bidirectional navigation via opaque tokens

| Field | Type | Meaning |
|---|---|---|
| `data` | `Vec<T>` | Items in this page |
| `next_cursor` | `Option<String>` | Token for next page; `None` = last page |
| `has_more` | `bool` | Convenience flag: true if more data exists |

```rust
use axum::response::IntoResponse;
use axum_api_kit::CursorResponse;
use serde::Serialize;

#[derive(Serialize)]
struct Post { id: String, content: String }

async fn feed(cursor: Option<String>) -> impl IntoResponse {
    let (posts, next_cursor) = fetch_posts(cursor, limit: 20);
    let has_more = next_cursor.is_some();
    
    CursorResponse {
        data: posts,
        next_cursor,
        has_more,
    }
}
```

### Success responses: `Created<T>`, `Accepted<T>`, `NoContent`

Helpers for the rest of the CRUD lifecycle, complementing `ListResponse` and `ApiError`.

| Type | Status | Body |
|---|---|---|
| `Created::new(resource)` | 201 | the resource as JSON |
| `Created::new(resource).with_location("/users/42")` | 201 | resource as JSON + `Location` header |
| `Accepted::new(job)` | 202 | the body as JSON |
| `NoContent` | 204 | empty |

```rust
use axum::response::IntoResponse;
use axum_api_kit::{Accepted, Created, NoContent};
use serde::Serialize;

#[derive(Serialize)]
struct User { id: String, name: String }

async fn create_user() -> impl IntoResponse {
    let user = User { id: "42".into(), name: "Ada".into() };
    Created::new(user).with_location("/users/42") // 201 + Location
}

async fn enqueue_export() -> impl IntoResponse {
    Accepted::new(serde_json::json!({ "job_id": "exp-1" })) // 202
}

async fn delete_user() -> impl IntoResponse {
    NoContent // 204
}
```

### `Problem` (RFC 9457)

With the `problem` feature (no new dependencies), `Problem` is an
[RFC 9457](https://www.rfc-editor.org/rfc/rfc9457) problem details response that emits
`Content-Type: application/problem+json`. Use it when the error format needs to
interoperate with gateways, OpenAPI tooling, or polyglot clients; `ApiError`'s flat shape
remains the default.

```rust
use axum::{http::StatusCode, response::IntoResponse};
use axum_api_kit::Problem;

async fn out_of_credit() -> impl IntoResponse {
    Problem::new(StatusCode::FORBIDDEN, "Insufficient credit")
        .with_type("https://example.com/probs/out-of-credit")
        .with_detail("Balance is 30, item costs 50")
        .with_instance("/account/12345/msgs/abc")
        .with_extension("balance", 30)
}
```

Serves HTTP 403 with `Content-Type: application/problem+json` and this body:

```json
{
    "type": "https://example.com/probs/out-of-credit",
    "title": "Insufficient credit",
    "status": 403,
    "detail": "Balance is 30, item costs 50",
    "instance": "/account/12345/msgs/abc",
    "balance": 30
}
```

An existing `ApiError` (or a factory tuple) bridges over losslessly in one line:

```rust
use axum::http::StatusCode;
use axum_api_kit::{ApiError, Problem};

// {"title":"Not Found","status":404,"detail":"account not found","code":"NOT_FOUND"}
let problem = Problem::from(ApiError::not_found("account not found"));

// or via the method form:
let problem = ApiError::new("NOT_FOUND", "account not found").into_problem(StatusCode::NOT_FOUND);
```

`with_retry_after(duration)` emits a delay-seconds `Retry-After` header; the delay is
header-only and never appears in the JSON body.

#### Accept-header content negotiation (opt-in)

`Problem`'s plain `IntoResponse` always serves `application/problem+json`; that never
changes. Handlers can opt in to serving the same body as plain `application/json` for
clients that explicitly prefer it: extract `ProblemFormat` (it reads the request's
`Accept` headers) and finish with `into_response_with`, or call
`into_response_for(&headers)` directly with a `HeaderMap`.

```rust
use axum::{http::StatusCode, response::Response};
use axum_api_kit::{Problem, ProblemFormat};

// Accept: application/json          -> Content-Type: application/json
// Accept: */* (or no Accept header) -> Content-Type: application/problem+json
async fn out_of_stock(format: ProblemFormat) -> Response {
    Problem::new(StatusCode::CONFLICT, "Out of stock").into_response_with(format)
}
```

Plain JSON is served only when the `Accept` header ranks `application/json` strictly
higher than `application/problem+json` (q-values considered; exact matches beat
`application/*`, which beats `*/*`). Every ambiguous case (no `Accept` header, `*/*`,
equal preference, unparseable values) keeps `application/problem+json`. The matcher is
deliberately minimal, not a full RFC 9110 implementation; the exact rules are documented
on `ProblemFormat::negotiate`.

#### problem+json extractor rejections (opt-in)

`ApiJson` and `ValidatedJson` rejection bodies never change. To get RFC 9457 rejections
instead, swap in the problem-flavored siblings: `ProblemJson<T>` (features `problem` +
`extract`) and `ProblemValidatedJson<T>` (features `problem` + `validator`). Same
deserialization, validation, and status codes; only the failure body format differs.
The format is chosen by naming the extractor in the handler signature, so enabling the
`problem` feature alone (for example through an unrelated dependency) changes nothing.

```rust
use axum_api_kit::ProblemValidatedJson;
use serde::Deserialize;
use validator::Validate;

#[derive(Deserialize, Validate)]
struct CreateAccount {
    #[validate(length(min = 2))]
    name: String,
}

async fn create_account(ProblemValidatedJson(account): ProblemValidatedJson<CreateAccount>) {
    let _ = account.name;
}
```

A validation failure rejects with HTTP 422 and this problem+json body, carrying the same
`code` and field-level details the flat `ApiError` rejection exposes (`message` maps to
`detail`, `code` and `details` become extension members):

```json
{
    "title": "Unprocessable Entity",
    "status": 422,
    "detail": "validation failed",
    "code": "VALIDATION_ERROR",
    "details": { "fields": { "name": [{ "code": "length", "params": { "min": 2, "value": "a" } }] } }
}
```

Rejections negotiate their `Content-Type` from the request's `Accept` headers exactly
like `ProblemFormat` above: `application/problem+json` in every ambiguous case, plain
`application/json` (byte-identical body) only when the client strictly prefers it. Each
rejection is a public `ProblemRejection { problem, format }`, inspectable in tests and
reusable as the rejection type of custom extractors.

## Error Propagation with `From` Implementations

Convert common Rust errors directly to `ApiError` (HTTP 500 Internal Error):

```rust
use std::io;
use axum_api_kit::ApiError;

async fn handler() -> Result<impl IntoResponse, ApiError> {
    // These use From<std::io::Error> and From<serde_json::Error>
    let json = std::fs::read_to_string("data.json")?;
    let value: serde_json::Value = serde_json::from_str(&json)?;
    Ok((StatusCode::OK, value.to_string()))
}
```

Supported conversions:
- `std::io::Error` - file I/O failures
- `serde_json::Error` - JSON parsing errors

## Optional Integrations

### Validator Integration (`validator` feature)

Enable the feature to convert `validator::ValidationErrors` directly into `ApiError`.

```rust
use axum::response::IntoResponse;
use axum_api_kit::ApiError;

#[cfg(feature = "validator")]
async fn create_user() -> Result<impl IntoResponse, ApiError> {
        // validate_user returns Result<(), validator::ValidationErrors>
        validate_user()?; // auto-converts into ApiError via From<ValidationErrors>
        Ok((axum::http::StatusCode::CREATED, "ok"))
}
```

The resulting `ApiError` uses this shape:

```json
{
    "code": "VALIDATION_ERROR",
    "message": "validation failed",
    "details": {
        "fields": {
            "email": [
                {
                    "code": "email",
                    "message": "invalid email",
                    "params": {
                        "value": "bad-input"
                    }
                }
            ]
        }
    }
}
```

### sqlx Integration (`sqlx` feature)

Enable the feature to convert `sqlx::Error` into semantically correct `ApiError` responses.

```toml
axum-api-kit = { version = "1", features = ["sqlx"] }
```

```rust
use axum::response::IntoResponse;
use axum_api_kit::ApiError;

async fn get_user(id: i64) -> Result<impl IntoResponse, ApiError> {
    let user = sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", id)
        .fetch_one(&pool)
        .await?; // RowNotFound -> 404, pool errors -> 503, etc.
    Ok(axum::Json(user))
}
```

| `sqlx::Error` variant | `code` | HTTP |
|---|---|---|
| `RowNotFound` | `NOT_FOUND` | 404 |
| `Database` (unique or FK violation) | `CONFLICT` | 409 |
| `Database` (check violation) | `VALIDATION_ERROR` | 422 |
| `Database` (other) | `DB_ERROR` | 500 |
| `PoolTimedOut` / `PoolClosed` / `WorkerCrashed` | `SERVICE_UNAVAILABLE` | 503 |
| everything else | `DB_ERROR` | 500 |

### Validated JSON extractor (`validator` feature)

`ValidatedJson<T>` deserializes a JSON body and runs `validator` validation before your
handler runs, rejecting with an `ApiError` body when either step fails.

```rust
use axum_api_kit::ValidatedJson;
use serde::Deserialize;
use validator::Validate;

#[derive(Deserialize, Validate)]
struct CreateUser {
    #[validate(length(min = 1, max = 100))]
    name: String,
    #[validate(email)]
    email: String,
}

async fn create_user(ValidatedJson(user): ValidatedJson<CreateUser>) {
    // `user` is deserialized and validated.
}
```

| Failure | HTTP | `code` |
|---|---|---|
| malformed JSON | 400 | `INVALID_JSON` |
| well-formed JSON of the wrong shape | 422 | `INVALID_BODY` |
| missing or incorrect `Content-Type` | 415 | `UNSUPPORTED_MEDIA_TYPE` |
| validation failure | 422 | `VALIDATION_ERROR` (with field-level `details`) |

### Pagination extractors (`extract` feature)

`Pagination` and `CursorPagination` parse query parameters into typed values and provide
helpers that build the matching response type. `limit` defaults to `50` and is clamped to
`1..=100` (`Pagination::DEFAULT_LIMIT` / `Pagination::MAX_LIMIT`); a non-numeric value
rejects with `400 Bad Request` (`INVALID_QUERY`).

```rust
use axum_api_kit::{CursorPagination, CursorResponse, ListResponse, Pagination};
use serde::Serialize;

#[derive(Serialize)]
struct Item { id: u64 }

// GET /items?limit=25&offset=50
async fn list(page: Pagination) -> ListResponse<Item> {
    let items = vec![Item { id: 1 }];
    page.list_response(items, 1) // -> { data, total, limit, offset }
}

// GET /feed?cursor=abc123&limit=25
async fn feed(page: CursorPagination) -> CursorResponse<Item> {
    let items = vec![Item { id: 1 }];
    page.cursor_response(items, Some("next".into())) // has_more = next_cursor.is_some()
}
```

### JSON extractor (`extract` feature)

`ApiJson<T>` is a drop-in replacement for `axum::Json` that rejects with an `ApiError` body
instead of Axum's default plain-text response. Use it when you want consistent error bodies
but don't need `validator` validation (otherwise reach for `ValidatedJson`).

| Failure | Status | `code` |
|---|---|---|
| malformed JSON | 400 | `INVALID_JSON` |
| well-formed JSON, wrong shape | 422 | `INVALID_BODY` |
| missing/incorrect `Content-Type` | 415 | `UNSUPPORTED_MEDIA_TYPE` |

```rust
use axum_api_kit::ApiJson;
use serde::Deserialize;

#[derive(Deserialize)]
struct CreateUser { name: String }

// Bad input becomes a JSON ApiError before the handler runs.
async fn create_user(ApiJson(user): ApiJson<CreateUser>) {
    let _ = user.name;
}
```

It also implements `IntoResponse`, so it can be returned from handlers like `axum::Json`.

### Observability middleware (`trace` feature)

Two `axum::middleware::from_fn` middlewares for request correlation and structured logging:

- `propagate_request_id` reuses an incoming `x-request-id` header (or mints a UUID v4),
  stores it in request extensions (extractable via `RequestId`), and echoes it on the
  response.
- `trace_requests` emits an `info`-level `tracing` event per request with `method`, `path`,
  `status`, `latency_ms`, and `request_id`. It is a no-op without a `tracing` subscriber.

```rust
use axum::{middleware, routing::get, Router};
use axum_api_kit::{propagate_request_id, trace_requests, RequestId};

async fn handler(RequestId(id): RequestId) -> String {
    format!("request {id}")
}

// The last `.layer` is the outermost, so request ids are assigned before
// trace_requests records its event.
let app: Router = Router::new()
    .route("/", get(handler))
    .layer(middleware::from_fn(trace_requests))
    .layer(middleware::from_fn(propagate_request_id));
```

### Health-probe router (`router` feature)

`health_routes` returns a `Router` with `/healthz` (liveness, always `ok`) and `/readyz`
(readiness, runs your async check). It is generic over router state, so it merges into a
stateful app. Capture whatever the readiness check needs in the closure.

```rust
use axum::Router;
use axum_api_kit::{health_routes, HealthResponse};

let app: Router = Router::new().merge(health_routes(|| async {
    // probe dependencies; return ok / degraded / unhealthy
    HealthResponse::ok()
}));
```

`/readyz` returns the status code of the `HealthResponse` it produces, so `unhealthy()`
yields `503`.

### CORS helper (`cors` feature)

`cors_allowing` builds a `tower_http` `CorsLayer` for a known origin allow-list (common REST
methods, `content-type`/`authorization` headers, credentials enabled). `cors_permissive()`
is available for local development.

```rust
use axum::{routing::get, Router};
use axum_api_kit::cors_allowing;

let app: Router = Router::new()
    .route("/", get(|| async { "ok" }))
    .layer(cors_allowing(["https://app.example.com"]));
```

### OpenAPI schemas (`openapi` feature)

With the `openapi` feature, `ApiError`, `ListResponse<T>`, `CursorResponse<T>`, and
`HealthResponse` derive [`utoipa::ToSchema`](https://docs.rs/utoipa), so you can reference
them from your `OpenApi` document and they show up in the generated spec.

```rust
use axum_api_kit::{ApiError, CursorResponse, HealthResponse, ListResponse};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(components(schemas(
    ApiError,
    HealthResponse,
    ListResponse<String>,
    CursorResponse<String>,
)))]
struct ApiDoc;

let spec = ApiDoc::openapi().to_json().unwrap();
```

## License

MIT

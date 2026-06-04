# axum-api-kit

Shared response types for Axum JSON APIs.

Every Axum CRUD service defines the same `ApiError`, `HealthResponse`, and paginated list types. This crate provides one canonical implementation.

## Installation

```toml
axum-api-kit = "0.7"
```

Optional integrations are gated behind feature flags:

```toml
# request extractors (Pagination, CursorPagination)
axum-api-kit = { version = "0.8", features = ["extract"] }

# JSON validation (ValidatedJson, From<ValidationErrors>)
axum-api-kit = { version = "0.8", features = ["validator"] }

# observability middleware (request id + tracing)
axum-api-kit = { version = "0.8", features = ["trace"] }
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
| `ApiError::unprocessable(msg)` | 422 |
| `ApiError::too_many_requests(msg)` | 429 |
| `ApiError::internal(msg)` | 500 |
| `ApiError::not_implemented(msg)` | 501 |
| `ApiError::db_error()` | 500 |
| `ApiError::service_unavailable(msg)` | 503 |

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
axum-api-kit = { version = "0.8", features = ["sqlx"] }
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

## License

MIT

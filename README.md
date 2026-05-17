# axum-api-kit

Shared response types for Axum JSON APIs.

Every Axum CRUD service defines the same `ApiError`, `HealthResponse`, and paginated list types. This crate provides one canonical implementation.

## Installation

```toml
axum-api-kit = "0.3"
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
    ApiError::not_found("user")
        .with_source("SELECT * FROM users WHERE id = 42")
        .with_details(json!({ "user_id": 42 }))
}

// Use ? operator in handlers with From<std::io::Error> and From<serde_json::Error>
async fn read_file() -> Result<impl IntoResponse, ApiError> {
    let content = std::fs::read_to_string("config.json")?;  // auto-converts io::Error
    let cfg: serde_json::Value = serde_json::from_str(&content)?;  // auto-converts JSON error
    Ok((axum::http::StatusCode::OK, content))
}
```
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

## License

MIT

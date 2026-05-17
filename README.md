# axum-api-kit

Shared response types for Axum JSON APIs.

Every Axum CRUD service defines the same `ApiError`, `HealthResponse`, and paginated list types. This crate provides one canonical implementation.

## Installation

```toml
axum-api-kit = "0.2"
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

## License

MIT

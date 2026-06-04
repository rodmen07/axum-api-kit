#![cfg(feature = "openapi")]

//! Verifies the response types register as OpenAPI components under the `openapi` feature.

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

#[test]
fn response_types_register_as_schemas() {
    let json = ApiDoc::openapi()
        .to_json()
        .expect("openapi serializes to json");

    // Component names.
    assert!(json.contains("ApiError"), "ApiError schema missing");
    assert!(
        json.contains("HealthResponse"),
        "HealthResponse schema missing"
    );

    // A few representative field names from the generated schemas.
    assert!(json.contains("\"code\""), "ApiError.code missing");
    assert!(
        json.contains("\"has_more\""),
        "CursorResponse.has_more missing"
    );
    assert!(json.contains("\"total\""), "ListResponse.total missing");
}

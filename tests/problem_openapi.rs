#![cfg(all(feature = "problem", feature = "openapi"))]

//! Verifies `Problem` registers as an OpenAPI component under the `openapi`
//! feature and keeps the header-only `retry_after` field out of the schema.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(components(schemas(axum_api_kit::Problem)))]
struct ApiDoc;

#[test]
fn problem_registers_as_schema_without_retry_after() {
    let json = ApiDoc::openapi()
        .to_json()
        .expect("openapi serializes to json");

    assert!(json.contains("Problem"), "Problem schema missing");
    assert!(json.contains("\"title\""), "Problem.title missing");
    assert!(json.contains("\"status\""), "Problem.status missing");
    assert!(
        !json.contains("retry_after"),
        "header-only retry_after leaked into the schema"
    );
}

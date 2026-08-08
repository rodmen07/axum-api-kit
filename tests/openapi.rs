#![cfg(feature = "openapi")]

//! Verifies the response types register as OpenAPI components under the `openapi` feature.
//!
//! These assertions navigate the PARSED document (`components.schemas.<name>.properties` /
//! `.required`) rather than substring-matching the serialized text, because a `contains` over the
//! whole document cannot say WHICH schema a property belongs to. The predecessor of this file
//! asserted `json.contains("\"has_more\"")` and `json.contains("\"total\"")` against the serialized
//! string; renaming `CursorResponse::has_more` to `hasMore` while renaming `ListResponse::limit` to
//! `has_more` left it fully green (`1 passed`) with both pagination types carrying a wrong wire
//! shape, while these tests fail four times on that same tree and name the drifted lists.
//!
//! One weakness it turned out NOT to have, recorded because it is the plausible-sounding claim that
//! motivated this rewrite and a control refuted it: `utoipa` does copy each type's rustdoc into the
//! schema `description`, and every one of these types documents its own JSON shape there — but the
//! description is JSON-escaped in the serialized output (`\"has_more\"`), so it never satisfied a
//! `contains("\"has_more\"")` looking for unescaped quotes. The old assertions were narrow, not
//! self-satisfying: they checked 3 of the 11 properties across the 4 schemas, never checked
//! `required`, and never checked that a skipped field stays out.

use serde_json::Value;
use utoipa::OpenApi;

use axum_api_kit::{ApiError, CursorResponse, HealthResponse, ListResponse};

#[derive(OpenApi)]
#[openapi(components(schemas(
    ApiError,
    HealthResponse,
    ListResponse<String>,
    CursorResponse<String>,
)))]
struct ApiDoc;

fn document() -> Value {
    let json = ApiDoc::openapi()
        .to_json()
        .expect("openapi serializes to json");
    serde_json::from_str(&json).expect("the generated document is valid json")
}

/// The schema object for `name`, failing loudly (rather than vacuously passing) if it is absent.
fn schema(doc: &Value, name: &str) -> Value {
    doc.get("components")
        .and_then(|c| c.get("schemas"))
        .and_then(|s| s.get(name))
        .unwrap_or_else(|| {
            panic!(
                "no `components.schemas.{name}` in the generated document; registered schemas: {:?}",
                doc.get("components")
                    .and_then(|c| c.get("schemas"))
                    .and_then(|s| s.as_object())
                    .map(|m| m.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default()
            )
        })
        .clone()
}

/// The property names a schema actually declares, sorted.
fn property_names(doc: &Value, name: &str) -> Vec<String> {
    let s = schema(doc, name);
    let props = s
        .get("properties")
        .and_then(|p| p.as_object())
        .unwrap_or_else(|| panic!("`{name}` declares no `properties` object at all"));
    assert!(
        !props.is_empty(),
        "`{name}` declares an empty `properties` object, so every field assertion below would be \
         satisfied only by its description text"
    );
    let mut names: Vec<String> = props.keys().cloned().collect();
    names.sort();
    names
}

/// The `required` field names a schema declares, sorted.
fn required_names(doc: &Value, name: &str) -> Vec<String> {
    let s = schema(doc, name);
    let mut names: Vec<String> = s
        .get("required")
        .and_then(|r| r.as_array())
        .unwrap_or_else(|| panic!("`{name}` declares no `required` array"))
        .iter()
        .map(|v| v.as_str().expect("required entries are strings").to_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn all_four_response_types_register_as_schemas() {
    let doc = document();
    for name in [
        "ApiError",
        "HealthResponse",
        "ListResponse_String",
        "CursorResponse_String",
    ] {
        // `schema` panics with the registered-name list if this is missing.
        let _ = schema(&doc, name);
    }
}

#[test]
fn api_error_declares_its_three_fields() {
    let doc = document();
    assert_eq!(
        property_names(&doc, "ApiError"),
        ["code", "details", "message"]
    );
}

#[test]
fn api_error_details_is_optional() {
    let doc = document();
    assert_eq!(required_names(&doc, "ApiError"), ["code", "message"]);
}

#[test]
fn health_response_declares_only_the_status_field() {
    let doc = document();
    assert_eq!(property_names(&doc, "HealthResponse"), ["status"]);
}

/// `HealthResponse::status_code` is `#[serde(skip)]` and drives the HTTP status, never the body.
/// The schema must not advertise it — a leak here would also mean `utoipa` had stopped honouring
/// serde's `skip`, which is what keeps the non-serializable `StatusCode` out of the document.
#[test]
fn health_response_does_not_advertise_the_private_status_code_field() {
    let doc = document();
    assert!(
        !property_names(&doc, "HealthResponse").contains(&"status_code".to_owned()),
        "the skipped `status_code` field leaked into the schema"
    );
}

#[test]
fn list_response_declares_its_four_fields() {
    let doc = document();
    assert_eq!(
        property_names(&doc, "ListResponse_String"),
        ["data", "limit", "offset", "total"]
    );
}

#[test]
fn list_response_requires_every_field() {
    let doc = document();
    assert_eq!(
        required_names(&doc, "ListResponse_String"),
        ["data", "limit", "offset", "total"]
    );
}

#[test]
fn cursor_response_declares_its_three_fields() {
    let doc = document();
    assert_eq!(
        property_names(&doc, "CursorResponse_String"),
        ["data", "has_more", "next_cursor"]
    );
}

/// `next_cursor` is absent on the last page, so it must not be `required`.
#[test]
fn cursor_response_does_not_require_next_cursor() {
    let doc = document();
    assert_eq!(
        required_names(&doc, "CursorResponse_String"),
        ["data", "has_more"]
    );
}

/// KNOWN GAP — pins today's WRONG behaviour so the defect has a mechanical existence.
///
/// The generated schema types `next_cursor` as `["string", "null"]`, because `utoipa` derives
/// nullability from `Option<String>`. The wire never emits `null` for it: the field carries
/// `#[serde(skip_serializing_if = "Option::is_none")]`, so on the last page the key is simply
/// absent (locked by `tests/default_response_contracts.rs`). A client generated from this document
/// therefore gets a `string | null` union whose `null` arm is unreachable.
///
/// Not fixed here: suppressing the `null` arm changes the document this feature emits for existing
/// `openapi` consumers, which is a decision about the published surface rather than a test fix.
/// See the `## Bugs` entry in `backlogs/axum-api-kit.md`. **Fixing the bug MUST turn this test
/// red**, and that red is the signal to close the entry.
#[test]
fn known_gap_cursor_next_cursor_is_typed_nullable_though_the_wire_omits_it() {
    let doc = document();
    let ty = schema(&doc, "CursorResponse_String")
        .get("properties")
        .and_then(|p| p.get("next_cursor"))
        .and_then(|f| f.get("type"))
        .cloned()
        .expect("next_cursor declares a type");
    assert_eq!(
        ty,
        serde_json::json!(["string", "null"]),
        "GAP CLOSED: `next_cursor` is no longer typed as nullable. The wire omits the key rather \
         than sending null, so this is the fix the known-gap entry was waiting for — close the \
         `## Bugs` entry in backlogs/axum-api-kit.md and delete this test."
    );
}

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
//!
//! Since 2026-08-08 the file also carries a CLASS guard, `no_registered_property_admits_null`
//! (its `problem`-feature sibling lives in `tests/problem_openapi.rs` beside the rest of the
//! `Problem` schema contract): rather than naming fields, it walks every property the document
//! declares and fails if any of them admits `null`. Every `Option` field in this crate carries
//! `#[serde(skip_serializing_if = "Option::is_none")]`, so absence is a missing key, not a
//! `null` — but `utoipa` derives nullability from `Option<T>` and reintroduces the union unless
//! the field opts out. Five fields across three types were shipping that way.
//!
//! The parsed-document helpers live in `tests/openapi_support/mod.rs` (moved there verbatim on
//! 2026-08-09) so `tests/problem_openapi.rs` calls the same walker instead of re-deriving it.

mod openapi_support;

use openapi_support::{
    every_property, null_paths, property_names, property_type, required_names, schema,
};
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

/// `next_cursor` is omitted from the body on the last page, never sent as `null`, so a client
/// generated from this document must not get a `string | null` union whose `null` arm is
/// unreachable. Absence is carried by staying out of `required`, asserted just above.
///
/// This replaces a KNOWN-GAP test that pinned the opposite (wrong) value; the wire side is byte
/// locked by `tests/default_response_contracts.rs::cursor_response_body_bytes_are_locked_on_the_last_page`.
#[test]
fn cursor_response_next_cursor_is_typed_string_not_a_nullable_union() {
    let doc = document();
    assert_eq!(
        property_type(&doc, "CursorResponse_String", "next_cursor"),
        serde_json::json!("string")
    );
}

/// `ApiError::details` is the same shape as `next_cursor` — `Option<T>` plus
/// `skip_serializing_if`, so the key is absent rather than `null`
/// (`src/error.rs`'s `serializes_without_details` locks that wire).
/// It reaches the document through `schema(value_type = Option<Object>)`, which does NOT by
/// itself suppress the `null` arm.
#[test]
fn api_error_details_is_typed_object_not_a_nullable_union() {
    let doc = document();
    assert_eq!(
        property_type(&doc, "ApiError", "details"),
        serde_json::json!("object")
    );
}

/// CLASS GUARD. No property of any schema this crate registers may admit `null`, because every
/// `Option` field in this crate carries a `#[serde(skip_serializing_if = ...)]` that omits it when
/// absent (`Option::is_none` for the fields whose inner type cannot BE null; `details_is_absent`
/// for `ApiError::details`, whose `Value` can, so `None` and `Some(Value::Null)` are both skipped):
/// absence is expressed by omitting the key and by staying out of `required`, never by sending
/// `null`. `utoipa` derives nullability from `Option<T>` and so reintroduces the union by default,
/// which is why each such field must opt out with `schema(nullable = false)`.
///
/// This is a whole-set equality over properties DISCOVERED in the document, not a hand-written
/// list of field names, so a newly added `Option` field is covered the day it lands. If a future
/// field genuinely serializes as `null` — i.e. it does NOT carry `skip_serializing_if` — this test
/// is the place that decision gets made explicitly, by allowing it here with a reason.
#[test]
fn no_registered_property_admits_null() {
    let doc = document();
    let properties = every_property(&doc);

    let schemas_with_properties: std::collections::BTreeSet<&str> =
        properties.iter().map(|(s, _, _)| s.as_str()).collect();
    assert_eq!(
        schemas_with_properties.into_iter().collect::<Vec<_>>(),
        [
            "ApiError",
            "CursorResponse_String",
            "HealthResponse",
            "ListResponse_String"
        ],
        "vacuity guard: the walker did not reach every registered schema, so an empty result \
         below would prove nothing"
    );
    assert_eq!(
        properties.len(),
        11,
        "vacuity guard: expected the 11 properties of the four core schemas, walked {:?}",
        properties
            .iter()
            .map(|(s, p, _)| format!("{s}.{p}"))
            .collect::<Vec<_>>()
    );

    let offenders: Vec<String> = properties
        .iter()
        .flat_map(|(s, p, body)| null_paths(body, &format!("{s}.{p}")))
        .collect();
    assert_eq!(
        offenders,
        Vec::<String>::new(),
        "these properties admit `null` in the generated document while the wire omits the key \
         instead; each needs `#[cfg_attr(feature = \"openapi\", schema(nullable = false))]`"
    );
}

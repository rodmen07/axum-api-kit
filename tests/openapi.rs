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
//! (and its `problem`-feature sibling): rather than naming fields, it walks every property the
//! document declares and fails if any of them admits `null`. Every `Option` field in this crate
//! carries `#[serde(skip_serializing_if = "Option::is_none")]`, so absence is a missing key, not
//! a `null` — but `utoipa` derives nullability from `Option<T>` and reintroduces the union unless
//! the field opts out. Five fields across three types were shipping that way.

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

/// The property's declared type, as the document states it.
fn property_type(doc: &Value, schema_name: &str, property: &str) -> Value {
    schema(doc, schema_name)
        .get("properties")
        .and_then(|p| p.get(property))
        .and_then(|f| f.get("type"))
        .cloned()
        .unwrap_or_else(|| panic!("`{schema_name}.{property}` declares no `type` at all"))
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

/// Every `(schema, property, body)` triple the document declares, descending through the `allOf`
/// branches that `#[serde(flatten)]` produces. That descent is load-bearing rather than defensive:
/// `Problem` carries a flattened `extensions` map, so its own fields live at
/// `components.schemas.Problem.allOf[1].properties` and a lookup at
/// `components.schemas.Problem.properties` finds **nothing at all**.
fn every_property(doc: &Value) -> Vec<(String, String, Value)> {
    fn collect(schema_name: &str, node: &Value, out: &mut Vec<(String, String, Value)>) {
        if let Some(props) = node.get("properties").and_then(|p| p.as_object()) {
            for (name, body) in props {
                out.push((schema_name.to_owned(), name.clone(), body.clone()));
            }
        }
        for combinator in ["allOf", "oneOf", "anyOf"] {
            if let Some(branches) = node.get(combinator).and_then(|b| b.as_array()) {
                for branch in branches {
                    collect(schema_name, branch, out);
                }
            }
        }
    }

    let schemas = doc
        .get("components")
        .and_then(|c| c.get("schemas"))
        .and_then(|s| s.as_object())
        .expect("the document declares components.schemas");
    let mut out = Vec::new();
    for (name, body) in schemas {
        collect(name, body, &mut out);
    }
    out
}

/// The sub-paths within one property body that admit `null`, as either an OpenAPI 3.1 type union
/// (`"type": ["string", "null"]`) or a 3.0-style `"nullable": true`. Recurses into `items` so an
/// element type is caught too.
fn null_paths(body: &Value, prefix: &str) -> Vec<String> {
    let mut found = Vec::new();
    let declares_null = match body.get("type") {
        Some(Value::Array(members)) => members.iter().any(|m| m == "null"),
        Some(Value::String(s)) => s == "null",
        _ => false,
    } || body.get("nullable") == Some(&Value::Bool(true));
    if declares_null {
        found.push(prefix.to_owned());
    }
    if let Some(items) = body.get("items") {
        found.extend(null_paths(items, &format!("{prefix}.items")));
    }
    found
}

/// CLASS GUARD. No property of any schema this crate registers may admit `null`, because every
/// `Option` field in this crate carries `#[serde(skip_serializing_if = "Option::is_none")]`:
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

/// The same class guard over the `problem` feature's `Problem` schema, which is registered
/// separately because it is behind its own feature flag. It lives in this file rather than in
/// `tests/problem_openapi.rs` because the parsed-document helpers above are what make the
/// assertion possible, and because that file is pending its own rewrite (see the `## Bugs` entry
/// in `backlogs/axum-api-kit.md`); duplicating the machinery would collide with it.
#[cfg(feature = "problem")]
#[test]
fn no_problem_property_admits_null() {
    #[derive(OpenApi)]
    #[openapi(components(schemas(axum_api_kit::Problem)))]
    struct ProblemDoc;

    let doc: Value = serde_json::from_str(
        &ProblemDoc::openapi()
            .to_json()
            .expect("openapi serializes to json"),
    )
    .expect("the generated document is valid json");

    let properties = every_property(&doc);
    let names: Vec<String> = properties
        .iter()
        .map(|(s, p, _)| format!("{s}.{p}"))
        .collect();
    assert_eq!(
        names,
        [
            "Problem.detail",
            "Problem.instance",
            "Problem.status",
            "Problem.title",
            "Problem.type",
        ],
        "vacuity guard: `Problem`'s fields sit under `allOf` because `extensions` is flattened, \
         so a walker that missed the descent would report an empty set and pass for free. \
         `retry_after` is `schema(ignore)` and must stay out."
    );

    let offenders: Vec<String> = properties
        .iter()
        .flat_map(|(s, p, body)| null_paths(body, &format!("{s}.{p}")))
        .collect();
    assert_eq!(
        offenders,
        Vec::<String>::new(),
        "these `Problem` properties admit `null` while the wire omits the key instead"
    );
}

#![cfg(all(feature = "problem", feature = "openapi"))]

//! The `Problem` schema contract under the `openapi` feature: registration, the exact property
//! set, the exact `required` set, the header-only `retry_after` staying out, and (the class
//! guard moved here from `tests/openapi.rs`) no property admitting `null`.
//!
//! Every assertion navigates the PARSED document. The predecessor of this file asserted
//! `json.contains("Problem")`, `json.contains("\"title\"")`, `json.contains("\"status\"")` and
//! `!json.contains("retry_after")` over the serialized text; a `contains` over the whole
//! document cannot attribute a property to a schema (the weakness `tests/openapi.rs` records for
//! its own predecessor, where two simultaneous wrong wire shapes left it fully green), and its
//! load-bearing NEGATIVE clause was one copy-edit away from a false red: `utoipa` copies
//! `Problem`'s struct-level rustdoc into the schema `description`, so the moment that prose
//! spells `retry_after` the way the field does, the substring assertion fails with the schema
//! still correct. Here the negative is held by parsed property names, which prose cannot reach.
//!
//! `Problem` is NOT a flat schema: `#[serde(flatten)] extensions` makes the generated schema an
//! `allOf` composition, so its own fields live at `components.schemas.Problem.allOf[1].properties`
//! (and `required` likewise) while `components.schemas.Problem.properties` does not exist at all.
//! Every lookup therefore goes through the shared walkers in `tests/openapi_support/mod.rs`,
//! which descend the combinators and fail loudly — never vacuously — when the schema, the walked
//! property set, or the `required` carrier is absent.

mod openapi_support;

use openapi_support::{every_property, null_paths, required_names_deep, schema};
use serde_json::Value;
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(components(schemas(axum_api_kit::Problem)))]
struct ApiDoc;

fn document() -> Value {
    let json = ApiDoc::openapi()
        .to_json()
        .expect("openapi serializes to json");
    serde_json::from_str(&json).expect("the generated document is valid json")
}

/// The property names the `Problem` schema declares across its `allOf` branches, sorted.
fn problem_property_names(doc: &Value) -> Vec<String> {
    let mut names: Vec<String> = every_property(doc)
        .into_iter()
        .filter(|(schema_name, _, _)| schema_name == "Problem")
        .map(|(_, property, _)| property)
        .collect();
    names.sort();
    names
}

#[test]
fn problem_registers_as_a_schema() {
    // `schema` panics with the registered-name list if `Problem` is missing, so an empty or
    // wrongly-keyed components section cannot pass silently.
    let _ = schema(&document(), "Problem");
}

/// The whole property set, as an equality rather than per-name `contains` checks: a property
/// appearing under the wrong schema, a field leaking in, or a field silently dropped all fail
/// here by name. `extensions` itself must NOT appear — it is flattened into the composition's
/// free-form branch, not declared as a property.
#[test]
fn problem_declares_exactly_its_five_wire_fields() {
    assert_eq!(
        problem_property_names(&document()),
        ["detail", "instance", "status", "title", "type"],
        "the `Problem` schema's declared property set drifted from the five RFC 9457 wire fields"
    );
}

/// RFC 9457 requires nothing, but this crate's contract is that `title` and `status` are always
/// present on the wire (both are non-`Option` fields), and the schema must say exactly that:
/// a consumer generating a client from this document gets non-optional accessors for these two
/// and optional ones for the rest. `required` lives inside the `allOf` branch, where a top-level
/// lookup finds nothing.
#[test]
fn problem_requires_exactly_title_and_status() {
    assert_eq!(
        required_names_deep(&document(), "Problem"),
        ["status", "title"],
        "the `Problem` schema's `required` set drifted; `title` and `status` are the only fields \
         every response carries"
    );
}

/// `retry_after` is `#[serde(skip)]` + `schema(ignore)`: it drives the `Retry-After` HTTP header
/// and never appears in the body, so it must not appear in the schema either. Asserted on parsed
/// property names — the schema `description` may mention the field name freely (rustdoc is copied
/// there verbatim), which the predecessor's substring form could not tolerate.
#[test]
fn problem_does_not_advertise_the_header_only_retry_after_field() {
    let doc = document();
    // Anchor on the schema's PRESENCE first (loud panic when absent): a negative assertion over
    // an absent schema's empty property list would otherwise pass for free — the exact vacuity
    // hazard the sibling suite's NC-5 control demonstrated on an unregistered `HealthResponse`.
    let _ = schema(&doc, "Problem");
    let names = problem_property_names(&doc);
    assert!(
        !names.is_empty(),
        "vacuity guard: `Problem` is registered but the walker returned no properties at all"
    );
    assert!(
        !names.contains(&"retry_after".to_owned()),
        "header-only `retry_after` leaked into the schema; declared properties: {names:?}"
    );
}

/// CLASS GUARD, moved verbatim from `tests/openapi.rs` on 2026-08-09 (it lived there only while
/// this file was pending its rewrite, because the walker it needs was defined there). No
/// `Problem` property may admit `null`: every `Option` field carries
/// `#[serde(skip_serializing_if = "Option::is_none")]`, so absence is an omitted key and staying
/// out of `required`, never a `null` — and `utoipa` reintroduces the union from `Option<T>`
/// unless the field opts out with `schema(nullable = false)`.
#[test]
fn no_problem_property_admits_null() {
    let doc = document();
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

#![cfg(all(feature = "openapi", feature = "extract"))]

//! Verifies the query parameters the `extract` extractors ENFORCE are the ones the `openapi`
//! feature GENERATES, under the joint `openapi` + `extract` gate.
//!
//! `Pagination` and `CursorPagination` derive `utoipa::IntoParams`, so a consumer writing
//! `#[utoipa::path(..., params(Pagination))]` gets `limit`/`offset` (and `cursor`/`limit`)
//! described in their spec instead of documenting them by hand or shipping a spec that omits
//! them entirely.
//!
//! Two properties of these assertions are load-bearing, both learned here already:
//!
//! 1. **They navigate the PARSED document** (`paths.<path>.<method>.parameters`), never
//!    `contains()` over the serialized text. A substring match cannot say which operation a
//!    parameter belongs to, and — as `tests/problem_openapi.rs`'s predecessor proved — a
//!    negative substring assertion passes for free when the thing it looks in is absent.
//! 2. **Every lookup is loud on absence.** `operation_parameters` panics on a missing path,
//!    a missing `parameters` array, and an EMPTY one, so an operation that stopped contributing
//!    parameters fails these tests instead of satisfying them vacuously.
//!
//! The bounds tests are drift guards in the `tests/roadmap_truth_tests.rs` sense: they read the
//! generated document on one side and `Pagination::DEFAULT_LIMIT` / `Pagination::MAX_LIMIT` on
//! the other. That is only possible because `src/pagination.rs` spells the bounds as numeric
//! literals — utoipa's `minimum`/`maximum` parse a literal token and reject a constant path, and
//! naming the constants there (where the macro allows it, as for `default`) would make this
//! suite compare a value against itself and report agreement no matter what either side said.

mod openapi_support;

use openapi_support::{null_paths, parameter, parameter_names};
use serde_json::Value;
use utoipa::OpenApi;

use axum_api_kit::{CursorPagination, Pagination};

#[utoipa::path(
    get,
    path = "/items",
    params(Pagination),
    responses((status = 200, description = "a page of items"))
)]
#[allow(dead_code)]
fn list_items() {}

#[utoipa::path(
    get,
    path = "/feed",
    params(CursorPagination),
    responses((status = 200, description = "a cursor page of items"))
)]
#[allow(dead_code)]
fn feed_items() {}

#[derive(OpenApi)]
#[openapi(paths(list_items, feed_items))]
struct ApiDoc;

fn document() -> Value {
    let json = ApiDoc::openapi()
        .to_json()
        .expect("openapi serializes to json");
    serde_json::from_str(&json).expect("the generated document is valid json")
}

/// Both operations' parameters as `(operation, parameter)` pairs. Used by the class-wide tests so
/// a parameter added to either extractor is covered the day it lands rather than when someone
/// remembers to extend a hand-written list.
fn every_contributed_parameter(doc: &Value) -> Vec<(&'static str, Value)> {
    let mut all = Vec::new();
    for (path, names) in [
        ("/items", ["limit", "offset"]),
        ("/feed", ["cursor", "limit"]),
    ] {
        for name in names {
            all.push((path, parameter(doc, path, "get", name)));
        }
    }
    all
}

/// A numeric JSON Schema keyword of a parameter, failing loudly when it is absent rather than
/// letting a missing bound read as a satisfied one.
fn keyword(param: &Value, keyword: &str) -> f64 {
    param
        .get("schema")
        .and_then(|s| s.get(keyword))
        .and_then(|v| v.as_f64())
        .unwrap_or_else(|| {
            panic!(
                "parameter `{}` declares no numeric `{keyword}`; its schema is {}",
                param.get("name").and_then(|n| n.as_str()).unwrap_or("?"),
                param.get("schema").cloned().unwrap_or(Value::Null)
            )
        })
}

#[test]
fn pagination_contributes_exactly_limit_and_offset() {
    let doc = document();
    assert_eq!(parameter_names(&doc, "/items", "get"), ["limit", "offset"]);
}

#[test]
fn cursor_pagination_contributes_exactly_cursor_and_limit() {
    let doc = document();
    assert_eq!(parameter_names(&doc, "/feed", "get"), ["cursor", "limit"]);
}

/// `IntoParams` has NO default location: `into_params` falls back to `ParameterIn::default()`,
/// which is `Path`. Without `#[into_params(parameter_in = Query)]` every one of these would be
/// documented as a path parameter of a path that does not contain it — a spec a client generator
/// would build unusable code from, with nothing else in this suite noticing.
#[test]
fn every_contributed_parameter_is_a_query_parameter() {
    let doc = document();
    let all = every_contributed_parameter(&doc);
    assert_eq!(
        all.len(),
        4,
        "vacuity guard: expected 4 parameters, walked {all:?}"
    );

    let misplaced: Vec<String> = all
        .iter()
        .filter(|(_, p)| p.get("in").and_then(|i| i.as_str()) != Some("query"))
        .map(|(path, p)| format!("{path}.{}", p["name"]))
        .collect();
    assert_eq!(
        misplaced,
        Vec::<String>::new(),
        "these parameters are read from the query string but are not documented `in: query`"
    );
}

/// Every one of these has a default or is genuinely optional (`GET /items` with no query string
/// is a valid request that yields the default page), so none may be advertised as required.
#[test]
fn no_contributed_parameter_is_required() {
    let doc = document();
    let all = every_contributed_parameter(&doc);
    assert_eq!(
        all.len(),
        4,
        "vacuity guard: expected 4 parameters, walked {all:?}"
    );

    let required: Vec<String> = all
        .iter()
        .filter(|(_, p)| p.get("required") != Some(&Value::Bool(false)))
        .map(|(path, p)| format!("{path}.{}", p["name"]))
        .collect();
    assert_eq!(
        required,
        Vec::<String>::new(),
        "these parameters are documented as required although the extractor supplies a default"
    );
}

/// DRIFT GUARD. The document's declared ceiling and the ceiling `clamp_limit` actually applies
/// are two independent sources; this is the only thing that makes them agree.
#[test]
fn the_declared_limit_maximum_is_the_max_limit_the_extractor_clamps_to() {
    let doc = document();
    for path in ["/items", "/feed"] {
        assert_eq!(
            keyword(&parameter(&doc, path, "get", "limit"), "maximum"),
            f64::from(Pagination::MAX_LIMIT),
            "{path}: the document promises a different ceiling than the extractor enforces"
        );
    }
}

/// DRIFT GUARD, same shape: a client reading this spec must see the page size it will actually
/// get when it sends no `limit`.
#[test]
fn the_declared_limit_default_is_the_default_limit_the_extractor_applies() {
    let doc = document();
    for path in ["/items", "/feed"] {
        assert_eq!(
            keyword(&parameter(&doc, path, "get", "limit"), "default"),
            f64::from(Pagination::DEFAULT_LIMIT),
            "{path}: the document promises a different default page size than the extractor applies"
        );
    }
}

/// The floor is 1, not 0: `clamp_limit` clamps `limit=0` up to 1 rather than returning an empty
/// page, so a spec advertising `minimum: 0` (which is what `u32` alone yields) would describe a
/// request the kit silently rewrites.
#[test]
fn the_declared_limit_minimum_is_the_clamp_floor_of_one() {
    let doc = document();
    for path in ["/items", "/feed"] {
        assert_eq!(
            keyword(&parameter(&doc, path, "get", "limit"), "minimum"),
            1.0,
            "{path}: the document does not describe the 1-item floor `clamp_limit` enforces"
        );
    }
}

/// `offset` falls back to the first page, which the document must say out loud: a generated
/// client that treats a defaultless integer as required would send one needlessly.
#[test]
fn the_declared_offset_default_is_the_first_page() {
    let doc = document();
    assert_eq!(
        keyword(&parameter(&doc, "/items", "get", "offset"), "default"),
        0.0
    );
}

/// CLASS GUARD, the parameter-side sibling of `tests/openapi.rs::no_registered_property_admits_null`.
/// `CursorPagination::cursor` is `Option<String>`, from which utoipa derives a `string | null`
/// union by default — but an absent cursor is an ABSENT PARAMETER, never `?cursor=null`.
/// Optionality is carried by `required: false`, asserted above. This walks what the document
/// declares rather than naming fields, so a new `Option` parameter is covered the day it lands.
#[test]
fn no_contributed_parameter_admits_null() {
    let doc = document();
    let all = every_contributed_parameter(&doc);
    assert_eq!(
        all.len(),
        4,
        "vacuity guard: expected 4 parameters, walked {all:?}"
    );

    let offenders: Vec<String> = all
        .iter()
        .flat_map(|(path, p)| {
            let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("?");
            let schema = p.get("schema").unwrap_or_else(|| {
                panic!("{path}.{name} declares no `schema`, so a null probe would prove nothing")
            });
            null_paths(schema, &format!("{path}.{name}"))
        })
        .collect();
    assert_eq!(
        offenders,
        Vec::<String>::new(),
        "these query parameters admit `null` in the generated document while absence is expressed \
         by omitting the parameter; each needs `#[cfg_attr(feature = \"openapi\", \
         param(nullable = false))]`"
    );
}

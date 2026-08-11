//! Shared parsed-document helpers for the OpenAPI contract suites.
//!
//! Lives in a subdirectory (`tests/openapi_support/mod.rs`) so cargo does not compile it as a
//! test target of its own; `tests/openapi.rs` and `tests/problem_openapi.rs` each include it
//! with `mod openapi_support;`. The helpers were moved here VERBATIM from `tests/openapi.rs`
//! (2026-08-09) so the `problem`-feature suite can call the same walker instead of re-deriving
//! the `allOf` descent — the exact duplication hazard the `## Bugs` entry that drove the move
//! warned about.
//!
//! Each including test binary compiles this module independently and uses a subset of it, so
//! `dead_code` is expected per binary and allowed here (the standard shared-test-module shape);
//! every helper has at least one caller across the two suites.
#![allow(dead_code)]

use serde_json::Value;

/// The schema object for `name`, failing loudly (rather than vacuously passing) if it is absent.
pub fn schema(doc: &Value, name: &str) -> Value {
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
///
/// TOP-LEVEL only: a composed schema (`allOf`, the shape `#[serde(flatten)]` produces) declares
/// no top-level `properties` and this panics loudly on it — use [`every_property`] there.
pub fn property_names(doc: &Value, name: &str) -> Vec<String> {
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

/// The `required` field names a schema declares, sorted. TOP-LEVEL only, like [`property_names`];
/// use [`required_names_deep`] for a composed schema.
pub fn required_names(doc: &Value, name: &str) -> Vec<String> {
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

/// The `required` field names a schema declares, collected across the `allOf`/`oneOf`/`anyOf`
/// branches with the same descent [`every_property`] uses, sorted.
///
/// Fails loudly when NO branch declares a `required` array at all, because "nothing is required"
/// and "the walker never reached the object carrying the requirement" must not read the same —
/// a blind walker would otherwise report an empty requirement set and pass a `required`-shape
/// assertion for free.
pub fn required_names_deep(doc: &Value, name: &str) -> Vec<String> {
    fn collect(node: &Value, out: &mut Vec<String>, seen: &mut bool) {
        if let Some(required) = node.get("required").and_then(|r| r.as_array()) {
            *seen = true;
            out.extend(
                required
                    .iter()
                    .map(|v| v.as_str().expect("required entries are strings").to_owned()),
            );
        }
        for combinator in ["allOf", "oneOf", "anyOf"] {
            if let Some(branches) = node.get(combinator).and_then(|b| b.as_array()) {
                for branch in branches {
                    collect(branch, out, seen);
                }
            }
        }
    }

    let s = schema(doc, name);
    let mut out = Vec::new();
    let mut seen = false;
    collect(&s, &mut out, &mut seen);
    assert!(
        seen,
        "`{name}` declares no `required` array in any branch — the schema shape changed out from \
         under this walker, so an empty requirement set here would prove nothing"
    );
    out.sort();
    out
}

/// The property's declared type, as the document states it.
pub fn property_type(doc: &Value, schema_name: &str, property: &str) -> Value {
    schema(doc, schema_name)
        .get("properties")
        .and_then(|p| p.get(property))
        .and_then(|f| f.get("type"))
        .cloned()
        .unwrap_or_else(|| panic!("`{schema_name}.{property}` declares no `type` at all"))
}

/// Every `(schema, property, body)` triple the document declares, descending through the `allOf`
/// branches that `#[serde(flatten)]` produces. That descent is load-bearing rather than defensive:
/// `Problem` carries a flattened `extensions` map, so its own fields live at
/// `components.schemas.Problem.allOf[1].properties` and a lookup at
/// `components.schemas.Problem.properties` finds **nothing at all**.
pub fn every_property(doc: &Value) -> Vec<(String, String, Value)> {
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

/// Every parameter object one operation declares, in document order.
///
/// Fails loudly when the path, the operation, or the `parameters` array is missing, and when the
/// array is EMPTY — an operation that contributes no parameters would otherwise satisfy every
/// "no parameter does X" assertion for free, which is the vacuity hazard that made
/// `tests/problem_openapi.rs`'s predecessor worthless.
pub fn operation_parameters(doc: &Value, path: &str, method: &str) -> Vec<Value> {
    let operation = doc
        .get("paths")
        .and_then(|p| p.get(path))
        .and_then(|p| p.get(method))
        .unwrap_or_else(|| {
            panic!(
                "no `paths.{path}.{method}` in the generated document; documented paths: {:?}",
                doc.get("paths")
                    .and_then(|p| p.as_object())
                    .map(|m| m.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default()
            )
        });
    let parameters = operation
        .get("parameters")
        .and_then(|p| p.as_array())
        .unwrap_or_else(|| panic!("`paths.{path}.{method}` declares no `parameters` array at all"));
    assert!(
        !parameters.is_empty(),
        "`paths.{path}.{method}` declares an EMPTY `parameters` array, so every parameter \
         assertion below would pass without observing anything"
    );
    parameters.clone()
}

/// The parameter names one operation declares, sorted.
pub fn parameter_names(doc: &Value, path: &str, method: &str) -> Vec<String> {
    let mut names: Vec<String> = operation_parameters(doc, path, method)
        .iter()
        .map(|p| {
            p.get("name")
                .and_then(|n| n.as_str())
                .unwrap_or_else(|| panic!("a parameter of `{path}.{method}` declares no `name`"))
                .to_owned()
        })
        .collect();
    names.sort();
    names
}

/// One named parameter of an operation, failing loudly with the names that ARE declared.
pub fn parameter(doc: &Value, path: &str, method: &str, name: &str) -> Value {
    let parameters = operation_parameters(doc, path, method);
    parameters
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
        .unwrap_or_else(|| {
            panic!(
                "`paths.{path}.{method}` declares no `{name}` parameter; declared: {:?}",
                parameter_names(doc, path, method)
            )
        })
        .clone()
}

/// The sub-paths within one property body that admit `null`, as either an OpenAPI 3.1 type union
/// (`"type": ["string", "null"]`) or a 3.0-style `"nullable": true`. Recurses into `items` so an
/// element type is caught too.
pub fn null_paths(body: &Value, prefix: &str) -> Vec<String> {
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

//! Conditional GET support: [`Revalidated<T>`], the read half of the CRUD
//! lifecycle that [`Created`](crate::Created) / [`Accepted`](crate::Accepted) /
//! [`NoContent`](crate::NoContent) cover for writes.
//!
//! The wrapper serializes its value ONCE into a buffer, hashes that buffer with
//! a vendored FNV-1a 64, emits the digest as a strong `ETag`, and answers
//! `304 Not Modified` with a zero-byte body when the request's `If-None-Match`
//! matches. Every decision here is recorded in
//! `docs/DESIGN_NOTE_C1_CONDITIONAL_GETS.md` (C1-D1 through C1-D4); this module
//! implements those defaults and nothing beyond them.

use axum::{
    http::{
        header::{CONTENT_TYPE, ETAG},
        HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// FNV-1a 64 offset basis (the hash of the empty input).
///
/// Constant from the FNV reference specification; `fnv_reference_vectors` in
/// this module's tests pins it against the published test vectors, so a typo
/// here is a red test rather than a silently different (but still internally
/// consistent) tag family.
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a 64 prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// The fixed `ApiError` message sent when the wrapped value fails to serialize.
///
/// Deliberately NOT the serde error text: `axum::Json`'s default 500 leaks the
/// serde message as a `text/plain` body, and a crate whose point is structured
/// JSON errors should neither leak internals nor change shape on failure. The
/// fixed string also keeps the failure body byte-lockable in tests.
const SERIALIZATION_FAILURE_MESSAGE: &str = "response body could not be serialized";

/// A `200 OK` JSON response carrying a strong `ETag`, answering
/// `304 Not Modified` when the request's `If-None-Match` matches.
///
/// The GET half of conditional requests (RFC 9110 §13): the wrapper serializes
/// `data` once, hashes the exact bytes it will send (vendored FNV-1a 64,
/// rendered as 16 lowercase hex digits), and emits `ETag: "<hex>"`. When a
/// matching `If-None-Match` value was handed in via
/// [`if_none_match`](Revalidated::if_none_match), it sends `304` with a
/// zero-byte body and the same `ETag` instead. Because the tag is computed from
/// the bytes actually emitted — not from a second serialization that merely
/// ought to agree — calling it a *strong* validator is true by construction.
///
/// Matching uses the RFC 9110 §13.1.2 **weak comparison**: a client returning
/// `W/"<hex>"` still matches, `If-None-Match: *` matches whenever there is a
/// representation to serve (for this wrapper there always is), and the field
/// may be a comma-separated list, matching if any member matches. A malformed
/// or non-ASCII field is treated as no match and produces a normal `200`,
/// never an error.
///
/// # Scope: safe methods only (C1-D4)
///
/// This type implements the GET/HEAD behaviour only. The `412 Precondition
/// Failed` half of conditional requests (`If-Match` on writes) is a separate
/// feature, deliberately deferred — see the design note.
///
/// # Serialization failure
///
/// If `data` fails to serialize, the response is `ApiError::internal` — status
/// `500`, `Content-Type: application/json`, and the structured body
/// `{"code":"INTERNAL_ERROR","message":"response body could not be
/// serialized"}` — with **no** `ETag` header. This diverges deliberately from
/// `axum::Json`'s `text/plain` serde-error body: a kit whose point is that
/// failures are structured JSON should not change shape on its own failure,
/// and the serde message (which can name field types and values) is not
/// leaked.
///
/// # `Vary`
///
/// The wrapper does not set `Vary`. A handler that negotiates content (so the
/// same path serves different representations) owns declaring that with
/// `Vary`, and its ETags will differ per representation because each is hashed
/// from the bytes served.
///
/// # Example
///
/// ```rust
/// use axum::http::{header::IF_NONE_MATCH, HeaderMap};
/// use axum::response::IntoResponse;
/// use axum_api_kit::Revalidated;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Item { id: String }
///
/// async fn get_item(headers: HeaderMap) -> impl IntoResponse {
///     let item = Item { id: "1".into() };
///     Revalidated::new(item).if_none_match(headers.get(IF_NONE_MATCH))
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Revalidated<T: Serialize> {
    /// The value serialized as the JSON response body.
    pub data: T,
    /// A caller-supplied opaque tag replacing the computed hash, unquoted.
    etag: Option<String>,
    /// The request's `If-None-Match` value, if the handler passed one in.
    if_none_match: Option<HeaderValue>,
}

impl<T: Serialize> Revalidated<T> {
    /// Builds a `200 OK` response for `data` carrying a computed strong `ETag`.
    pub fn new(data: T) -> Self {
        Self {
            data,
            etag: None,
            if_none_match: None,
        }
    }

    /// Replaces the computed hash with a caller-supplied validator.
    ///
    /// This is the escape hatch the design note names (C1-D1): a consumer who
    /// needs a validator that resists a deliberate collision computes their own
    /// digest (e.g. SHA-256) and hands it in here, and this crate takes no
    /// dependency for it. Pass the **unquoted** opaque tag — the wrapper
    /// renders it as `ETag: "<tag>"` and matches with the same weak comparison.
    ///
    /// Every byte must be legal inside an entity-tag (RFC 9110 §8.8.3:
    /// `0x21`, `0x23`-`0x7E` — printable ASCII except `"`). If any byte is
    /// not, the tag is discarded: the response degrades to a plain `200` with
    /// no `ETag` header and can never answer `304` (the same omit-rather-than-
    /// panic policy as [`Created::with_location`](crate::Created::with_location),
    /// with degradation preferred over emitting a malformed field).
    pub fn with_etag(mut self, tag: impl Into<String>) -> Self {
        self.etag = Some(tag.into());
        self
    }

    /// Hands in the request's `If-None-Match` header value, enabling `304`s.
    ///
    /// Takes the value straight from axum's `HeaderMap`
    /// (`headers.get(IF_NONE_MATCH)`), so a handler needs no unwrapping.
    /// Without this call (or with `None`) the response is always a `200`. Only
    /// the single value passed is examined; a request carrying several
    /// `If-None-Match` field lines is combinable into one comma-separated
    /// value by the client per RFC 9110 §5.2, and axum's `get` returns the
    /// first.
    pub fn if_none_match(mut self, header: Option<&HeaderValue>) -> Self {
        self.if_none_match = header.cloned();
        self
    }
}

impl<T: Serialize> IntoResponse for Revalidated<T> {
    fn into_response(self) -> Response {
        // Serialize ONCE: the buffer that is hashed is the buffer that is sent.
        let body = match serde_json::to_vec(&self.data) {
            Ok(body) => body,
            Err(_) => {
                return crate::ApiError::internal(SERIALIZATION_FAILURE_MESSAGE).into_response()
            }
        };

        let tag = match self.etag {
            Some(custom) => {
                if custom.bytes().all(is_etag_char) {
                    Some(format!("\"{custom}\""))
                } else {
                    // An unusable validator means no revalidation, not a
                    // malformed field on the wire: plain 200, no ETag.
                    None
                }
            }
            None => Some(format!("\"{:016x}\"", fnv1a_64(&body))),
        };

        let Some(tag) = tag else {
            return (
                [(CONTENT_TYPE, HeaderValue::from_static("application/json"))],
                body,
            )
                .into_response();
        };

        // The computed arm is always 18 ASCII bytes of `"`, hex, `"`; the
        // custom arm was just vetted byte-by-byte. Neither can fail here.
        let tag_value = HeaderValue::from_str(&tag)
            .expect("entity-tag characters are always a valid header value");

        let matched = self
            .if_none_match
            .as_ref()
            .is_some_and(|header| if_none_match_matches(header, &tag));

        if matched {
            // 304: zero-byte body, the same ETag repeated (RFC 9110 §15.4.5),
            // and — the design note's C1-D3 decision — no representation
            // metadata: this wrapper sets neither Content-Type nor
            // Content-Length here. (axum's routing layer adds
            // `content-length: 0` to every bodyless response it forwards,
            // exactly as it does for `NoContent`'s 204; RFC 9110 §8.6
            // explicitly permits that header on a 304.)
            (StatusCode::NOT_MODIFIED, [(ETAG, tag_value)]).into_response()
        } else {
            (
                [
                    (CONTENT_TYPE, HeaderValue::from_static("application/json")),
                    (ETAG, tag_value),
                ],
                body,
            )
                .into_response()
        }
    }
}

/// FNV-1a 64 over `bytes` — vendored so the tag can never drift across
/// toolchains (the design note's C1-D1: `std::hash::DefaultHasher` disclaims
/// cross-release stability, which would break ETags between fleet instances
/// built on different rustc versions).
fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Whether `byte` may appear inside an entity-tag's opaque part
/// (RFC 9110 §8.8.3 `etagc`: `0x21` or `0x23`-`0x7E`; obs-text deliberately
/// excluded, since this crate never mints non-ASCII tags).
fn is_etag_char(byte: u8) -> bool {
    byte == 0x21 || (0x23..=0x7e).contains(&byte)
}

/// RFC 9110 §13.1.2: does an `If-None-Match` field match `tag`?
///
/// `tag` is this response's full quoted entity-tag (always strong). Members
/// are compared with the weak comparison function (§8.8.3.2): a `W/` prefix on
/// the client's member is stripped, then the quoted opaque-tags must be
/// byte-identical. `*` matches unconditionally — this wrapper always has a
/// representation. A malformed member (unquoted, or illegal bytes inside) is
/// skipped; a non-ASCII field value matches nothing. Splitting on `,` is safe
/// because `etagc` excludes the comma.
fn if_none_match_matches(header: &HeaderValue, tag: &str) -> bool {
    let Ok(field) = header.to_str() else {
        return false;
    };
    field.split(',').map(str::trim).any(|member| {
        if member == "*" {
            return true;
        }
        let opaque = member.strip_prefix("W/").unwrap_or(member);
        is_well_formed_entity_tag(opaque) && opaque == tag
    })
}

/// Whether `candidate` is a well-formed quoted entity-tag: `"` + etagc* + `"`.
fn is_well_formed_entity_tag(candidate: &str) -> bool {
    let bytes = candidate.as_bytes();
    bytes.len() >= 2
        && bytes[0] == b'"'
        && bytes[bytes.len() - 1] == b'"'
        && bytes[1..bytes.len() - 1].iter().copied().all(is_etag_char)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published FNV-1a 64 reference vectors. A wrong constant above makes
    /// a tag family that is internally consistent (every response-level test
    /// could still pass), so the algorithm is pinned to the external spec
    /// here, not to this crate's own output.
    #[test]
    fn fnv_reference_vectors() {
        assert_eq!(fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn weak_comparison_strips_the_clients_weak_prefix() {
        let header = HeaderValue::from_static("W/\"abc\"");
        assert!(if_none_match_matches(&header, "\"abc\""));
    }

    #[test]
    fn a_list_matches_when_any_member_matches() {
        let header = HeaderValue::from_static("\"one\" , W/\"two\", \"three\"");
        assert!(if_none_match_matches(&header, "\"two\""));
    }

    #[test]
    fn star_matches_any_tag() {
        let header = HeaderValue::from_static("*");
        assert!(if_none_match_matches(&header, "\"anything\""));
    }

    #[test]
    fn an_unquoted_member_never_matches() {
        // Byte-identical to the tag except for the missing quotes.
        let header = HeaderValue::from_static("abc");
        assert!(!if_none_match_matches(&header, "\"abc\""));
    }

    #[test]
    fn a_non_ascii_field_never_matches() {
        let header = HeaderValue::from_bytes(b"\"ab\xffc\"").expect("opaque bytes are legal");
        assert!(!if_none_match_matches(&header, "\"abc\""));
    }

    #[test]
    fn a_malformed_member_does_not_hide_a_matching_one() {
        let header = HeaderValue::from_static("not-quoted, \"abc\"");
        assert!(if_none_match_matches(&header, "\"abc\""));
    }

    #[test]
    fn etag_chars_exclude_the_double_quote() {
        assert!(is_etag_char(b'!'));
        assert!(is_etag_char(b'~'));
        assert!(!is_etag_char(b'"'));
        assert!(!is_etag_char(b' '));
        assert!(!is_etag_char(0x7f));
    }
}

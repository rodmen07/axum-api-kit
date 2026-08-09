//! Accept-header content negotiation for [`Problem`](crate::Problem)
//! responses: [`ProblemFormat`], its infallible extractor impl, and the
//! dependency-free `Accept` parser behind [`ProblemFormat::negotiate`].
//!
//! Moved verbatim out of the parent module (formerly a single
//! `src/problem.rs`) in the 2026-08-08 size split. The public path is
//! unchanged: `ProblemFormat` is re-exported by the parent module and, from
//! there, at the crate root.

use std::convert::Infallible;

use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts, HeaderMap},
};

use super::APPLICATION_PROBLEM_JSON;

/// The media type a [`Problem`](crate::Problem) response is served as: the RFC 9457 default
/// `application/problem+json`, or plain `application/json` for clients that
/// explicitly prefer it.
///
/// This is the opt-in content negotiation half of the `problem` feature.
/// [`Problem`](crate::Problem)'s plain [`IntoResponse`](axum::response::IntoResponse) impl always emits
/// `application/problem+json`; nothing changes for existing users. To
/// negotiate, either extract `ProblemFormat` in a handler (it implements
/// [`FromRequestParts`] infallibly, reading the `Accept` headers) and finish
/// with [`Problem::into_response_with`](crate::Problem::into_response_with), or call
/// [`Problem::into_response_for`](crate::Problem::into_response_for) with the
/// request's [`HeaderMap`] directly.
///
/// Both formats serve byte-identical JSON bodies; only the `Content-Type`
/// header differs. [`Default`] is [`ProblemFormat::ProblemJson`].
///
/// # Example
///
/// ```rust,no_run
/// use axum::{http::StatusCode, response::Response, routing::get, Router};
/// use axum_api_kit::{Problem, ProblemFormat};
///
/// // Accept: application/json          -> Content-Type: application/json
/// // Accept: */* (or no Accept header) -> Content-Type: application/problem+json
/// async fn not_found(format: ProblemFormat) -> Response {
///     Problem::new(StatusCode::NOT_FOUND, "Not Found").into_response_with(format)
/// }
///
/// let app: Router = Router::new().route("/missing", get(not_found));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProblemFormat {
    /// Serve `Content-Type: application/problem+json` (the RFC 9457 media
    /// type, and the default in every ambiguous case).
    #[default]
    ProblemJson,
    /// Serve `Content-Type: application/json`, for clients whose `Accept`
    /// header strictly prefers plain JSON.
    Json,
}

impl ProblemFormat {
    /// Chooses the response format from a request's `Accept` headers.
    ///
    /// This is a minimal, hand-rolled matcher, deliberately not a full
    /// RFC 9110 implementation. The rules:
    ///
    /// 1. Recognized media ranges are `application/problem+json`,
    ///    `application/json`, `application/*`, and `*/*` (ASCII
    ///    case-insensitive, across all `Accept` headers). Every other range,
    ///    including other `+json` suffix types, is ignored.
    /// 2. A range's weight is its `q` parameter (default `1`). Parameters
    ///    other than the first `q` are ignored. A range whose `q` value does
    ///    not parse per the RFC 9110 `qvalue` grammar (`0` to `1` with at
    ///    most three decimals) is ignored entirely.
    /// 3. Each of the two servable types takes its weight from the most
    ///    specific matching range (an exact match beats `application/*`,
    ///    which beats `*/*`); among equally specific matches the highest `q`
    ///    wins.
    /// 4. [`ProblemFormat::Json`] is returned only when plain
    ///    `application/json` ends up with a strictly higher weight than
    ///    `application/problem+json`. Everything else (no `Accept` header,
    ///    `*/*`, `application/*`, equal weights, `q=0` on both, unparseable
    ///    headers) returns [`ProblemFormat::ProblemJson`].
    ///
    /// | `Accept` | result |
    /// |---|---|
    /// | (absent) | problem+json |
    /// | `*/*` | problem+json |
    /// | `application/*` | problem+json |
    /// | `application/json` | plain JSON |
    /// | `application/problem+json` | problem+json |
    /// | `application/json, */*` | problem+json (tie at `q=1`) |
    /// | `application/json;q=0.9, application/problem+json;q=0.5` | plain JSON |
    /// | `application/problem+json, application/json` | problem+json (tie) |
    /// | `text/html` | problem+json (neither matched) |
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum::http::{header, HeaderMap, HeaderValue};
    /// use axum_api_kit::ProblemFormat;
    ///
    /// let mut headers = HeaderMap::new();
    /// headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    /// assert_eq!(ProblemFormat::negotiate(&headers), ProblemFormat::Json);
    ///
    /// assert_eq!(
    ///     ProblemFormat::negotiate(&HeaderMap::new()),
    ///     ProblemFormat::ProblemJson
    /// );
    /// ```
    pub fn negotiate(headers: &HeaderMap) -> Self {
        // Weight of the best matching range seen so far for each servable
        // type: (specificity, q in thousandths). Specificity: 3 exact match,
        // 2 application/*, 1 */*, 0 nothing matched yet.
        let mut problem = (0u8, 0u16);
        let mut json = (0u8, 0u16);

        for value in headers.get_all(header::ACCEPT) {
            let Ok(value) = value.to_str() else {
                // Non-UTF8 header values cannot express a preference we
                // recognize; skip them (the default then wins).
                continue;
            };
            for range in value.split(',') {
                let mut parts = range.split(';');
                // split always yields at least one segment.
                let media = parts.next().unwrap_or("").trim().to_ascii_lowercase();

                let mut q = 1000u16; // qvalue defaults to 1 per RFC 9110
                let mut malformed = false;
                for param in parts {
                    let (key, val) = match param.split_once('=') {
                        Some((key, val)) => (key.trim(), Some(val.trim())),
                        None => (param.trim(), None),
                    };
                    if key.eq_ignore_ascii_case("q") {
                        match val.and_then(parse_qvalue) {
                            Some(thousandths) => q = thousandths,
                            None => malformed = true,
                        }
                        break; // the first q parameter decides
                    }
                }
                if malformed {
                    continue;
                }

                let (specificity, to_problem, to_json) = match media.as_str() {
                    "application/problem+json" => (3u8, true, false),
                    "application/json" => (3, false, true),
                    "application/*" => (2, true, true),
                    "*/*" => (1, true, true),
                    _ => continue,
                };
                let update = |slot: &mut (u8, u16)| {
                    if specificity > slot.0 {
                        *slot = (specificity, q);
                    } else if specificity == slot.0 && q > slot.1 {
                        slot.1 = q;
                    }
                };
                if to_problem {
                    update(&mut problem);
                }
                if to_json {
                    update(&mut json);
                }
            }
        }

        if json.1 > problem.1 {
            Self::Json
        } else {
            Self::ProblemJson
        }
    }

    /// The `Content-Type` value this format serves.
    ///
    /// # Example
    ///
    /// ```rust
    /// use axum_api_kit::{ProblemFormat, APPLICATION_PROBLEM_JSON};
    ///
    /// assert_eq!(ProblemFormat::ProblemJson.content_type(), APPLICATION_PROBLEM_JSON);
    /// assert_eq!(ProblemFormat::Json.content_type(), "application/json");
    /// ```
    pub fn content_type(self) -> &'static str {
        match self {
            Self::ProblemJson => APPLICATION_PROBLEM_JSON,
            Self::Json => "application/json",
        }
    }
}

impl<S> FromRequestParts<S> for ProblemFormat
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self::negotiate(&parts.headers))
    }
}

/// Parses an RFC 9110 `qvalue` into thousandths (`0..=1000`).
///
/// Grammar: `( "0" [ "." *3DIGIT ] ) / ( "1" [ "." *3("0") ] )`. Anything
/// else (empty, `.5`, `1.5`, more than three decimals, stray characters)
/// returns `None`, and [`ProblemFormat::negotiate`] ignores the whole media
/// range that carried it.
fn parse_qvalue(s: &str) -> Option<u16> {
    let (int, frac) = match s.split_once('.') {
        Some((int, frac)) => (int, frac),
        None => (s, ""),
    };
    if frac.len() > 3 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    match int {
        "0" => {
            let mut thousandths = 0u16;
            for digit in frac.bytes() {
                thousandths = thousandths * 10 + u16::from(digit - b'0');
            }
            for _ in frac.len()..3 {
                thousandths *= 10;
            }
            Some(thousandths)
        }
        "1" => frac.bytes().all(|digit| digit == b'0').then_some(1000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn accept(value: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, HeaderValue::from_static(value));
        headers
    }

    #[test]
    fn qvalue_grammar() {
        assert_eq!(parse_qvalue("1"), Some(1000));
        assert_eq!(parse_qvalue("1."), Some(1000));
        assert_eq!(parse_qvalue("1.0"), Some(1000));
        assert_eq!(parse_qvalue("1.000"), Some(1000));
        assert_eq!(parse_qvalue("0"), Some(0));
        assert_eq!(parse_qvalue("0."), Some(0));
        assert_eq!(parse_qvalue("0.5"), Some(500));
        assert_eq!(parse_qvalue("0.85"), Some(850));
        assert_eq!(parse_qvalue("0.855"), Some(855));

        assert_eq!(parse_qvalue("1.001"), None);
        assert_eq!(parse_qvalue("1.5"), None);
        assert_eq!(parse_qvalue("0.8555"), None); // more than three decimals
        assert_eq!(parse_qvalue(".5"), None);
        assert_eq!(parse_qvalue(""), None);
        assert_eq!(parse_qvalue("abc"), None);
        assert_eq!(parse_qvalue("01"), None);
        assert_eq!(parse_qvalue("-1"), None);
        assert_eq!(parse_qvalue("0.5x"), None);
    }

    #[test]
    fn negotiate_defaults_to_problem_json_in_every_ambiguous_case() {
        // No Accept header at all.
        assert_eq!(
            ProblemFormat::negotiate(&HeaderMap::new()),
            ProblemFormat::ProblemJson
        );
        for value in [
            "*/*",
            "application/*",
            "application/problem+json",
            "application/json, */*",                      // tie at q=1
            "application/problem+json, application/json", // tie at q=1
            "application/json;q=0.5, application/problem+json;q=0.5", // explicit tie
            "text/html",                                  // neither matched
            "application/json;q=0",                       // refused, nothing else
            "application/json;q=abc",                     // malformed q: range ignored
            "application/json;q",                         // bare q with no value: range ignored
            "application/json;q=1.5", // q outside the RFC grammar: range ignored
            "application/vnd.api+json", // +json suffix types are not matched
            ";;;,,,",                 // unparseable garbage
        ] {
            assert_eq!(
                ProblemFormat::negotiate(&accept(value)),
                ProblemFormat::ProblemJson,
                "Accept: {value}"
            );
        }
    }

    #[test]
    fn negotiate_serves_plain_json_on_strict_preference() {
        for value in [
            "application/json",
            "Application/JSON", // ASCII case-insensitive
            " application/json ; q=1 ",
            "application/json;q=0.9, application/problem+json;q=0.5",
            "application/json, application/problem+json;q=0.8",
            "text/html;q=1, application/json;q=0.9", // unrelated types are ignored
            "*/*;q=0.1, application/json;q=0.2",
            // Specificity: the exact problem+json match (q=0.5) beats the
            // wildcard (q=1) for problem+json, so plain JSON wins at 1 > 0.5.
            "application/*;q=1, application/problem+json;q=0.5",
        ] {
            assert_eq!(
                ProblemFormat::negotiate(&accept(value)),
                ProblemFormat::Json,
                "Accept: {value}"
            );
        }
    }

    #[test]
    fn negotiate_keeps_problem_json_on_strict_preference_for_it() {
        for value in [
            "application/problem+json;q=0.9, application/json;q=0.5",
            "application/json;q=0.1, application/problem+json",
            "application/*;q=1, application/json;q=0.5", // specificity, mirrored
        ] {
            assert_eq!(
                ProblemFormat::negotiate(&accept(value)),
                ProblemFormat::ProblemJson,
                "Accept: {value}"
            );
        }
    }

    #[test]
    fn negotiate_scans_all_accept_headers() {
        let mut headers = HeaderMap::new();
        headers.append(header::ACCEPT, HeaderValue::from_static("text/html"));
        headers.append(header::ACCEPT, HeaderValue::from_static("application/json"));
        assert_eq!(ProblemFormat::negotiate(&headers), ProblemFormat::Json);
    }

    #[test]
    fn negotiate_skips_non_utf8_accept_values() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, HeaderValue::from_bytes(&[0xFF]).unwrap());
        assert_eq!(
            ProblemFormat::negotiate(&headers),
            ProblemFormat::ProblemJson
        );
    }

    #[test]
    fn problem_format_default_and_content_types() {
        assert_eq!(ProblemFormat::default(), ProblemFormat::ProblemJson);
        assert_eq!(
            ProblemFormat::ProblemJson.content_type(),
            APPLICATION_PROBLEM_JSON
        );
        assert_eq!(ProblemFormat::Json.content_type(), "application/json");
    }
}

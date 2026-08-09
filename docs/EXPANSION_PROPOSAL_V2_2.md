# Expansion proposal: the axum-api-kit feature track after v2.1.0

Status: **PROPOSED 2026-08-09 — awaiting the user's decision.** This document is the
deliverable of `ROADMAP.md`'s v2.2.0 done-when step 1 (drafted by the product analyst;
the user owns the choice). Nothing in it is started, promised, or scheduled beyond
what `ROADMAP.md` already schedules: a v2.2.0 release whose scope is decided here.

**How to answer.** Every decision below is an overridable default, numbered D1-D5.
"Accepted" as a one-word answer takes all five defaults exactly as written. Any
default can be overridden individually ("accepted, but D1 adds C4"), and "nothing
yet" is a valid answer — the ROADMAP's v2.2.0 heading then records that with a date
and a re-ask condition instead of a fictional scope.

**What happens on acceptance.** The decision is written into `ROADMAP.md`'s
`### v2.2.0` heading in the same commit that records it (its done-when step 3),
replacing the placeholder with the accepted scope and with done-when criteria a
build, test, or CI run can check. Implementation starts only after that commit.

## Constraints every candidate was screened against

- **Additive-only within 2.x** (ROADMAP Semver policy): no breaking public-API
  change, and existing response bytes stay locked by the byte-identity tests
  (`tests/rejection_bytes.rs`, `tests/default_response_contracts.rs`). A candidate
  that cannot be built additively is marked as a major driver and excluded from
  v2.2.0 by construction.
- **MSRV 1.81**, enforced by the `MSRV 1.81` CI job. No candidate below needs a
  toolchain above it.
- **Dependency-light posture**: the crate has four unconditional runtime deps
  (axum, serde, serde_json, http) and five optional ones, each behind the flag
  that needs it. A candidate adding a dependency says so explicitly.
- **Eight existing feature flags**: `validator`, `sqlx`, `extract`, `trace`,
  `router`, `cors`, `openapi`, `problem`. Candidates prefer completing an
  existing flag's surface over minting a new flag.

Each capability gap below was verified absent by grep over `src/` on
`origin/main` (`789ec29`) before being claimed, not inherited from prose.

## Candidates

### C1. Conditional-request helpers (ETag / If-None-Match / 304)

A response wrapper (working name `Revalidated<T>`) that computes a strong ETag
from the serialized body, emits it on the response, and returns `304 Not
Modified` with an empty body when the request's `If-None-Match` matches. The
GET half of the CRUD lifecycle the success types (`Created`/`Accepted`/
`NoContent`) already cover for writes.

- Classification: **additive minor**. New type, new opt-in flag (working name
  `etag`); no existing response's bytes change.
- Dependencies: none if the hash is a vendored FNV-1a/SipHash over the
  serialized bytes (std only); a collision-resistant digest would add a dep
  (e.g. `sha2`) — decide inside the design note, defaulting to std-only.
- Size: one PR (type + tests), plus its short design note per the ROADMAP's
  Later-section convention.

### C2. `openapi` completion: the extractors' query parameters enter the document

Today only the response types derive `utoipa::ToSchema`; the `extract` flag's
`Pagination` (`limit`/`offset`, with `DEFAULT_LIMIT`/`MAX_LIMIT` semantics) and
`CursorPagination` (`cursor`/`limit`) parse query parameters that are invisible
to a generated spec — a consumer documents them by hand or not at all. Gate
`utoipa::IntoParams` derives behind `openapi` + `extract` together, so the
parameters the extractors actually enforce appear in the document with their
defaults and bounds.

- Classification: **additive minor**. The generated document GAINS parameter
  entries for consumers who opt in; no API change, no response-byte change.
  (Document-shape changes for `openapi` users shipped as a minor with a
  changelog entry once already: the nullability fix now in `[Unreleased]`.)
- Dependencies: none (utoipa is already the `openapi` dep).
- Size: one PR. The parsed-schema test pattern to extend is already in
  `tests/openapi.rs`.

### C3. JSON fallbacks under `router`: unmatched routes stop speaking plain text

An axum app built from this kit returns structured `ApiError` JSON for every
handled failure — and axum's default empty-body 404 / plain-text 405 for a path
nobody routed, which is exactly the response a client's JSON error handler
cannot parse. Ship `api_fallback()` (404 `ApiError` for unmatched paths) and a
documented `MethodNotAllowed` mapping (405), under the existing `router` flag;
a problem-flavored sibling under `router` + `problem` returns the RFC 9457
shape instead, chosen by naming it — the same opt-in pattern `ProblemJson`
already established.

- Classification: **additive minor**. New handlers only; nothing existing
  changes bytes. `health_routes` is untouched.
- Dependencies: none.
- Size: one PR, response-level tests binding status + `Content-Type` + body
  bytes (the `tests/default_response_contracts.rs` pattern), asserted through a
  real `Router` — behaviour, not greps.

### C4. `IdempotencyKey` extractor (`extract`)

A typed header extractor for `Idempotency-Key` with an `ApiError` rejection
when missing or malformed (and the negotiated problem shape via the existing
`ProblemRejection` machinery). Extraction and validation only — replay storage
is a service concern and stays out of scope.

- Classification: **additive minor** under the existing `extract` flag.
- Dependencies: none (UUID-format validation can reuse the `trace` flag's
  `uuid` dep only when that flag is on; default is syntactic validation, std
  only).
- Size: one PR.

### C5. Request-timeout layer (new flag, new dependency) — NOT recommended

A `TimeoutLayer` producing 504 as `ApiError`/`Problem` would need `tower` as a
runtime dependency (today it is dev-only) or a hand-rolled middleware. Axum
consumers can already compose `tower_http::timeout` with an `ApiError` mapper
in ~10 lines. Weak fit for the dependency-light posture; listed so the option
is visibly declined rather than unconsidered.

- Classification: additive minor, but with a new runtime dep and a new flag.

### C6. Sort-parameter extractor (`extract`)

`SortParams` parsing `?sort=field,-other` against a caller-supplied allow-list,
rejecting unknown fields with 422 (the `ApiError` and problem shapes the
existing extractors use). Completes the list-endpoint story next to
`Pagination`. Slightly more design surface than C4 (allow-list API), which is
why it is second in the deferred queue rather than in v2.2.0.

- Classification: **additive minor** under `extract`.
- Dependencies: none.
- Size: one PR, likely preceded by a short design note on the allow-list API.

## Recommended defaults

- **D1 — v2.2.0 scope: C2 + C3.** One theme: *the API contract is fully
  described and fully JSON* — the parameters the kit enforces appear in the
  spec it generates, and no path through a kit-built app answers in a format
  the kit's own error contract doesn't cover. Both are dependency-free,
  both complete surfaces that already exist rather than opening new ones, and
  together they are two small PRs plus release prep — matching the crate's
  one-minor-per-week cadence ceiling.
- **D2 — C2 ships without a new feature flag**, gated on `openapi` + `extract`
  jointly (a new flag would multiply the feature matrix for no isolation gain;
  the `problemjson` module already uses exactly this joint-gate pattern).
- **D3 — C3 ships under the existing `router` flag**, with the problem-flavored
  fallback a separately named constructor under `router` + `problem` — format
  chosen by naming it, never by feature-sniffing, so enabling `problem` cannot
  change any existing response's bytes.
- **D4 — the deferred queue, in order: C1, then C4, then C6** as v2.3.0+
  candidates, each entering only through its own short design note PR (the
  ROADMAP's Later-section convention). C1 leads because it is the largest
  consumer-visible gap; its `etag`-flag and hash-choice questions are decided
  in its design note, not here.
- **D5 — C5 is declined**, recorded here as considered-and-rejected (new
  runtime dependency for behavior consumers already compose in a few lines).
  Re-proposing it requires new evidence, per the backlog's rejection-decay
  convention (L-022): this reason is conditional on `tower` remaining dev-only
  and on axum offering no native timeout surface.

## What acceptance does NOT authorize

Cutting v2.2.0 still follows the ROADMAP's Releases policy (prep on main, CI
green, the release named); the byte-identity and roadmap-truth guards still
gate every PR; and any candidate that turns out non-additive mid-implementation
comes back as a re-scope, not a silent major.

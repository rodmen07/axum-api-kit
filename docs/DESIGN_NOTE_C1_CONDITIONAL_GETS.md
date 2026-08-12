# Design note: C1 — conditional GETs (ETag / `If-None-Match` / 304)

Status: **OPEN — awaiting the owner's answer.** Written 2026-08-12 as `ROADMAP.md`'s
v2.3.0 done-when clause 1, which asks for exactly this: a short note that ANSWERS the
two questions `docs/EXPANSION_PROPOSAL_V2_2.md` deliberately left to C1's own design
note, and that names the wire format the implementation's byte-level tests will bind.

**This note decides HOW, never WHETHER.** C1 is already scheduled: the owner's
"Accepted" on the expansion proposal (2026-08-10) took D4 as written, and D4 is an
ordered deferred queue — C1, then C4, then C6. Nothing here re-orders that queue, adds
to it, or revives the declined C5. No implementation ships in this PR either; slice 2
is its own PR with its own tests.

**How to answer.** Every decision below is an overridable default. **"Accepted" as a
one-word answer takes all four exactly as written.** Any one can be overridden by name
("accepted, but C1-D2 gates it behind `etag`"), and "not yet" is a valid answer — the
ROADMAP heading then records that with a date and a re-ask condition rather than a
scope nobody chose.

**Numbering.** The defaults here are **C1-D1 … C1-D4**, deliberately not `D1…D4`: the
expansion proposal already owns `D1-D5`, and a one-word override that names a bare
`D2` would be ambiguous across two documents that both live in `docs/`.

## What C1 is (quoted from the accepted proposal, not re-described)

> A response wrapper (working name `Revalidated<T>`) that computes a strong ETag from
> the serialized body, emits it on the response, and returns `304 Not Modified` with
> an empty body when the request's `If-None-Match` matches. The GET half of the CRUD
> lifecycle the success types (`Created`/`Accepted`/`NoContent`) already cover for
> writes.

## The gap, verified rather than inherited

Checked at the source on `origin/main` `2dd3fe889e535174d09a35359101d1c4303dc484`
before anything below was written:

- `grep -rniE "etag|if_none_match|not_modified|Hasher|fnv|sha2|siphash" src/` → **0
  matches**. The crate has no conditional-request surface and no hashing of any kind
  today, so nothing here is a second implementation of something already present.
- `http::header::ETAG` and `http::header::IF_NONE_MATCH` both exist in the
  **unconditional** `http = "1"` dependency (`http-1.4.2/src/header/name.rs:502` and
  `:640`). Naming the two headers therefore costs no dependency and no feature flag.

## C1-D1 — the hash: a vendored FNV-1a over the emitted bytes, std only

**Default: implement FNV-1a (64-bit) inside this crate over the exact bytes the
response body carries, and render it as 16 lowercase hex digits.** No dependency, no
`std::hash` type, roughly ten lines: `hash = 0xcbf29ce4_84222325`, then for each byte
`hash ^= byte; hash = hash.wrapping_mul(0x0000_0100_0000_01b3)`. The proposal's stated
default was "std-only"; this keeps it, and pins WHICH std-only, because the obvious
reading of "std-only" is the wrong one:

**Rejected — `std::hash::DefaultHasher`.** It looks like the zero-code std answer and
it is a latent defect. Its own documentation, read from the installed toolchain
(rustc 1.96.0, `library/std/src/hash/random.rs:90-91`) rather than from memory:

> The internal algorithm is not specified, and so it and its hashes should not be
> relied upon over releases.

`DefaultHasher::new()` is documented as stable *within* a build ("the same as all
other `DefaultHasher` instances created through `new` or `default`", `random.rs:98`),
which is exactly what makes this hard to catch: every test passes, and the failure
appears in production as two real symptoms. A fleet whose instances were built with
different rustc versions emits different ETags for byte-identical bodies, so clients
revalidate forever and never see a 304; and a routine toolchain bump silently
invalidates every ETag the service ever issued. A vendored FNV-1a cannot drift,
because this crate defines it.

**Deferred, not declined — a collision-resistant digest (`sha2`).** FNV-1a is not a
cryptographic hash and this note does not pretend otherwise. What a collision would
cost, stated precisely rather than as a shrug: a client holding representation A is
told 304 for a *different* representation B of the same resource, i.e. a stale read.
Accidentally that is negligible — the birthday bound over one resource's own version
history is n²/2⁶⁵, about 3·10⁻¹⁴ for a resource that changes a thousand times.
Deliberately it is reachable by whoever controls the body bytes. So the condition that
revives `sha2` is concrete, not "if we feel like it": **a consumer needs a validator
that resists a DELIBERATE collision by a party who can influence the represented
data.** Until then the escape hatch costs the crate nothing — the wrapper accepts a
caller-supplied validator (`Revalidated::with_etag(..)`), so that consumer computes
SHA-256 in their own service and this crate takes no dependency and mints no flag for
it.

**Headroom, if wanted (one line, still std-only):** FNV-1a 128-bit — offset basis
`0x6c62272e07bb0142_62b821756295c58d`, prime `0x00000000_01000000_00000000_0000013b`
(2⁸⁸ + 2⁸ + 0x3b), rendered as 32 hex digits. It moves the accidental bound to n²/2¹²⁹ and changes
nothing else in this note. It is not the default because 64 bits already puts
accidental collisions far below every other failure mode in the stack, and a
128-bit tag is not resistant to a deliberate collision either — the case that would
justify the extra width is the case that wants `sha2` instead.

## C1-D2 — the flag: no new `etag` flag; `Revalidated<T>` ships unconditional

**Default: no new feature flag. The type ships in the default (empty) feature set,
beside `Created` / `Accepted` / `NoContent` — the family it completes.**

The proposal sketched "a new type behind a new opt-in flag (working name `etag`)" and
then explicitly handed this question here, which is why answering it the other way is
in remit rather than a re-scope. The reasons, measured off `Cargo.toml` rather than
recalled:

- **Every flag this crate has gates a cost `Revalidated<T>` does not carry.** Five
  pull an optional dependency (`validator`→`validator`, `sqlx`→`sqlx`,
  `trace`→`tracing`+`uuid`, `cors`→`tower-http`, `openapi`→`utoipa`), one turns on an
  axum sub-feature (`extract`→`axum/query`), and the two dependency-free ones each
  gate a large alternative surface: `router` is 276 lines of service wiring and
  `problem` is 1695 (`src/problem/mod.rs` 675 + `negotiation.rs` 365 +
  `problemjson.rs` 655) implementing a second error format end to end. C1 is one
  response type with no dependency, no axum sub-feature and no alternative contract.
- **The crate's own screening constraint says so**, and the owner accepted it:
  *"Candidates prefer completing an existing flag's surface over minting a new flag"*,
  and D2 declined a flag for C2 on the ground that it *"would multiply the feature
  matrix for no isolation gain"*. There is no isolation gain here — the gated thing
  would be ~100 lines of std code — and the matrix cost is real: `--all-features` is
  the only configuration in which a flag-gated surface is ever compiled by this repo's
  CI (`.github/workflows/ci.yml` runs exactly `cargo test` and `cargo test
  --all-features`).
- **Its siblings are unconditional.** `success.rs` (218 lines) is in the default
  feature set, and C1 is defined by the proposal as the GET half of precisely that
  family. A flag would put one quarter of one lifecycle behind an opt-in.
- **It stays a response type.** The wrapper never extracts anything: the handler
  reads the request header with axum's own `HeaderMap` and hands the value in
  (`Revalidated::new(data).if_none_match(headers.get(IF_NONE_MATCH))`). So this does
  not smuggle a request-side surface into the unconditional half of the crate, which
  is the line the existing layout draws.

**The strongest reason to override, stated rather than buried: reversibility is
asymmetric.** Shipping unconditional and gating it later is a BREAKING change and
would need a 3.0. Shipping behind `etag` and dissolving the flag later is not: the
flag can be kept as a no-op `etag = []` so no consumer's `features = ["etag"]` stops
resolving. If the owner wants the option to change their mind cheaply, that is a
sufficient reason to answer **"accepted, but C1-D2 gates it behind `etag`"**, and
nothing else in this note changes if they do.

## C1-D3 — the wire format (the thing slice 2's tests will bind)

**On a 200:** one `ETag` header carrying a **strong** entity-tag — a quoted opaque-tag
with no `W/` prefix and no other parameters:

```
ETag: "9d4f0b2a1c3e5678"
```

**The bytes hashed are the bytes emitted.** The wrapper serializes the value ONCE into
a buffer, hashes that buffer, and sends that same buffer as the body. The tag is then
a function of what is actually on the wire by construction, rather than of a second
serialization that merely ought to agree with the first — which is what makes calling
it a *strong* validator honest.

**Matching:** `If-None-Match` is compared with the **weak comparison function** (RFC
9110 §13.1.2, defined in §8.8.3.2), so a client returning `W/"9d4f0b2a1c3e5678"`
matches the tag above; `If-None-Match: *` matches whenever a representation exists;
and the field may be a comma-separated list, matching if ANY member matches, with
optional whitespace around the commas ignored. A malformed or unparsable field is
treated as no match — it produces a normal 200, never an error.

**On a match:** status `304`, a **zero-byte** body, and the same `ETag` header
repeated. RFC 9110 §15.4.5 requires both halves: a 304 carries no content, and a
server generating one MUST still send the `ETag` it would have sent on the 200.

**What is NOT on the 304:** no `Content-Type` and no `Content-Length`. Being precise
about the authority here, because the ROADMAP's clause 2 phrased it as "the entity
headers RFC 9110 forbids there" and that is slightly stronger than the RFC: what §15.4.5
actually mandates is the absent body plus the presence of `ETag` (and of `Content-Location`,
`Date` and `Vary` when the 200 would have carried them). Omitting representation metadata
for a representation that is not being sent is this note's decision, and it is what slice 2
should assert — as a decision, not as a quoted prohibition.

## C1-D4 — scope limit: safe methods only in v2.3.0

**Default: `Revalidated<T>` implements the GET/HEAD behaviour only.** RFC 9110 §13.1.2
also specifies that a matching `If-None-Match` on a method that is neither GET nor HEAD
must produce `412 Precondition Failed` — the optimistic-concurrency half of conditional
requests. That is a different feature with a different consumer story (`If-Match` on
`PUT`/`PATCH`/`DELETE`), and folding it in silently would make this milestone two
features wearing one name. Recorded as considered-and-deferred rather than
unconsidered, with the condition that would schedule it: **a consumer asks for
conditional WRITES, at which point it enters the deferred queue behind C4 and C6 as
its own candidate with its own note.**

## What slice 2 still has to settle (deliberately not decided here)

Naming these keeps them from being settled silently by whoever implements first:

- The exact constructor and builder names (`Revalidated::new` / `with_etag` /
  `if_none_match` are working names, not decisions).
- The `Content-Type` on the 200 — `application/json`, matching every other JSON body
  this crate emits, but the wrapper writes it itself now that it owns the buffer.
- Behaviour when serialization fails. `axum::Json` answers a 500 with a plain-text
  body; a crate whose entire point is that failures are structured JSON should
  probably answer `ApiError`, and the byte-level test for it belongs with the
  implementation.
- Whether the `Semver compatibility` job's bare `cargo semver-checks` (no
  `--all-features` in `ci.yml`) covers a surface at all if C1-D2 is overridden into a
  flag. Under the default it is moot, since an unconditional item is in whatever
  feature set that tool checks. If the flag is chosen, settle it with
  `cargo semver-checks --help` before assuming coverage.
- Interaction with `Vary` when a handler negotiates content — out of scope for the
  wrapper, but worth a documented sentence.

## What accepting this does NOT authorize

Accepting these four defaults schedules an implementation, nothing else. It does not
re-order the deferred queue (`C1 → C4 → C6` stands), does not revive C5, and does not
loosen any gate: slice 2 is still additive-only under the 2.x policy, still has to
leave `tests/rejection_bytes.rs` and `tests/default_response_contracts.rs` passing
unchanged, and still has to prove its behaviour through a real `Router` via `oneshot`
rather than through a grep. A candidate that turns out non-additive mid-implementation
comes back as a re-scope, never a silent major.

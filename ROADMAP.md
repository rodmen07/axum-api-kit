# Roadmap

axum-api-kit is stable under the 1.0 freeze: no breaking changes in 1.x, with an ACTIVE additive feature stream alongside the maintenance policy recorded here. Expansion is the default posture (user direction, 2026-07-18): 1.1.0 through 1.3.0 all added features, and v1.4.0 below continues that stream. Maintenance work happens to keep the crate healthy against upstream releases or when it unblocks feature work.

Last updated: 2026-07-19.

## Current state (2026-07-19)

- Latest published release: 1.3.0 (2026-07-18) on crates.io: the `problem` feature (dependency-free RFC 9457 `Problem` responses with `application/problem+json`) plus always-on `ApiError::too_many_requests_with_retry_after` and `service_unavailable_with_retry_after` factories. Tag, GitHub release, and the publish workflow all completed; docs.rs built green.
- Shipped since the 1.0.0 freeze (2026-06-03), all additive: 1.1.0 success helpers `Created`/`Accepted`/`NoContent` (2026-06-07), 1.2.0 `ApiJson` extractor with structured JSON rejections (2026-06-07), 1.2.1 through 1.2.7 cross-feature integration test coverage and formatting (2026-06-27), 1.3.0 problem responses and Retry-After factories (2026-07-18).
- Eight feature flags: `validator`, `sqlx`, `extract`, `trace`, `router`, `cors`, `openapi`, `problem`. axum 0.8. MSRV 1.81 declared in Cargo.toml and enforced by the `MSRV 1.81` CI job (see v1.3.2 PR 1).

## Policy

### Semver
- The 1.0.0 freeze (2026-06-03) means no breaking changes in 1.x, not no releases. Additive 1.x minors are policy-compatible and have shipped (1.1.0 through 1.3.0).
- A 1.x minor is justified by: additive helpers, new opt-in feature flags, dependency-bound bumps, or an MSRV raise (with a changelog note).
- A 1.x patch is justified by: bug fixes, docs, tests, formatting.
- A 2.0 is justified only by: a breaking public-API change, or an axum major version (0.9 or 1.0). Because axum types appear in the public API, an axum major is the primary (and currently only known) 2.0 trigger.

### MSRV
- MSRV is 1.81 (`rust-version` in Cargo.toml). It is raised only in a minor release with a changelog note.
- The 1.75 -> 1.81 raise (2026-07-26) corrects fiction rather than dropping support: v2.0.0's validator 0.18 -> 0.20 bump made 1.75 unbuildable (validator 0.20.0 hard-requires rustc 1.81; no 0.20.x is lower), so no consumer on 1.75 could ever have built 2.0.0 with all features. The changelog note rides the next release.
- Toolchains 1.81-1.84 need one pin: `cargo update validator_derive --precise 0.20.0` (0.20.1 is edition2024, which cargo < 1.85 cannot parse). The `MSRV 1.81` CI job documents and exercises exactly this recipe.

### Dependencies
- Optional-feature dependencies (validator, sqlx, tower-http, utoipa) track their upstream crates: a compatible upstream minor gets a bound-widen in a 1.x minor release.
- An axum major forces a written impact assessment before any code change (see the recurring watch below).

### Releases
- Publishing is release-event-triggered: `publish.yml` fires when a GitHub release is published. Version ships (release-prep commit, tag, GitHub release, docs.rs spot-check) run under the user's standing delegation of 2026-07-18 via this documented flow; crates.io publishes are irreversible, so anything ambiguous stays with the user.

## Next milestones

### v1.3.0 ship - COMPLETE 2026-07-18
1.3.0 is live on crates.io with a green docs.rs build (release prep committed, tag v1.3.0 pushed, GitHub release published, publish workflow succeeded, under the standing delegation).

### v1.3.1: maintenance policy on record - agent-doable, mostly complete 2026-07-18
- Create ROADMAP.md at the repo root (this file): done 2026-07-18.
- One-line pointer from the README Stability section to this file: done 2026-07-18.
- USER-ONLY: cut a docs-only release, or let these changes ride along with the next real release.
- Done when: this file and the README pointer are committed to main.

### v1.3.2: CI enforcement of the stated policy - agent-doable, one or two small PRs
CI currently runs the stable toolchain only and checks neither promise this document makes.
- PR 1: SHIPPED 2026-07-26 — the `MSRV 1.81` job in ci.yml builds and tests on a pinned Rust 1.81 toolchain with all features, so the declared `rust-version` is actually enforced. Finding en route: the planned "1.75" was already unbuildable (see the MSRV policy section above), so the job enforces the corrected floor, and `tests/msrv_gate_tests.rs` keeps Cargo.toml, ci.yml, and this document agreeing.
- PR 2: add a cargo-semver-checks CI job comparing against the latest published version, mechanically guarding the 1.x additive-only promise.
- USER-ONLY: cut a release if the user wants these shipped rather than repo-only.
- Done when: both jobs exist and run green on main.

### v1.4.0: problem feature round 2 - agent-doable, one or two small PRs

Continues the additive feature stream by promoting the two features deferred by name in the 1.3.0 changelog.
- PR 1: Accept-header content negotiation for `Problem` responses (serve problem+json or plain JSON based on what the client accepts).
- PR 2: problem+json-formatted extractor rejections: `ApiJson`/`ValidatedJson` failures emit RFC 9457 bodies when the `problem` feature is enabled.
- Unblocked 2026-07-18 by the 1.3.0 release; the 1.4.0 release cut follows the documented flow under the standing delegation.
- Done when: both features are on main with tests and changelog entries.

### Recurring: dependency and toolchain watch (no fixed version)
Background stream: pick these up when they unblock feature work or upstream ships something relevant. Respect the cadence of at most roughly one minor version per week, and only release when there is something to ship.
- Periodically check for new axum, tower-http, utoipa, validator, and sqlx releases; widen bounds and fix deprecations in a small PR when upstream ships a compatible minor.
- Confirm CI is green and docs build; fix only actual breakage, no feature work.
- If axum announces 0.9 or 1.0: write a short impact assessment as its own PR before any code changes (this is the designated 2.0 trigger).
- USER-ONLY: all resulting releases and publishes.

## Later / candidates (unscheduled)

- Further additive 1.x feature ideas as they arise; new opt-in feature flags are policy-compatible. Propose each as a short design note in its PR.
- 2.0 planning: begins only when an axum major lands; no other 2.0 driver exists.

## Blocked and user-only

- Releases run via the documented publish.yml flow under the user's standing delegation (2026-07-18); ambiguous or irreversible judgment calls stay with the user.
- Nothing is blocked by infrastructure. The crate has no runtime infrastructure (crates.io plus GitHub Actions only). The 2026-06-04 GCP/Fly decommission removed the in-house CRM services that were the original consumers; that reinforces the maintenance-only stance but blocks nothing technically.

## History / supersession

- The pre-1.0 roadmap (0.6.0 through 1.0.0, published 2026-06-03) is complete; 1.0.0 was the stable API freeze.
- Earlier planning notes described the crate as "1.0, frozen" with the roadmap done. In practice the freeze froze the existing API against breaking changes while additive 1.x minors continued (1.1.0, 1.2.0, 1.2.1-1.2.7, 1.3.0). "Frozen" means no breaking changes and no active feature stream, not no releases ever. This file supersedes those notes as the single roadmap of record for this crate.
- The shared backlog at d:/Projects/.claude/skills/autodev/backlogs/cargo-crates.md governs agent priority (slokit first); axum-api-kit carries its own additive feature stream (v1.4.0 and beyond), not just maintenance. Read any older "1.0, frozen" phrasing as "1.x additive-only".

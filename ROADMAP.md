# Roadmap

axum-api-kit is stable under an additive-only policy: the 1.0.0 freeze (2026-06-03) carried 1.x through five feature minors with no breaking change, and the 2.x line continues the same posture — 2.0.0 (2026-07-22) changed nothing in this crate's own API or response bytes (see Current state for its real scope). Expansion is the default posture (user direction, 2026-07-18): 1.1.0 through 1.4.0 all added features. Maintenance work happens to keep the crate healthy against upstream releases or when it unblocks feature work.

Last updated: 2026-07-27. The release-state claims in this file are guarded by `tests/roadmap_truth_tests.rs`, which reads this file against Cargo.toml and CHANGELOG.md and fails `cargo test` when they disagree.

## Current state (2026-07-27)

- Latest published release: 2.0.0 (2026-07-22) on crates.io: the opt-in `validator` dependency moved 0.18 -> 0.20, clearing RUSTSEC-2024-0421 (idna 0.5 Punycode) and RUSTSEC-2024-0370 (proc-macro-error, unmaintained) for downstream consumers. Breaking ONLY for consumers who enable the `validator` feature (the `Validate` trait bound on `ValidatedJson<T>` is version-coupled); this crate's own public API and every response's bytes were verified unchanged — the byte-identity suites passed without modification against validator 0.20.
- Shipped since the 1.0.0 freeze (2026-06-03): 1.1.0 success helpers `Created`/`Accepted`/`NoContent` (2026-06-07), 1.2.0 `ApiJson` extractor with structured JSON rejections (2026-06-07), 1.2.1 through 1.2.7 cross-feature integration test coverage and formatting (2026-06-27), 1.3.0 problem responses and Retry-After factories (2026-07-18), 1.4.0 Accept-header negotiation for `Problem` plus the `ProblemJson`/`ProblemValidatedJson` rejection extractors (2026-07-19), 2.0.0 validator 0.18 -> 0.20 (2026-07-22; the only non-additive entry, scoped as above).
- Unreleased on main: what CHANGELOG.md's `[Unreleased]` section lists — currently the MSRV raise note (1.75 -> 1.81) and the `MSRV 1.81` CI job, plus the explicit least-privilege `permissions:` blocks on both workflows and their `tests/workflow_permissions.rs` guard. These ride the next release, whose prep must move them into a dated section and drop this bullet (guarded).
- Eight feature flags: `validator`, `sqlx`, `extract`, `trace`, `router`, `cors`, `openapi`, `problem`. axum 0.8. MSRV 1.81 declared in Cargo.toml and enforced by the `MSRV 1.81` CI job (see v1.3.2 PR 1).

## Policy

### Semver
- The additive-only policy survives major lines: within any major line (1.x then, 2.x now), no breaking changes; minors add, patches fix. Existing response bytes stay locked by the byte-identity tests either way.
- A minor is justified by: additive helpers, new opt-in feature flags, dependency-bound bumps, or an MSRV raise (with a changelog note).
- A patch is justified by: bug fixes, docs, tests, formatting.
- A major is justified only by a breaking public-API change. The designated trigger was an axum major (0.9 or 1.0), because axum types appear in the public API — but the first major to actually fire (2.0.0, 2026-07-22) came through a different route: a version-coupled optional dependency (`validator`), whose security-driven bump changes a public trait bound for feature users. Both routes are live; an axum major remains the primary future trigger.

### MSRV
- MSRV is 1.81 (`rust-version` in Cargo.toml). It is raised only in a minor release with a changelog note.
- The 1.75 -> 1.81 raise (2026-07-26) corrects fiction rather than dropping support: v2.0.0's validator 0.18 -> 0.20 bump made 1.75 unbuildable (validator 0.20.0 hard-requires rustc 1.81; no 0.20.x is lower), so no consumer on 1.75 could ever have built 2.0.0 with all features. The changelog note rides the next release.
- Toolchains 1.81-1.84 need one pin: `cargo update validator_derive --precise 0.20.0` (0.20.1 is edition2024, which cargo < 1.85 cannot parse). The `MSRV 1.81` CI job documents and exercises exactly this recipe.

### Dependencies
- Optional-feature dependencies (validator, sqlx, tower-http, utoipa) track their upstream crates: a compatible upstream minor gets a bound-widen in a minor release. A bump that is version-coupled into the public API (the validator case) is a major here — assess before bumping.
- An axum major forces a written impact assessment before any code change (see the recurring watch below).

### Releases
- Publishing is release-event-triggered: `publish.yml` fires when a GitHub release is published. Version ships (release-prep commit, tag, GitHub release, docs.rs spot-check) are DELEGATED to the agent (user decision 2026-07-26, recorded in the autodev SKILL's Merges-and-releases policy; it supersedes the per-item USER-ONLY release markers this file previously carried). Cut a release only when release prep is on main, CI is green on that commit, and this file or the backlog names the release. crates.io publishes are irreversible: re-read version and changelog before tagging and confirm the published version afterwards. Genuinely ambiguous judgment calls (for example a disputed major-vs-minor classification) stay with the user.

## Next milestones

### v1.3.0 ship - COMPLETE 2026-07-18
1.3.0 is live on crates.io with a green docs.rs build (release prep committed, tag v1.3.0 pushed, GitHub release published, publish workflow succeeded, under the standing delegation).

### v1.3.1: maintenance policy on record - COMPLETE 2026-07-19
This file and the README pointer were committed to main 2026-07-18 (the done-when). The open "docs-only release or ride along" question resolved itself: both files are in the v1.4.0 tag's tree (verified via `git ls-tree v1.4.0`), so they shipped with 1.4.0 on 2026-07-19.

### v1.3.2: CI enforcement of the stated policy - COMPLETE 2026-08-01
CI historically ran the stable toolchain only and checked neither promise this document makes. (The "v1.3.2" label is historical; these are repo-side CI gates that ride the next release whatever its version number.)
- PR 1: SHIPPED 2026-07-26 — the `MSRV 1.81` job in ci.yml builds and tests on a pinned Rust 1.81 toolchain with all features, so the declared `rust-version` is actually enforced. Finding en route: the planned "1.75" was already unbuildable (see the MSRV policy section above), so the job enforces the corrected floor, and `tests/msrv_gate_tests.rs` keeps Cargo.toml, ci.yml, and this document agreeing.
- PR 2: SHIPPED 2026-08-01 as PR #10 (`3ab884a`) — the `Semver compatibility` job runs `cargo semver-checks` against the latest published version, mechanically guarding the additive-only promise (now against the 2.x line). This line read "PR 2: add ..." and the heading read "PR 2 open" for six days after it merged; corrected 2026-08-07 from `gh pr list --state all` and the run below.
- Done when: both jobs exist and run green on main. **MET** — main run `30701775073` (2026-08-01, the merge of PR #10) shows `MSRV 1.81` and `Semver compatibility` both `success`, read from `gh api .../runs/30701775073/jobs`, alongside `Test, Lint, Format` and `Security audit`.

### v1.4.0: problem feature round 2 - COMPLETE 2026-07-19, released the same day
Both deferred 1.3.0 features shipped as the planned two PRs (PR #2: Accept-header content negotiation for `Problem`; PR #3: the `ProblemJson`/`ProblemValidatedJson` sibling extractors with RFC 9457 rejection bodies), then release prep (PR #4), tag v1.4.0, GitHub release, and a green publish workflow; the registry confirmed 1.4.0 at the time (2.0.0 has since superseded it as latest).

### Recurring: dependency and toolchain watch (no fixed version)
Background stream: pick these up when they unblock feature work or upstream ships something relevant. Respect the cadence of at most roughly one minor version per week, and only release when there is something to ship.
- Periodically check for new axum, tower-http, utoipa, validator, and sqlx releases; widen bounds and fix deprecations in a small PR when upstream ships a compatible minor.
- Confirm CI is green and docs build; fix only actual breakage, no feature work.
- If axum announces 0.9 or 1.0: write a short impact assessment as its own PR before any code changes (this is the designated major trigger).
- Resulting releases run under the release delegation above (2026-07-26).

## Later / candidates (unscheduled)

- Further additive feature ideas as they arise; new opt-in feature flags are policy-compatible. Propose each as a short design note in its PR.
- 3.0 planning: begins only on the next forced breaking change. An axum major (0.9 or 1.0) remains the designated trigger and forces the written impact assessment first; 2.0.0 proved a version-coupled dependency bump is a second live route.

## Blocked and user-only

- Releases are delegated per the Releases policy above (2026-07-26). Still user-only: writing repo secrets (`gh secret set`), and any judgment call this file marks ambiguous.
- Nothing is blocked by infrastructure. The crate has no runtime infrastructure (crates.io plus GitHub Actions only).

## History / supersession

- The pre-1.0 roadmap (0.6.0 through 1.0.0, published 2026-06-03) is complete; 1.0.0 was the stable API freeze.
- Earlier planning notes described the crate as "1.0, frozen" with the roadmap done. In practice the freeze froze the existing API against breaking changes while additive minors continued. "Frozen" means no breaking changes and no active feature stream, not no releases ever. This file supersedes those notes as the single roadmap of record for this crate.
- v1.4.0 shipped end to end on 2026-07-19 (PRs #2/#3 features, #4 release prep; tag, GitHub release, publish workflow success, registry confirmed).
- 2.0.0 shipped 2026-07-22 (PR #7, validator 0.18 -> 0.20, user-approved publish; cleared RUSTSEC-2024-0421 and RUSTSEC-2024-0370; tag v2.0.0, publish run 29890003927 success, registry confirmed `newest_version` 2.0.0).
- Superseded prose, recorded before deletion (roadmap-truth pass, 2026-07-27): the Current state section claimed "Latest published release: 1.3.0" for five days after 2.0.0 published; the shipped-since-the-freeze list stopped at 1.3.0 (missing 1.4.0 and 2.0.0); v1.4.0 still sat under Next milestones as open work; the Later section claimed "2.0 planning: begins only when an axum major lands; no other 2.0 driver exists" after the 2.0 had already shipped through a non-axum driver; and the old Blocked section cited the 2026-06-04 infra decommission as "reinforcing the maintenance-only stance", which contradicted this file's own expansion-first intro. All corrected in this pass, and the release-state claims are now mechanically guarded by `tests/roadmap_truth_tests.rs`.
- The shared backlog at d:/Projects/.claude/skills/autodev/backlogs/cargo-crates.md governs agent priority (slokit first); axum-api-kit carries its own additive feature stream, not just maintenance. Read any older "1.0, frozen" phrasing as "additive-only within the current major line".

//! Drift guards for ROADMAP.md's release-state claims (roadmap-truth pass,
//! 2026-07-27).
//!
//! The claims live in three places that must agree: ROADMAP.md's "Latest
//! published release" line and milestone headings, `Cargo.toml`'s `version`,
//! and CHANGELOG.md's released sections. Between 2026-07-22 and 2026-07-27 the
//! roadmap claimed 1.3.0 was the latest release while crates.io served 2.0.0,
//! and v1.4.0 sat under "Next milestones" as open work five days after it
//! shipped — a one-time reconciliation would rot the same way, so these tests
//! read all three sources per run and fail `cargo test` on disagreement.
//! Precedent: `tests/msrv_gate_tests.rs` here, `tests/roadmap_truth.rs` in
//! slokit and svccat.
//!
//! Deliberately NOT guarded: the crates.io registry state. No unit test can
//! reach the network, so "the published version matches" stays a
//! command-verified claim at release time (`curl` the crates.io API), not a
//! test assertion.

use std::fs;
use std::path::Path;

fn repo_file(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
        .replace('\r', "")
}

/// `version = "X.Y.Z"` from Cargo.toml's `[package]` section.
fn cargo_version() -> String {
    let manifest = repo_file("Cargo.toml");
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("version = \""))
        .expect("Cargo.toml declares no version; nothing to reconcile against");
    let start = line.find('"').expect("version line has no opening quote") + 1;
    let end = line.rfind('"').expect("version line has no closing quote");
    line[start..end].to_string()
}

/// The version named by ROADMAP.md's "Latest published release:" claim.
/// Only a BULLET making the claim counts (`- Latest published release: `,
/// at line start) — the History section may quote the phrase inside a
/// tombstone without it being a live claim. Exactly one bullet must exist —
/// if the line is deleted, this guard fails loudly instead of passing
/// vacuously.
fn roadmap_latest_published() -> String {
    let roadmap = repo_file("ROADMAP.md");
    let needle = "- Latest published release: ";
    let claims: Vec<&str> = roadmap.lines().filter(|l| l.starts_with(needle)).collect();
    assert_eq!(
        claims.len(),
        1,
        "ROADMAP.md must state `- Latest published release: <version>` as a \
         bullet exactly once (found {}); the guard has nothing to check \
         without it",
        claims.len()
    );
    let rest = &claims[0][needle.len()..];
    let version: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    assert!(
        !version.is_empty(),
        "ROADMAP.md's `Latest published release:` bullet names no version"
    );
    version
}

/// Every released version in CHANGELOG.md (`## [X.Y.Z] - date` sections),
/// in FILE order — the extractor never sorts, so an ordering guard built on
/// it later cannot be made unfalsifiable by a sorting extractor.
fn changelog_released_versions() -> Vec<String> {
    let changelog = repo_file("CHANGELOG.md");
    changelog
        .lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("## [")?;
            let (version, _) = rest.split_once(']')?;
            if version == "Unreleased" {
                None
            } else {
                Some(version.to_string())
            }
        })
        .collect()
}

/// The `## Next milestones` section of ROADMAP.md (heading to next `## `).
fn milestones_section() -> String {
    let roadmap = repo_file("ROADMAP.md");
    let start = roadmap
        .find("## Next milestones")
        .expect("ROADMAP.md has no `## Next milestones` section; the open-milestone guard stopped guarding anything");
    let body = &roadmap[start..];
    let end = body["## ".len()..]
        .find("\n## ")
        .map(|i| i + "## ".len())
        .unwrap_or(body.len());
    let section = body[..end].to_string();
    assert!(
        section.contains("### v"),
        "the `## Next milestones` section contains no `### v` heading; \
         the open-milestone guard stopped guarding anything"
    );
    section
}

#[test]
fn the_roadmap_latest_published_release_matches_cargo_toml() {
    let claimed = roadmap_latest_published();
    let actual = cargo_version();
    assert_eq!(
        claimed, actual,
        "ROADMAP.md claims the latest published release is {claimed} but \
         Cargo.toml declares version {actual}; a release prep (or a stale \
         roadmap) updated one without the other — reconcile them in the same \
         commit"
    );
}

#[test]
fn the_cargo_version_has_its_own_dated_changelog_section() {
    let version = cargo_version();
    let released = changelog_released_versions();
    assert!(
        released.iter().any(|v| v == &version),
        "Cargo.toml declares version {version} but CHANGELOG.md has no \
         `## [{version}] - <date>` section; a version bump shipped without \
         its changelog entry (released sections found: {released:?})"
    );
}

#[test]
fn no_released_version_is_still_an_open_milestone() {
    let released = changelog_released_versions();
    let section = milestones_section();
    for version in &released {
        for line in section.lines() {
            let Some(rest) = line.strip_prefix(&format!("### v{version}")) else {
                continue;
            };
            // Boundary check: `### v1.3.0` must not claim to match a
            // hypothetical released 1.3.0x; the next char may not extend the
            // version number.
            if rest.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
                continue;
            }
            let lowered = line.to_lowercase();
            assert!(
                lowered.contains("complete") || lowered.contains("shipped"),
                "version {version} has a dated CHANGELOG release section, but \
                 ROADMAP.md still lists it as an open milestone: `{line}`. \
                 Mark the heading COMPLETE/SHIPPED or move it to History"
            );
        }
    }
}

#[test]
fn the_roadmap_declares_unreleased_work_exactly_when_the_changelog_has_some() {
    let changelog = repo_file("CHANGELOG.md");
    let unreleased_start = changelog
        .find("## [Unreleased]")
        .expect("CHANGELOG.md has no [Unreleased] section heading");
    let body = &changelog[unreleased_start + "## [Unreleased]".len()..];
    let end = body.find("\n## [").unwrap_or(body.len());
    let has_entries = !body[..end].trim().is_empty();

    let roadmap = repo_file("ROADMAP.md");
    let declares = roadmap.contains("Unreleased on main");

    if has_entries {
        assert!(
            declares,
            "CHANGELOG.md's [Unreleased] section has entries but ROADMAP.md \
             has no `Unreleased on main` bullet describing them"
        );
    } else {
        assert!(
            !declares,
            "ROADMAP.md still carries an `Unreleased on main` bullet but \
             CHANGELOG.md's [Unreleased] section is empty — a release prep \
             moved the entries out without dropping the bullet"
        );
    }
}

#[test]
fn extractors_report_file_order_and_sane_counts() {
    // Self-test so a silent extractor regression cannot blind the guards.
    let released = changelog_released_versions();
    assert!(
        released.len() >= 20,
        "the changelog extractor found only {} released sections; CHANGELOG.md \
         has 24 as of 2026-07-27, so the extractor is likely broken",
        released.len()
    );
    let pos = |v: &str| {
        released
            .iter()
            .position(|x| x == v)
            .unwrap_or_else(|| panic!("{v} missing from extracted released versions"))
    };
    assert!(
        pos("2.0.0") < pos("1.4.0") && pos("1.4.0") < pos("1.0.0"),
        "released versions are not in file order (newest first); the \
         extractor must report FILE order and never sort: {released:?}"
    );
}

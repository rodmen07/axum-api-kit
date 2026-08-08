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
//! Added 2026-08-08 (milestone-definition pass): a SCHEDULING guard alongside
//! the consistency ones. On that date every versioned milestone in
//! `## Next milestones` was COMPLETE — four of four — so the roadmap named
//! nothing scheduled, while CHANGELOG `[Unreleased]` had held releasable work
//! for thirteen days that the Releases policy forbade cutting because no
//! document named a release for it. The five guards above could not catch it:
//! they check that claims AGREE, and an all-complete milestone list is
//! perfectly self-consistent. `the_next_milestones_section_always_names_an_open_milestone`
//! closes that gap, and it is deliberately a forcing function — the commit
//! that marks a milestone COMPLETE must name its successor in the same commit
//! or `cargo test` goes red, which is the same shape as the unreleased-work
//! bullet guard below.
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

/// The `## Next milestones` section of ROADMAP.md, from its HEADING LINE to
/// the next `## ` heading line.
///
/// The heading match is anchored at line start, and exactly one anchored
/// match must exist. A bare substring search is not good enough here for the
/// same reason `roadmap_latest_published` anchors its bullet: this document
/// discusses its own guards in prose, so the section name also appears inside
/// backticks in the intro paragraph and in History. Measured 2026-08-08 —
/// adding one such sentence made the unanchored search return the PROSE
/// mention, whose "section" ended at the next real heading and contained no
/// milestones at all, failing three guards at once. Loud that time; the same
/// bug with the mention placed later in the file is the silent direction.
fn milestones_section() -> String {
    const HEADING: &str = "## Next milestones";
    let roadmap = repo_file("ROADMAP.md");
    let starts: Vec<usize> = roadmap
        .match_indices(HEADING)
        .map(|(i, _)| i)
        .filter(|i| *i == 0 || roadmap.as_bytes()[i - 1] == b'\n')
        .collect();
    assert_eq!(
        starts.len(),
        1,
        "ROADMAP.md must carry exactly one `{HEADING}` heading at line start \
         (found {}); the milestone guards have nothing to read without it",
        starts.len()
    );
    let start = starts[0];
    let end = roadmap[start + HEADING.len()..]
        .find("\n## ")
        .map(|i| start + HEADING.len() + i)
        .unwrap_or(roadmap.len());
    let section = roadmap[start..end].to_string();
    assert!(
        section.contains("### v"),
        "the `## Next milestones` section contains no `### v` heading; \
         the open-milestone guard stopped guarding anything"
    );
    section
}

/// Every `### v...` heading inside `## Next milestones`, in FILE order.
/// Built on `milestones_section`, so it inherits that function's loud failure
/// when the section or its headings disappear.
fn versioned_milestone_headings() -> Vec<String> {
    milestones_section()
        .lines()
        .filter(|l| l.starts_with("### v"))
        .map(|l| l.to_string())
        .collect()
}

/// Whether a milestone heading declares itself finished. One predicate shared
/// by the released-milestone guard and the open-milestone guard, so the two
/// can never drift into disagreeing about what "done" reads like.
fn heading_is_closed(line: &str) -> bool {
    let lowered = line.to_lowercase();
    lowered.contains("complete") || lowered.contains("shipped")
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
            assert!(
                heading_is_closed(line),
                "version {version} has a dated CHANGELOG release section, but \
                 ROADMAP.md still lists it as an open milestone: `{line}`. \
                 Mark the heading COMPLETE/SHIPPED or move it to History"
            );
        }
    }
}

#[test]
fn the_next_milestones_section_always_names_an_open_milestone() {
    let headings = versioned_milestone_headings();
    let open: Vec<&String> = headings.iter().filter(|l| !heading_is_closed(l)).collect();
    assert!(
        !open.is_empty(),
        "ROADMAP.md's `## Next milestones` section has {} versioned milestone \
         heading(s) and every one of them is marked COMPLETE/SHIPPED, so this \
         crate has nothing scheduled. That is the state this file was in on \
         2026-08-08, and it is how releasable work sat on main for thirteen \
         days: the Releases policy only permits a cut when the roadmap or the \
         backlog NAMES the release, so an empty schedule silently blocks \
         shipping. Define the next milestone (a `### v<x.y.z>: <name>` heading \
         with a done-when that a command can check) in the SAME commit that \
         marked the last one complete. Headings found: {headings:?}",
        headings.len()
    );
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

    // Same self-test for the milestone extractor. Without it, an extractor
    // that silently returned nothing would make
    // `no_released_version_is_still_an_open_milestone` pass vacuously (it
    // iterates and asserts nothing when the list is empty). The open-milestone
    // guard fails loudly on an empty list by construction, so this assertion
    // is what protects the OTHER one.
    let headings = versioned_milestone_headings();
    assert!(
        headings.len() >= 4,
        "the milestone extractor found only {} `### v` heading(s) in \
         `## Next milestones`; ROADMAP.md has 5 as of 2026-08-08, so the \
         extractor is likely broken: {headings:?}",
        headings.len()
    );
}

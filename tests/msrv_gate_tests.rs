//! Drift guards for the MSRV gate (v1.3.2 PR 1, 2026-07-26).
//!
//! The declared MSRV lives in three places that must agree: `Cargo.toml`'s
//! `rust-version`, the `MSRV <version>` job in `.github/workflows/ci.yml`
//! (whose `name:` is also the branch-protection required-context string), and
//! ROADMAP.md's policy claim. These tests read all three sources per run, so a
//! future MSRV raise that edits one file but not the others fails `cargo test`
//! instead of shipping a job that enforces a version the manifest no longer
//! declares (or vice versa). Precedent: svccat's `tests/ci_lint_gate_tests.rs`
//! and slokit's `tests/roadmap_truth.rs`.
//!
//! Deliberately NOT guarded: CHANGELOG.md. Its MSRV note is release-cycle
//! staging that moves out of `[Unreleased]` at release time, so pinning it
//! here would fail the build on every release-prep commit.

use std::fs;
use std::path::Path;

fn repo_file(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The single source of truth: `rust-version` from Cargo.toml.
fn declared_msrv() -> String {
    let manifest = repo_file("Cargo.toml");
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("rust-version"))
        .expect("Cargo.toml declares no rust-version; the MSRV gate has nothing to enforce");
    let start = line
        .find('"')
        .expect("rust-version line has no opening quote")
        + 1;
    let end = line
        .rfind('"')
        .expect("rust-version line has no closing quote");
    line[start..end].to_string()
}

/// The `msrv:` job block from ci.yml: everything between the `  msrv:` job key
/// and the next two-space-indented job key.
fn msrv_job_block() -> String {
    let ci = repo_file(".github/workflows/ci.yml");
    let mut in_block = false;
    let mut block = String::new();
    for line in ci.lines() {
        let is_job_key = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#');
        if is_job_key {
            in_block = line.trim() == "msrv:";
            continue;
        }
        if in_block {
            block.push_str(line);
            block.push('\n');
        }
    }
    assert!(
        !block.is_empty(),
        "ci.yml has no `msrv:` job; the MSRV gate was deleted or renamed out of CI"
    );
    block
}

#[test]
fn the_msrv_job_name_matches_the_declared_rust_version() {
    let msrv = declared_msrv();
    let block = msrv_job_block();
    let expected = format!("name: MSRV {msrv}");
    assert!(
        block.lines().any(|l| l.trim() == expected),
        "ci.yml's msrv job is not named `MSRV {msrv}`. The job name is the \
         branch-protection required-context string, so it must track \
         Cargo.toml's rust-version ({msrv}), and branch protection must be \
         updated in the same change."
    );
}

#[test]
fn the_msrv_job_installs_the_declared_toolchain() {
    let msrv = declared_msrv();
    let block = msrv_job_block();
    let expected = format!("uses: dtolnay/rust-toolchain@{msrv}");
    assert!(
        block.lines().any(|l| l.trim() == expected),
        "ci.yml's msrv job does not install `dtolnay/rust-toolchain@{msrv}`; \
         it would build with a toolchain other than the declared rust-version"
    );
}

#[test]
fn every_cargo_invocation_in_the_msrv_job_uses_stable_or_the_declared_toolchain() {
    let msrv = declared_msrv();
    let block = msrv_job_block();
    let msrv_flag = format!("+{msrv}");
    let mut saw_msrv_invocation = false;
    for line in block.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        // Examine explicit toolchain selectors on cargo commands only.
        for word in trimmed.split_whitespace() {
            if let Some(tool) = word.strip_prefix('+') {
                assert!(
                    tool == "stable" || *word == msrv_flag,
                    "msrv job invokes `cargo {word}`, which is neither +stable \
                     (the resolver) nor +{msrv} (the declared rust-version)"
                );
                if *word == msrv_flag {
                    saw_msrv_invocation = true;
                }
            }
        }
    }
    assert!(
        saw_msrv_invocation,
        "msrv job never invokes `cargo +{msrv}`; nothing in the job actually \
         builds with the declared rust-version"
    );
}

#[test]
fn the_roadmap_msrv_claim_matches_the_declared_rust_version() {
    let msrv = declared_msrv();
    let roadmap = repo_file("ROADMAP.md");
    // Collect every `MSRV is <version>` claim; all of them must state the
    // declared version, and at least one must exist.
    let mut claims = Vec::new();
    for (idx, _) in roadmap.match_indices("MSRV is ") {
        let rest = &roadmap[idx + "MSRV is ".len()..];
        let version: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if !version.is_empty() {
            claims.push(version);
        }
    }
    assert!(
        !claims.is_empty(),
        "ROADMAP.md no longer states the MSRV; the policy section lost its claim"
    );
    for claim in &claims {
        assert_eq!(
            claim, &msrv,
            "ROADMAP.md claims `MSRV is {claim}` but Cargo.toml declares \
             rust-version = \"{msrv}\"; reconcile them in the same change"
        );
    }
}

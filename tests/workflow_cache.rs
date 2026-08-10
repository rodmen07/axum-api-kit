//! Drift guard for the cache posture of publish jobs.
//!
//! A job that runs `cargo publish` must never attempt a rust-cache save.
//! `cargo publish` creates a packaging tree under `target/package/` that the
//! Swatinem/rust-cache post-run cleanup walks and mis-assumes: on publish run
//! 31280520878 (2026-08-08, the first run of publish.yml ever observed) the
//! save emitted four failure-level ENOENT annotations for
//! `target/package/axum-api-kit-2.1.0/tests/{target,trybuild}` — directories
//! the cleaner expects by convention and the packaging tree does not carry —
//! on a run whose every step succeeded. Standing failure-level noise on the
//! one workflow holding the crates.io publish path is exactly what hides a
//! real failure. The same action, saving in two ci.yml jobs with no
//! `cargo publish` beside it, produces zero annotations (run 31315859188,
//! all four jobs), so the ban is scoped to publish jobs, not to the action.
//!
//! A save-suppressed step (`save-if: "false"`) is legal: rust-cache's post
//! step checks `save-if` before any cleanup runs, so a restore-only step
//! never walks `target/` (verified against src/save.ts on the v2 branch).
//! Removing the step altogether — the shape shipped with this guard — is
//! equally legal: the invariant is "never attempts a save", not "never
//! appears".
//!
//! The workflow list is discovered from `.github/workflows`, never
//! hand-enumerated, and an empty discovery is a hard failure, as is finding
//! no publish job at all: a guard that scans nothing proves nothing. If
//! publishing ever moves off GitHub Actions, delete this guard in the same
//! commit, consciously. Precedent for the discovery-and-parse shape:
//! `tests/workflow_permissions.rs`.

use serde_norway::Value;
use std::fs;
use std::path::PathBuf;

/// Discover and parse every workflow file. Empty discovery is a hard failure:
/// a guard that silently scans nothing proves nothing.
fn workflows() -> Vec<(String, Value)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".github/workflows");
    let mut found = Vec::new();
    for entry in fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read workflow dir {}: {e}", dir.display()))
    {
        let path = entry.expect("readable dir entry").path();
        let is_yaml = path
            .extension()
            .is_some_and(|ext| ext == "yml" || ext == "yaml");
        if !is_yaml {
            continue;
        }
        let name = path
            .file_name()
            .expect("workflow file name")
            .to_string_lossy()
            .into_owned();
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {name}: {e}"));
        let value: Value = serde_norway::from_str(&text)
            .unwrap_or_else(|e| panic!("{name} is not parseable YAML: {e}"));
        found.push((name, value));
    }
    assert!(
        !found.is_empty(),
        "no workflow files discovered under {} - the guard is scanning nothing",
        dir.display()
    );
    found
}

/// Every job, across every workflow, that runs `cargo publish` in any step,
/// returned as (workflow-file, job-id, job-value). Finding none is a hard
/// failure: this crate publishes via Actions, so a corpus with no publish job
/// means the guard has lost the thing it exists to watch.
fn publish_jobs() -> Vec<(String, String, Value)> {
    let mut found = Vec::new();
    for (name, value) in workflows() {
        let Some(jobs) = value.get("jobs").and_then(Value::as_mapping) else {
            continue;
        };
        for (job_id, job) in jobs {
            let job_id = job_id.as_str().unwrap_or("<non-string job id>").to_owned();
            let runs_publish = steps_of(job).iter().any(|step| {
                step.get("run")
                    .and_then(Value::as_str)
                    .is_some_and(|run| run.contains("cargo publish"))
            });
            if runs_publish {
                found.push((name.clone(), job_id, job.clone()));
            }
        }
    }
    assert!(
        !found.is_empty(),
        "no job in any workflow runs `cargo publish` - the guard is guarding nothing; \
         if publishing moved off Actions, delete this guard in the same commit"
    );
    found
}

/// The steps of a job, or empty when the job declares none.
fn steps_of(job: &Value) -> Vec<Value> {
    job.get("steps")
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default()
}

/// The guard: any rust-cache step in a job that runs `cargo publish` must
/// suppress its save with `save-if: "false"`. A missing `save-if` (the
/// default is true) or an explicit true is a violation; absence of the step
/// satisfies the invariant trivially.
#[test]
fn jobs_running_cargo_publish_never_attempt_a_cache_save() {
    let mut violations = Vec::new();
    for (workflow, job_id, job) in publish_jobs() {
        for step in steps_of(&job) {
            let Some(uses) = step.get("uses").and_then(Value::as_str) else {
                continue;
            };
            if !uses.starts_with("Swatinem/rust-cache") {
                continue;
            }
            let save_if = step.get("with").and_then(|with| with.get("save-if"));
            let suppressed = matches!(save_if, Some(Value::Bool(false)))
                || save_if.and_then(Value::as_str) == Some("false");
            if !suppressed {
                violations.push(format!(
                    "{workflow} job `{job_id}`: step `{uses}` will attempt a cache save \
                     after `cargo publish` has left its packaging tree in target/ \
                     (save-if is {}); set `save-if: \"false\"` or remove the step",
                    save_if.map_or("absent, defaulting to true".to_owned(), |v| format!(
                        "{v:?}"
                    )),
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "publish jobs attempting a cache save:\n{}",
        violations.join("\n")
    );
}

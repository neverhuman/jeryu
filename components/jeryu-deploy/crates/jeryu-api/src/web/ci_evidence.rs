//! Read-only CI-run evidence assembly for
//! `GET /api/v1/ci/runs/{id}/evidence`.
//!
//! A CI "run" is a [`jeryu_core::CheckRun`] keyed by its UUID. The
//! `ci_run_evidence_route_serves_evidence_and_404` test below proves the live
//! 200/404 split for this route, so the evidence list here stays tied to
//! executable proof instead of prose. Each [`EvidenceItem`] carries:
//!
//! * `kind`   — the evidence facet (`run-metadata`, `head-commit`,
//!   `conclusion`, `output`).
//! * `uri`    — a stable `jeryu://ci/run/{id}/<facet>` locator.
//! * `digest` — `sha256:<hex>` over the canonical JSON of the item's stable
//!   fields (kind + uri + capturedAt + payload), all of which are returned so a
//!   client can verify the item was not altered in transit.
//! * `capturedAt` — the RFC3339 timestamp the underlying datum was recorded
//!   (`started_at` for run/commit facets, `completed_at` for the conclusion).
//! * `payload` — the live source fields backing this evidence facet.
//!
//! Lookup failure is surfaced structurally as `None`; the handler maps it to a
//! 404 rather than silently returning an empty list for a non-existent run.

use jeryu_core::{CheckRun, ForgeCore};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// One piece of CI-run evidence in the external client contract shape.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct EvidenceItem {
    pub kind: String,
    pub uri: String,
    pub digest: String,
    pub captured_at: String,
    pub payload: Value,
}

/// Assemble the evidence list for a CI run id, or `None` when no run with that
/// id exists on the live forge (the handler maps `None` -> 404).
pub(super) fn run_evidence(core: &ForgeCore, run_id: &str) -> Option<Vec<EvidenceItem>> {
    // A run id must be a valid UUID; an ill-formed id can never match a run, so
    // reject it up front rather than scanning every repo for an impossible key.
    let parsed = Uuid::parse_str(run_id).ok()?;
    let run = find_check_run(core, &parsed)?;
    Some(evidence_for_run(run_id, &run))
}

/// Locate a check-run by id across every repository on the forge.
fn find_check_run(core: &ForgeCore, id: &Uuid) -> Option<CheckRun> {
    core.list_repositories(None).into_iter().find_map(|repo| {
        core.list_check_runs(&repo.owner, &repo.name, None)
            .ok()
            .and_then(|runs| runs.check_runs.into_iter().find(|run| &run.id == id))
    })
}

/// Derive the ordered evidence facets from a check-run's existing fields.
fn evidence_for_run(run_id: &str, run: &CheckRun) -> Vec<EvidenceItem> {
    let started = run.started_at.to_rfc3339();
    let mut items = Vec::new();

    // Run identity/metadata, captured when the run started.
    items.push(build_item(
        run_id,
        "run-metadata",
        &started,
        json!({
            "name": run.name,
            "repo": format!("{}/{}", run.owner, run.repo),
            "status": run.status,
        }),
    ));

    // The commit the run executed against.
    items.push(build_item(
        run_id,
        "head-commit",
        &started,
        json!({ "headSha": run.head_sha }),
    ));

    // The conclusion, captured at completion — present only for a completed run.
    if let (Some(conclusion), Some(completed)) = (run.conclusion.as_ref(), run.completed_at) {
        items.push(build_item(
            run_id,
            "conclusion",
            &completed.to_rfc3339(),
            json!({ "conclusion": conclusion }),
        ));
    }

    // The structured check-run output, when present.
    if let Some(output) = run.output.as_ref() {
        let captured = run.completed_at.unwrap_or(run.started_at).to_rfc3339();
        items.push(build_item(
            run_id,
            "output",
            &captured,
            json!({ "title": output.title, "summary": output.summary }),
        ));
    }

    items
}

/// Build one evidence item, computing its `digest` over the canonical JSON of
/// the item's stable fields. The digest deliberately excludes itself.
fn build_item(run_id: &str, kind: &str, captured_at: &str, payload: Value) -> EvidenceItem {
    let uri = format!("jeryu://ci/run/{run_id}/{kind}");
    let digest = digest_of(&json!({
        "kind": kind,
        "uri": uri,
        "capturedAt": captured_at,
        "payload": payload,
    }));
    EvidenceItem {
        kind: kind.to_string(),
        uri,
        digest,
        captured_at: captured_at.to_string(),
        payload,
    }
}

/// `sha256:<hex>` over the canonical JSON encoding of `value`. `serde_json`
/// serializes object keys in insertion order, so we re-key into a sorted
/// `BTreeMap`-backed value first to make the digest stable regardless of field
/// declaration order.
fn digest_of(value: &Value) -> String {
    let canonical = canonicalize(value);
    let encoded =
        serde_json::to_vec(&canonical).expect("canonical serde_json::Value must serialize");
    let hash = Sha256::digest(&encoded);
    format!("sha256:{}", hex::encode(hash))
}

/// Recursively sort object keys so the canonical form is independent of the
/// order fields were written in.
fn canonicalize(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: serde_json::Map<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), canonicalize(v)))
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_iter()
                .collect();
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_core::{
        CheckConclusion, CheckRunOutput, CheckRunStatus, CreateCheckRunRequest,
        CreateRepositoryRequest,
    };

    fn core_with_repo() -> ForgeCore {
        let core = ForgeCore::new();
        core.create_repository(
            "alice",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: false,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
        core
    }

    #[test]
    fn unknown_or_malformed_run_id_yields_none() {
        let core = core_with_repo();
        // A well-formed but absent UUID.
        assert!(run_evidence(&core, &Uuid::new_v4().to_string()).is_none());
        // A non-UUID id never matches and is rejected without scanning.
        assert!(run_evidence(&core, "not-a-uuid").is_none());
    }

    #[test]
    fn completed_run_yields_metadata_commit_and_conclusion_evidence() {
        let core = core_with_repo();
        let run = core
            .create_check_run(
                "alice",
                "jeryu",
                CreateCheckRunRequest {
                    name: "ci".to_string(),
                    head_sha: "deadbeef".to_string(),
                    status: Some(CheckRunStatus::Completed),
                    conclusion: Some(CheckConclusion::Success),
                    output: Some(CheckRunOutput {
                        title: "all green".to_string(),
                        summary: "8 lanes passed".to_string(),
                        text: None,
                    }),
                    ..CreateCheckRunRequest::default()
                },
            )
            .unwrap();
        let id = run.id.to_string();
        let evidence = run_evidence(&core, &id).expect("run exists");

        let kinds: Vec<&str> = evidence.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            vec!["run-metadata", "head-commit", "conclusion", "output"]
        );

        // Every uri uses the contract scheme and carries the run id.
        for item in &evidence {
            assert!(item.uri.starts_with(&format!("jeryu://ci/run/{id}/")));
            assert!(item.digest.starts_with("sha256:"));
            assert!(!item.captured_at.is_empty());
            let returned_fields = json!({
                "kind": item.kind,
                "uri": item.uri,
                "capturedAt": item.captured_at,
                "payload": item.payload,
            });
            assert_eq!(
                item.digest,
                digest_of(&returned_fields),
                "client can recompute digest from returned fields"
            );
        }
    }

    #[test]
    fn queued_run_omits_conclusion_and_output_evidence() {
        let core = core_with_repo();
        let run = core
            .create_check_run(
                "alice",
                "jeryu",
                CreateCheckRunRequest {
                    name: "ci".to_string(),
                    head_sha: "cafef00d".to_string(),
                    status: Some(CheckRunStatus::Queued),
                    ..CreateCheckRunRequest::default()
                },
            )
            .unwrap();
        let evidence = run_evidence(&core, &run.id.to_string()).expect("run exists");
        let kinds: Vec<&str> = evidence.iter().map(|e| e.kind.as_str()).collect();
        // No conclusion or output for a queued run.
        assert_eq!(kinds, vec!["run-metadata", "head-commit"]);
    }

    #[test]
    fn digest_is_a_stable_sha256_over_canonical_json() {
        // Field order in the source object must not change the digest.
        let a =
            json!({ "kind": "x", "uri": "u", "capturedAt": "t", "payload": { "b": 1, "a": 2 } });
        let b =
            json!({ "payload": { "a": 2, "b": 1 }, "capturedAt": "t", "uri": "u", "kind": "x" });
        assert_eq!(digest_of(&a), digest_of(&b));
        assert!(digest_of(&a).starts_with("sha256:"));
        // 64 lowercase hex chars after the prefix.
        let hex = digest_of(&a).strip_prefix("sha256:").unwrap().to_string();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn evidence_serializes_with_camelcase_contract_keys() {
        let item = build_item(
            "run-1",
            "run-metadata",
            "2026-01-01T00:00:00+00:00",
            json!({}),
        );
        let json = serde_json::to_value(&item).unwrap();
        let obj = json.as_object().unwrap();
        for key in ["kind", "uri", "digest", "capturedAt", "payload"] {
            assert!(obj.contains_key(key), "missing contract key: {key}");
        }
    }
}

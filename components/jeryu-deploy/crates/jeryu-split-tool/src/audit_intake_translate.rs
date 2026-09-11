//! Translate only authenticated payload facts. Commit arrays never establish graph coverage.
use super::*;

#[derive(Deserialize)]
struct Repository {
    id: u64,
    full_name: String,
}
#[derive(Deserialize)]
struct Head {
    sha: String,
    repo: Option<JsonObject<Repository>>,
}
#[derive(Deserialize)]
struct Base {
    repo: JsonObject<Repository>,
}
#[derive(Deserialize)]
struct PullRequest {
    number: u64,
    head: JsonObject<Head>,
    base: JsonObject<Base>,
    merged: Option<bool>,
    merge_commit_sha: Option<String>,
}
#[derive(Deserialize)]
struct Release {
    id: u64,
    tag_name: String,
}
#[derive(Deserialize)]
struct Payload {
    repository: JsonObject<Repository>,
    #[serde(rename = "ref")]
    reference: Option<String>,
    ref_type: Option<String>,
    before: Option<String>,
    after: Option<String>,
    created: Option<bool>,
    deleted: Option<bool>,
    forced: Option<bool>,
    action: Option<String>,
    number: Option<u64>,
    pull_request: Option<JsonObject<PullRequest>>,
    release: Option<JsonObject<Release>>,
}

fn reference(value: &str, prefix: &str) -> bool {
    value.starts_with(prefix)
        && value.len() > prefix.len()
        && value.len() <= 1024
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._/-".contains(&byte))
        && !value
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
}
fn endpoint(value: &Option<String>) -> bool {
    value
        .as_deref()
        .is_some_and(|value| crate::audit_scheduler::hex(value, 40))
}
fn zero(value: &Option<String>) -> bool {
    value
        .as_deref()
        .is_some_and(|value| value.bytes().all(|byte| byte == b'0'))
}
fn pending(state: &str, facts: Value) -> Value {
    json!({"state":state,"facts":facts,"planning_complete":false,"queue_imported":false,
        "headers_authenticated":false,"source_verified":false,"execution_verified":false,"publication_qualified":false})
}

/// Failure is represented as metadata, so parsing cannot erase the committed reception.
pub(super) fn translate(route: &Route, headers: &Headers, bytes: &[u8]) -> Value {
    let Ok(JsonObject(payload)) = serde_json::from_slice::<JsonObject<Payload>>(bytes) else {
        return pending(
            "malformed_payload",
            json!({"reason":"invalid_or_duplicate_identity_fields"}),
        );
    };
    if payload.repository.0.id != route.repository_id
        || payload.repository.0.full_name != route.repository
    {
        return pending(
            "wrong_repository",
            json!({"reason":"signed_repository_does_not_match_configured_route"}),
        );
    }
    let facts = match headers.event.as_str() {
        "push" => {
            let Some(reference) = payload.reference.as_deref() else {
                return pending("incomplete_identity", json!({"event":"push"}));
            };
            if !endpoint(&payload.before)
                || !endpoint(&payload.after)
                || payload.created != Some(zero(&payload.before))
                || payload.deleted != Some(zero(&payload.after))
                || zero(&payload.before) && zero(&payload.after)
                || payload.forced.is_none()
            {
                return pending(
                    "incomplete_identity",
                    json!({"event":"push","reason":"missing_or_contradictory_endpoints"}),
                );
            }
            let transition = if zero(&payload.before) {
                "creation"
            } else if zero(&payload.after) {
                "deletion"
            } else {
                "graph_classification_required"
            };
            let facts = json!({"event":"push","source_ref":reference,"before":payload.before,"after":payload.after,
                "transition":transition,"forced_observed":payload.forced,"git_graph_required":true,
                "payload_commit_array_used_for_coverage":false});
            if reference == "refs/heads/audit-evidence" {
                return pending("pending_evidence_branch_review", facts);
            }
            if self::reference(reference, "refs/tags/") {
                return pending("pending_immutable_tag_resolution", facts);
            }
            if !self::reference(reference, "refs/heads/") {
                return pending(
                    "incomplete_identity",
                    json!({"event":"push","reason":"unsupported_source_ref"}),
                );
            }
            facts
        }
        "pull_request" => {
            let Some(JsonObject(pr)) = payload.pull_request else {
                return pending("incomplete_identity", json!({"event":"pull_request"}));
            };
            if pr.number == 0
                || payload.number != Some(pr.number)
                || pr.base.0.repo.0.id != route.repository_id
                || pr.base.0.repo.0.full_name != route.repository
                || !oid(&pr.head.0.sha)
            {
                return pending(
                    "incomplete_identity",
                    json!({"event":"pull_request","reason":"missing_or_conflicting_pr_identity"}),
                );
            }
            let Some(JsonObject(head_repo)) = pr.head.0.repo else {
                return pending(
                    "source_unavailable",
                    json!({"event":"pull_request","number":pr.number,"head_commit":pr.head.0.sha}),
                );
            };
            if head_repo.id == 0 || !slug(&head_repo.full_name) {
                return pending("incomplete_identity", json!({"event":"pull_request"}));
            }
            let merged = payload.action.as_deref() == Some("closed") && pr.merged == Some(true);
            if merged && !pr.merge_commit_sha.as_deref().is_some_and(oid) {
                return pending(
                    "incomplete_identity",
                    json!({"event":"pull_request","number":pr.number,"reason":"merged_revision_missing"}),
                );
            }
            json!({"event":"pull_request","number":pr.number,"source_ref":format!("refs/pull/{}/head",pr.number),
                "head_commit":pr.head.0.sha,"head_repository_id":head_repo.id,"head_repository":head_repo.full_name,
                "fork":head_repo.id!=route.repository_id,"fork_execution_admission":false,"git_graph_required":true,
                "merged_commit":if merged {pr.merge_commit_sha} else {None}})
        }
        "release" => {
            let Some(JsonObject(release)) = payload.release else {
                return pending("incomplete_identity", json!({"event":"release"}));
            };
            let name = format!("refs/tags/{}", release.tag_name);
            if release.id == 0 || !reference(&name, "refs/tags/") {
                return pending("incomplete_identity", json!({"event":"release"}));
            }
            // GitHub's target_commitish may be a floating branch; never invent its tip.
            return pending(
                "pending_immutable_tag_resolution",
                json!({"event":"release","release_id":release.id,"source_ref":name,
                "tag_object_id":null,"peeled_commit":null,"git_graph_required":true}),
            );
        }
        "create" | "delete" => {
            let kind = payload.ref_type.as_deref();
            let prefix = match kind {
                Some("branch") => "refs/heads/",
                Some("tag") => "refs/tags/",
                _ => "",
            };
            let name = payload
                .reference
                .as_deref()
                .map(|name| format!("{prefix}{name}"));
            if prefix.is_empty() || !name.as_deref().is_some_and(|name| reference(name, prefix)) {
                return pending(
                    "incomplete_identity",
                    json!({"event":headers.event,"reason":"missing_ref_identity"}),
                );
            }
            return pending(
                "incomplete_identity",
                json!({"event":headers.event,"source_ref":name,
                "reason":"event_has_no_immutable_previous_and_next_objects","git_graph_required":true}),
            );
        }
        _ => {
            return pending(
                "unsupported_event",
                json!({"event_header_claim":headers.event,"reason":"durably_retained_for_receiver_review"}),
            );
        }
    };
    pending("pending_plan", facts)
}

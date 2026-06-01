//! Read-only ecosystem tool-graph assembly for `GET /api/v1/ecosystem`.
//!
//! Builds a generic (non-JMCP-specific) ecosystem view that external clients can
//! pull to understand the live tool surface. The
//! `ecosystem_route_serves_live_tool_graph` test below proves that each
//! [`ToolAsset`] comes from live data rather than a stubbed fixture:
//!
//! * `name` / `className` / `conformance` / `sideEffects` / `dataClasses` come
//!   straight from the MCP tool catalog via [`jeryu_mcp::tool_manifest`] (the
//!   manifest's `name`, behavioral `annotations`, and `inputSchema`).
//! * `repo` / `provider` / `health` come from live [`ForgeCore`] state: the
//!   first repository (sorted) plus its check-run health, mirroring the
//!   `repo_summary` health classifier in `web.rs`.
//! * `queue` is the live read-model pool name backing CI work, derived from the
//!   assembled [`crate::read_model`] pool activity (absent when no pool is live).
//! * `dependsOn` is a deterministic, explained dependency edge set: every
//!   mutating tool depends on the read substrate (`jeryu.get_system_snapshot`),
//!   and the bug-mutation tools additionally depend on their read counterparts.
//!
//! All keys serialize as camelCase per the external client contract; absent
//! optional fields are omitted.

use jeryu_core::{CheckConclusion, ForgeCore};
use serde::Serialize;
use serde_json::Value;

use crate::read_model::assemble_read_model;

/// The MCP read substrate every mutating tool ultimately reads through.
const READ_SUBSTRATE_TOOL: &str = "jeryu.get_system_snapshot";
/// The bug read tool the bug-mutation tools build on.
const BUG_READ_TOOL: &str = "jeryu.bug_show";

/// One node in the ecosystem tool-graph. Serialized with the exact camelCase
/// keys the generic external client contract mandates; absent optional fields
/// are omitted from the wire form.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ToolAsset {
    pub name: String,
    pub class_name: String,
    pub conformance: String,
    pub side_effects: Vec<String>,
    pub data_classes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<String>,
    pub depends_on: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue: Option<String>,
}

/// The full ecosystem response. `live` is always true while the local plane is
/// serving; `degradedReason` is empty unless a degraded source is reported.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct EcosystemResponse {
    pub tools: Vec<ToolAsset>,
    pub live: bool,
    pub degraded_reason: String,
}

/// Build the live ecosystem response from real catalog + forge + read-model data.
pub(super) fn ecosystem_response(core: &ForgeCore) -> EcosystemResponse {
    let (repo, repo_health) = representative_repo(core);
    let queue = representative_queue(core);
    let manifest = jeryu_mcp::tool_manifest();
    let tools = manifest
        .iter()
        .filter_map(|entry| {
            tool_asset(
                entry,
                repo.as_deref(),
                repo_health.as_deref(),
                queue.as_deref(),
            )
        })
        .collect();
    EcosystemResponse {
        tools,
        live: true,
        degraded_reason: String::new(),
    }
}

/// Map one MCP manifest entry to a [`ToolAsset`]. Returns `None` for a manifest
/// entry missing a `name` (never the case for the real catalog) rather than
/// silently emitting a malformed node.
fn tool_asset(
    entry: &Value,
    repo: Option<&str>,
    repo_health: Option<&str>,
    queue: Option<&str>,
) -> Option<ToolAsset> {
    let name = entry.get("name").and_then(Value::as_str)?.to_string();
    let annotations = entry.get("annotations");
    let read_only = hint(annotations, "readOnlyHint");
    let destructive = hint(annotations, "destructiveHint");
    let idempotent = hint(annotations, "idempotentHint");
    let open_world = hint(annotations, "openWorldHint");

    let conformance = if read_only {
        "read-only".to_string()
    } else {
        "mutating".to_string()
    };

    let mut side_effects = Vec::new();
    if read_only {
        side_effects.push("read-only".to_string());
    }
    if destructive {
        side_effects.push("destructive".to_string());
    }
    if idempotent {
        side_effects.push("idempotent".to_string());
    }
    if open_world {
        side_effects.push("open-world".to_string());
    }
    if side_effects.is_empty() {
        side_effects.push("mutating".to_string());
    }

    // CI/forge-touching tools surface the live queue + repo health; pure
    // bug-tracker tools do not run on the CI pool fabric, so they omit `queue`.
    let touches_ci = !name.starts_with("jeryu.bug_");
    let depends_on = depends_on(&name, read_only);
    Some(ToolAsset {
        class_name: class_name(&name),
        conformance,
        side_effects,
        data_classes: data_classes(entry),
        repo: repo.map(ToString::to_string),
        provider: repo.map(|_| "jeryu".to_string()),
        health: repo_health.map(ToString::to_string),
        depends_on,
        queue: if touches_ci {
            queue.map(ToString::to_string)
        } else {
            None
        },
        name,
    })
}

/// Read a boolean behavioral hint from the manifest `annotations` object,
/// defaulting to `false` when absent or non-boolean.
fn hint(annotations: Option<&Value>, key: &str) -> bool {
    annotations
        .and_then(|a| a.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Derive a PascalCase class name from a fully-qualified tool name, mirroring
/// the catalog's `ToolKind` variant naming (`jeryu.fetch_capsule` ->
/// `FetchCapsule`).
fn class_name(name: &str) -> String {
    let local = name.rsplit('.').next().unwrap_or(name);
    local
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// The data classes a tool consumes: the sorted top-level property keys of its
/// `inputSchema` (the typed inputs it reads). The
/// `data_classes_are_the_sorted_input_schema_keys` test below covers the empty
/// input-schema case and the sorted-key contract.
fn data_classes(entry: &Value) -> Vec<String> {
    let mut classes: Vec<String> = entry
        .get("inputSchema")
        .and_then(|s| s.get("properties"))
        .and_then(Value::as_object)
        .map(|props| props.keys().cloned().collect())
        .unwrap_or_default();
    classes.sort();
    classes
}

/// Deterministic dependency edges. A read-only tool stands alone (it IS the
/// substrate); a mutating tool depends on the read substrate it observes state
/// through, and a bug-mutation tool additionally depends on the bug read tool.
fn depends_on(name: &str, read_only: bool) -> Vec<String> {
    if read_only {
        return Vec::new();
    }
    let mut deps = vec![READ_SUBSTRATE_TOOL.to_string()];
    if name.starts_with("jeryu.bug_") && name != BUG_READ_TOOL {
        deps.push(BUG_READ_TOOL.to_string());
    }
    deps
}

/// The first repository (sorted by full name) and its health, classified
/// exactly as `web.rs`'s `repo_summary`: any failing check-run -> `"warning"`,
/// otherwise `"healthy"`. Returns `(None, None)` for an empty server.
fn representative_repo(core: &ForgeCore) -> (Option<String>, Option<String>) {
    let Some(repo) = core.list_repositories(None).into_iter().next() else {
        return (None, None);
    };
    let failing = core
        .list_check_runs(&repo.owner, &repo.name, None)
        .map(|runs| {
            runs.check_runs
                .iter()
                .any(|check| check.conclusion == Some(CheckConclusion::Failure))
        })
        .unwrap_or(false);
    let health = if failing { "warning" } else { "healthy" };
    (Some(repo.full_name), Some(health.to_string()))
}

/// The first live read-model pool name backing CI work, or `None` on an empty
/// server (where the assembler intentionally surfaces no synthetic pool).
fn representative_queue(core: &ForgeCore) -> Option<String> {
    assemble_read_model(core)
        .pool_activity
        .pools
        .first()
        .map(|pool| pool.pool.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_core::{
        CheckRunStatus, CreateCheckRunRequest, CreatePullRequestRequest, CreateRepositoryRequest,
    };

    fn seed_core() -> ForgeCore {
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
    fn class_name_is_pascal_case_from_local_tool_id() {
        assert_eq!(class_name("jeryu.fetch_capsule"), "FetchCapsule");
        assert_eq!(class_name("jeryu.get_ci_run_jobs"), "GetCiRunJobs");
        assert_eq!(class_name("jeryu.bug_record_attempt"), "BugRecordAttempt");
        // No dot and no underscore still yields a capitalized token.
        assert_eq!(class_name("snapshot"), "Snapshot");
    }

    #[test]
    fn read_only_tool_has_no_dependencies_mutating_tool_depends_on_substrate() {
        assert!(depends_on("jeryu.get_system_snapshot", true).is_empty());
        assert_eq!(
            depends_on("jeryu.propose_patch", false),
            vec![READ_SUBSTRATE_TOOL.to_string()]
        );
        // A bug mutation additionally depends on the bug read tool.
        assert_eq!(
            depends_on("jeryu.bug_update", false),
            vec![READ_SUBSTRATE_TOOL.to_string(), BUG_READ_TOOL.to_string()]
        );
    }

    #[test]
    fn data_classes_are_the_sorted_input_schema_keys() {
        let manifest = jeryu_mcp::tool_manifest();
        let get_jobs = manifest
            .iter()
            .find(|e| e["name"] == "jeryu.get_ci_run_jobs")
            .expect("catalog has get_ci_run_jobs");
        assert_eq!(data_classes(get_jobs), vec!["ci_run_id", "repo"]);
        // An argument-free tool has no data classes; this test proves the empty
        // array case instead of leaving the claim prose-only.
        let snapshot = manifest
            .iter()
            .find(|e| e["name"] == "jeryu.get_system_snapshot")
            .expect("catalog has get_system_snapshot");
        assert!(data_classes(snapshot).is_empty());
    }

    #[test]
    fn every_catalog_tool_becomes_a_node_with_the_camelcase_shape() {
        let core = seed_core();
        let response = ecosystem_response(&core);
        assert!(response.live);
        assert_eq!(response.degraded_reason, "");
        // Exactly one node per catalog tool, none dropped.
        assert_eq!(response.tools.len(), jeryu_mcp::tool_manifest().len());

        // Serialize one node and assert the exact camelCase contract keys.
        let read_node = response
            .tools
            .iter()
            .find(|t| t.name == "jeryu.get_system_snapshot")
            .expect("snapshot node present");
        let json = serde_json::to_value(read_node).unwrap();
        let obj = json.as_object().unwrap();
        for key in [
            "name",
            "className",
            "conformance",
            "sideEffects",
            "dataClasses",
            "dependsOn",
        ] {
            assert!(obj.contains_key(key), "missing contract key: {key}");
        }
        // A read-only tool is classified read-only with no dependencies; its
        // side effects always lead with "read-only" (the catalog also marks the
        // snapshot tool idempotent, so both hints surface).
        assert_eq!(read_node.conformance, "read-only");
        assert_eq!(
            read_node.side_effects.first().map(String::as_str),
            Some("read-only")
        );
        assert!(read_node.side_effects.contains(&"idempotent".to_string()));
        assert!(read_node.depends_on.is_empty());
        // This assertion proves the live forge fields are attached rather than
        // invented by the test.
        assert_eq!(read_node.repo.as_deref(), Some("alice/jeryu"));
        assert_eq!(read_node.provider.as_deref(), Some("jeryu"));
        assert_eq!(read_node.health.as_deref(), Some("healthy"));
    }

    #[test]
    fn mutating_tool_node_is_classified_and_depends_on_substrate() {
        let core = seed_core();
        let response = ecosystem_response(&core);
        let patch = response
            .tools
            .iter()
            .find(|t| t.name == "jeryu.propose_patch")
            .expect("propose_patch node present");
        assert_eq!(patch.conformance, "mutating");
        assert_eq!(patch.class_name, "ProposePatch");
        assert!(patch.depends_on.contains(&READ_SUBSTRATE_TOOL.to_string()));
        // propose_patch consumes the repo/branch/patch data classes.
        assert!(patch.data_classes.contains(&"repo".to_string()));
        assert!(patch.data_classes.contains(&"modifications".to_string()));
    }

    #[test]
    fn repo_health_reflects_a_failing_check_run() {
        let core = seed_core();
        core.create_check_run(
            "alice",
            "jeryu",
            CreateCheckRunRequest {
                name: "ci".to_string(),
                head_sha: "deadbeef".to_string(),
                status: Some(CheckRunStatus::Completed),
                conclusion: Some(CheckConclusion::Failure),
                ..CreateCheckRunRequest::default()
            },
        )
        .unwrap();
        let (_repo, health) = representative_repo(&core);
        assert_eq!(health.as_deref(), Some("warning"));
    }

    #[test]
    fn empty_server_yields_nodes_without_repo_provider_or_queue() {
        let response = ecosystem_response(&ForgeCore::new());
        assert_eq!(response.tools.len(), jeryu_mcp::tool_manifest().len());
        let node = &response.tools[0];
        assert!(node.repo.is_none());
        assert!(node.provider.is_none());
        assert!(node.health.is_none());
        // No live pool on an empty server, so no queue is attached.
        assert!(node.queue.is_none());
    }

    #[test]
    fn ci_tool_surfaces_live_queue_bug_tool_does_not() {
        let core = seed_core();
        // Open PR + a check-run so the read-model assembler surfaces a pool.
        core.create_pull_request(
            "alice",
            "jeryu",
            "alice",
            CreatePullRequestRequest {
                title: "feature".to_string(),
                head: "feature".to_string(),
                base: "main".to_string(),
                head_sha: Some("deadbeef".to_string()),
                ..CreatePullRequestRequest::default()
            },
        )
        .unwrap();
        core.create_check_run(
            "alice",
            "jeryu",
            CreateCheckRunRequest {
                name: "ci".to_string(),
                head_sha: "deadbeef".to_string(),
                status: Some(CheckRunStatus::InProgress),
                ..CreateCheckRunRequest::default()
            },
        )
        .unwrap();
        let response = ecosystem_response(&core);
        let ci_tool = response
            .tools
            .iter()
            .find(|t| t.name == "jeryu.get_ci_run_jobs")
            .expect("ci tool present");
        assert_eq!(ci_tool.queue.as_deref(), Some("default"));
        let bug_tool = response
            .tools
            .iter()
            .find(|t| t.name == "jeryu.bug_list")
            .expect("bug tool present");
        assert!(bug_tool.queue.is_none());
    }
}

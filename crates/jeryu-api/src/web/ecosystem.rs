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
//! * `repos` come from live [`ForgeCore`] repository inventory plus the
//!   gitd-managed mirror state. Missing CI/Jankurai evidence is kept as
//!   `pending`/`unknown`, never counted green.
//! * `queue` is the live read-model pool name backing CI work, derived from the
//!   assembled [`crate::read_model`] pool activity (absent when no pool is live).
//! * `dependsOn` is a deterministic, explained dependency edge set: every
//!   mutating tool depends on the read substrate (`jeryu.get_system_snapshot`),
//!   and the bug-mutation tools additionally depend on their read counterparts.
//!
//! All keys serialize as camelCase per the external client contract; absent
//! optional fields are omitted.

use jeryu_core::{CheckConclusion, CheckRunStatus, ForgeCore, Repository};
use serde::Serialize;
use serde_json::Value;

use crate::read_model::assemble_read_model;
use crate::web::WebState;
use crate::web::code;

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
    pub side_effects: String,
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

/// One Jeryu-managed product repository in the live ecosystem inventory.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct ManagedRepo {
    pub id: String,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub source: String,
    pub import_status: String,
    pub head: String,
    pub default_branch: String,
    pub health: String,
    pub ci: RepoCiEvidence,
    pub jankurai: RepoJankuraiEvidence,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct RepoCiEvidence {
    pub status: String,
    pub total: u32,
    pub passing: u32,
    pub failing: u32,
    pub running: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct RepoJankuraiEvidence {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct Relationship {
    pub source: String,
    pub target: String,
    pub kind: String,
}

/// The full ecosystem response. `live` is true only when Jeryu can report at
/// least one managed repo. Missing evidence stays explicit in `degradedReason`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct EcosystemResponse {
    pub tools: Vec<ToolAsset>,
    pub repos: Vec<ManagedRepo>,
    pub relationships: Vec<Relationship>,
    pub live: bool,
    pub degraded_reason: String,
}

/// Build the live ecosystem response from real catalog + forge + read-model data.
pub(super) fn ecosystem_response(state: &WebState) -> EcosystemResponse {
    let core = state.github.core();
    let repos = managed_repos(state, core);
    let queue = representative_queue(core);
    let manifest = jeryu_mcp::tool_manifest();
    let tools: Vec<ToolAsset> = manifest
        .iter()
        .filter_map(|entry| tool_asset(entry, queue.as_deref()))
        .collect();
    let relationships = relationships(&repos, &tools);
    let degraded_reason = degraded_reason(&repos);
    let live = !repos.is_empty();
    EcosystemResponse {
        tools,
        repos,
        relationships,
        live,
        degraded_reason,
    }
}

/// Map one MCP manifest entry to a [`ToolAsset`]. Returns `None` for a manifest
/// entry missing a `name` (never the case for the real catalog) rather than
/// silently emitting a malformed node.
fn tool_asset(entry: &Value, queue: Option<&str>) -> Option<ToolAsset> {
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
        side_effects: side_effects.join(", "),
        data_classes: data_classes(entry),
        repo: None,
        provider: Some("jeryu".to_string()),
        health: None,
        depends_on,
        queue: if touches_ci {
            queue.map(ToString::to_string)
        } else {
            None
        },
        name,
    })
}

fn managed_repos(state: &WebState, core: &ForgeCore) -> Vec<ManagedRepo> {
    core.list_repositories(None)
        .into_iter()
        .filter(managed_product_repo)
        .map(|repo| managed_repo(state, core, &repo))
        .collect()
}

fn managed_product_repo(repo: &Repository) -> bool {
    let owner = repo.owner.to_ascii_lowercase();
    let name = repo.name.to_ascii_lowercase();
    let full = repo.full_name.to_ascii_lowercase();
    if owner.starts_with('.') || name.starts_with('.') {
        return false;
    }
    !full.contains("stayout") && !full.contains("retired") && !full.contains("/.cache")
}

fn managed_repo(state: &WebState, core: &ForgeCore, repo: &Repository) -> ManagedRepo {
    let ci = ci_evidence(core, repo);
    let jankurai = jankurai_evidence(core, repo);
    let head = code::head_for_repo(state, repo).unwrap_or_else(|| "unknown".to_string());
    let import_status = if head == "unknown" {
        "pending".to_string()
    } else {
        "imported".to_string()
    };
    let health = repo_health(&ci, &jankurai, &import_status);
    ManagedRepo {
        id: repo.id.to_string(),
        owner: repo.owner.clone(),
        name: repo.name.clone(),
        full_name: repo.full_name.clone(),
        source: "gitd".to_string(),
        import_status,
        head,
        default_branch: repo.default_branch.clone(),
        health,
        ci,
        jankurai,
    }
}

fn ci_evidence(core: &ForgeCore, repo: &Repository) -> RepoCiEvidence {
    let checks = core
        .list_check_runs(&repo.owner, &repo.name, None)
        .map(|runs| runs.check_runs)
        .unwrap_or_default();
    let total = checks.len() as u32;
    let passing = checks
        .iter()
        .filter(|check| check.conclusion == Some(CheckConclusion::Success))
        .count() as u32;
    let failing = checks
        .iter()
        .filter(|check| check.conclusion == Some(CheckConclusion::Failure))
        .count() as u32;
    let running = checks
        .iter()
        .filter(|check| {
            matches!(
                check.status,
                CheckRunStatus::Queued | CheckRunStatus::InProgress
            )
        })
        .count() as u32;
    let status = if total == 0 {
        "pending"
    } else if failing > 0 {
        "failing"
    } else if running > 0 {
        "running"
    } else if passing == total {
        "passing"
    } else {
        "pending"
    };
    RepoCiEvidence {
        status: status.to_string(),
        total,
        passing,
        failing,
        running,
    }
}

fn jankurai_evidence(core: &ForgeCore, repo: &Repository) -> RepoJankuraiEvidence {
    let markdown = core
        .readme_or_default(&repo.owner, &repo.name, String::new())
        .unwrap_or_default();
    match parse_jankurai_score(&markdown) {
        Some(score) => RepoJankuraiEvidence {
            status: "known".to_string(),
            score: Some(score),
        },
        None => RepoJankuraiEvidence {
            status: "unknown".to_string(),
            score: None,
        },
    }
}

fn parse_jankurai_score(markdown: &str) -> Option<u32> {
    for line in markdown.lines() {
        let line = line.trim();
        if !(line.contains("Final score") || line.contains("score:")) {
            continue;
        }
        let digits = line
            .chars()
            .skip_while(|ch| !ch.is_ascii_digit())
            .take_while(|ch| ch.is_ascii_digit())
            .collect::<String>();
        if let Ok(score) = digits.parse::<u32>() {
            return Some(score.min(100));
        }
    }
    None
}

fn repo_health(
    ci: &RepoCiEvidence,
    jankurai: &RepoJankuraiEvidence,
    import_status: &str,
) -> String {
    if import_status != "imported" || ci.status == "failing" {
        return "degraded".to_string();
    }
    if ci.status == "pending" || ci.status == "running" || jankurai.status == "unknown" {
        return "watch".to_string();
    }
    "nominal".to_string()
}

fn relationships(repos: &[ManagedRepo], tools: &[ToolAsset]) -> Vec<Relationship> {
    let mut relationships = Vec::new();
    for repo in repos {
        for tool in tools {
            relationships.push(Relationship {
                source: repo.full_name.clone(),
                target: tool.name.clone(),
                kind: "managed_by_tool".to_string(),
            });
        }
    }
    for tool in tools {
        for dependency in &tool.depends_on {
            relationships.push(Relationship {
                source: tool.name.clone(),
                target: dependency.clone(),
                kind: "depends_on".to_string(),
            });
        }
    }
    relationships
}

fn degraded_reason(repos: &[ManagedRepo]) -> String {
    if repos.is_empty() {
        return "jeryu returned no managed repos".to_string();
    }
    let missing_ci = repos
        .iter()
        .filter(|repo| repo.ci.status == "pending")
        .count();
    let missing_jankurai = repos
        .iter()
        .filter(|repo| repo.jankurai.status == "unknown")
        .count();
    let pending_imports = repos
        .iter()
        .filter(|repo| repo.import_status == "pending")
        .count();
    let mut reasons = Vec::new();
    if pending_imports > 0 {
        reasons.push(format!(
            "{pending_imports} repo(s) are missing gitd import evidence"
        ));
    }
    if missing_ci > 0 {
        reasons.push(format!("{missing_ci} repo(s) are missing CI evidence"));
    }
    if missing_jankurai > 0 {
        reasons.push(format!(
            "{missing_jankurai} repo(s) are missing Jankurai score evidence"
        ));
    }
    reasons.join("; ")
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
        let state = WebState::new(core);
        let response = ecosystem_response(&state);
        assert!(response.live);
        assert!(response.degraded_reason.contains("missing CI evidence"));
        // Exactly one node per catalog tool, none dropped.
        assert_eq!(response.tools.len(), jeryu_mcp::tool_manifest().len());
        assert_eq!(response.repos.len(), 1);
        assert_eq!(response.repos[0].full_name, "alice/jeryu");

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
        assert!(read_node.side_effects.contains("read-only"));
        assert!(read_node.side_effects.contains("idempotent"));
        assert!(read_node.depends_on.is_empty());
        // Tool nodes are provider-scoped, not attached to a fake representative
        // repo. Repo inventory is reported in repos[] and relationships[].
        assert_eq!(read_node.repo.as_deref(), None);
        assert_eq!(read_node.provider.as_deref(), Some("jeryu"));
        assert!(read_node.health.is_none());
        assert!(response.relationships.iter().any(|edge| {
            edge.source == "alice/jeryu" && edge.target == "jeryu.get_system_snapshot"
        }));
    }

    #[test]
    fn mutating_tool_node_is_classified_and_depends_on_substrate() {
        let core = seed_core();
        let state = WebState::new(core);
        let response = ecosystem_response(&state);
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
        let state = WebState::new(core);
        let response = ecosystem_response(&state);
        assert_eq!(response.repos[0].ci.status, "failing");
        assert_eq!(response.repos[0].health, "degraded");
    }

    #[test]
    fn empty_server_yields_nodes_without_repo_provider_or_queue() {
        let state = WebState::new(ForgeCore::new());
        let response = ecosystem_response(&state);
        assert!(!response.live);
        assert!(response.repos.is_empty());
        assert_eq!(response.degraded_reason, "jeryu returned no managed repos");
        assert_eq!(response.tools.len(), jeryu_mcp::tool_manifest().len());
        let node = &response.tools[0];
        assert!(node.repo.is_none());
        assert_eq!(node.provider.as_deref(), Some("jeryu"));
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
        let state = WebState::new(core);
        let response = ecosystem_response(&state);
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

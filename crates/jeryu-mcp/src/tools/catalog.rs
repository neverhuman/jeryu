//! The 16-tool catalog: kinds, definitions, input schemas, and argument normalization.
//!
//! Concept renames applied vs. the source (D4):
//! - `mr_iid`      -> `pr_number`
//! - `project_id`  -> `repo`
//! - `pipeline_id` -> `ci_run_id`
//! - tool `get_pipeline_jobs` -> `get_ci_run_jobs`

use serde_json::Value;

use super::schema::*;
use crate::backend::ToolDescriptor;
use crate::{TOOL_PREFIX, tools::CATALOG};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolKind {
    FetchCapsule,
    GetSystemSnapshot,
    GetCiRunJobs,
    GetCiBottlenecks,
    ExplainBlockers,
    PlanValidation,
    RunTests,
    ProposePatch,
    RacePatches,
    RequestMerge,
    BugSubmit,
    BugList,
    BugShow,
    BugReady,
    BugUpdate,
    BugRecordAttempt,
}

#[derive(Debug, Clone)]
pub(crate) struct ToolDefinition {
    title: &'static str,
    description: &'static str,
    annotations: Value,
    input_schema: Value,
    output_schema: Value,
    kind: ToolKind,
}

impl ToolDefinition {
    pub(crate) fn descriptor(&self, action_id: &str) -> ToolDescriptor {
        ToolDescriptor {
            name: format!("{TOOL_PREFIX}{action_id}"),
            title: self.title.to_string(),
            description: self.description.to_string(),
            input_schema: self.input_schema.clone(),
            output_schema: self.output_schema.clone(),
            annotations: self.annotations.clone(),
        }
    }

    /// Validate raw MCP arguments and produce a normalized argument object for the
    /// backend. Returns `None` when required args are missing/ill-typed (-> -32602).
    /// This is the structural equivalent of the source `build_intent`.
    pub(crate) fn normalize_args(&self, args: Value) -> Option<Value> {
        let s = |k: &str| args.get(k).and_then(Value::as_str).map(ToString::to_string);
        let i = |k: &str| args.get(k).and_then(Value::as_i64);
        let opt_s = |k: &str| args.get(k).and_then(Value::as_str).map(ToString::to_string);

        let out = match self.kind {
            ToolKind::FetchCapsule => serde_json::json!({ "job_id": i("job_id")? }),
            ToolKind::GetSystemSnapshot => serde_json::json!({}),
            ToolKind::GetCiRunJobs => serde_json::json!({
                "repo": i("repo")?,
                "ci_run_id": i("ci_run_id")?,
            }),
            ToolKind::GetCiBottlenecks => serde_json::json!({
                "repo": i("repo")?,
                "ref_name": opt_s("ref_name"),
                "limit": args.get("limit").and_then(Value::as_i64),
            }),
            ToolKind::ExplainBlockers => serde_json::json!({
                "entity_type": s("entity_type")?,
                "entity_id": i("entity_id")?,
            }),
            ToolKind::PlanValidation => serde_json::json!({
                "repo": i("repo")?,
                "test_ids": parse_string_array(args.get("test_ids")?)?,
                "ref_name": s("ref_name")?,
            }),
            ToolKind::RunTests => serde_json::json!({
                "repo": i("repo")?,
                "target_ref": s("target_ref")?,
                "test_scope": s("test_scope")?,
            }),
            ToolKind::ProposePatch => serde_json::json!({
                "repo": i("repo")?,
                "branch_name": s("branch_name")?,
                "base_ref": s("base_ref")?,
                "commit_message": s("commit_message")?,
                "modifications": parse_modifications(args.get("modifications")?)?,
                "pr_title": opt_s("pr_title"),
            }),
            ToolKind::RacePatches => serde_json::json!({
                "repo": i("repo")?,
                "base_branch": s("base_branch")?,
                "commit_message": s("commit_message")?,
                "hypotheses": parse_hypotheses(args.get("hypotheses")?)?,
            }),
            ToolKind::RequestMerge => serde_json::json!({
                "repo": i("repo")?,
                "pr_number": i("pr_number")?,
                "source_branch": s("source_branch")?,
                "target_branch": s("target_branch")?,
            }),
            ToolKind::BugSubmit => serde_json::json!({
                "report": args.get("report")?.clone(),
                "idempotency_key": opt_s("idempotency_key"),
            }),
            ToolKind::BugList => serde_json::json!({
                "project": opt_s("project"),
                "status": opt_s("status"),
                "sort": opt_s("sort"),
            }),
            ToolKind::BugShow => serde_json::json!({ "bug_id": s("bug_id")? }),
            ToolKind::BugReady => serde_json::json!({ "project": opt_s("project") }),
            ToolKind::BugUpdate => serde_json::json!({
                "bug_id": s("bug_id")?,
                "status": opt_s("status"),
                "severity": opt_s("severity"),
                "priority": opt_s("priority"),
                "component": opt_s("component"),
                "owner": opt_s("owner"),
            }),
            ToolKind::BugRecordAttempt => serde_json::json!({
                "bug_id": s("bug_id")?,
                "attempt": args.get("attempt")?.clone(),
            }),
        };
        Some(out)
    }
}

/// Build every tool descriptor in `CATALOG` order.
pub(crate) fn catalog() -> Vec<ToolDescriptor> {
    CATALOG
        .iter()
        .filter_map(|id| tool_definition(id).map(|def| def.descriptor(id)))
        .collect()
}

pub(crate) fn tool_definition(action_id: &str) -> Option<ToolDefinition> {
    let (title, description, annotations, kind) = match action_id {
        "fetch_capsule" => (
            "Fetch capsule",
            "Fetch the latest structured failure capsule for a job.",
            tool_annotations(true, false, true, false),
            ToolKind::FetchCapsule,
        ),
        "get_system_snapshot" => (
            "System snapshot",
            "Get a full system state summary.",
            tool_annotations(true, false, true, false),
            ToolKind::GetSystemSnapshot,
        ),
        "get_ci_run_jobs" => (
            "CI run jobs",
            "Fetch the downstream-expanded job list for a CI run.",
            tool_annotations(true, false, true, false),
            ToolKind::GetCiRunJobs,
        ),
        "get_ci_bottlenecks" => (
            "CI bottlenecks",
            "Return historical CI bottlenecks for a repo and optional ref.",
            tool_annotations(true, false, true, false),
            ToolKind::GetCiBottlenecks,
        ),
        "explain_blockers" => (
            "Explain blockers",
            "Explain why a job, release, or pull request is blocked.",
            tool_annotations(true, false, true, false),
            ToolKind::ExplainBlockers,
        ),
        "plan_validation" => (
            "Plan validation",
            "Validate a proposed test plan into proof lanes.",
            tool_annotations(true, false, true, false),
            ToolKind::PlanValidation,
        ),
        "run_tests" => (
            "Run tests",
            "Create an ephemeral branch and trigger a CI run for a test scope.",
            tool_annotations(false, false, false, true),
            ToolKind::RunTests,
        ),
        "propose_patch" => (
            "Propose patch",
            "Create a branch, apply a patch, and open a pull request.",
            tool_annotations(false, false, false, true),
            ToolKind::ProposePatch,
        ),
        "race_patches" => (
            "Race patches",
            "Launch multiple patch hypotheses and keep the first green.",
            tool_annotations(false, false, false, true),
            ToolKind::RacePatches,
        ),
        "request_merge" => (
            "Request merge",
            "Evaluate whether a pull request can be merged through the proof gate.",
            tool_annotations(false, true, false, true),
            ToolKind::RequestMerge,
        ),
        "bug_submit" => (
            "Submit bug",
            "Submit a canonical bug report to the local RedlineDB tracker.",
            tool_annotations(false, false, false, true),
            ToolKind::BugSubmit,
        ),
        "bug_list" => (
            "List bugs",
            "List bugs from the local RedlineDB tracker.",
            tool_annotations(true, false, true, false),
            ToolKind::BugList,
        ),
        "bug_show" => (
            "Show bug",
            "Show a bug and its history from the local RedlineDB tracker.",
            tool_annotations(true, false, true, false),
            ToolKind::BugShow,
        ),
        "bug_ready" => (
            "Ready bugs",
            "List ready unblocked bugs from the local RedlineDB tracker.",
            tool_annotations(true, false, true, false),
            ToolKind::BugReady,
        ),
        "bug_update" => (
            "Update bug",
            "Update triage fields on a local bug.",
            tool_annotations(false, false, false, true),
            ToolKind::BugUpdate,
        ),
        "bug_record_attempt" => (
            "Record bug attempt",
            "Append agent or human attempt history to a local bug.",
            tool_annotations(false, false, false, true),
            ToolKind::BugRecordAttempt,
        ),
        _ => return None,
    };

    let input_schema = tool_input_schema(action_id)?;

    let output_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "success": { "type": "boolean" },
            "message": { "type": "string" },
            "data": {}
        },
        "required": ["success", "message"]
    });

    Some(ToolDefinition {
        title,
        description,
        annotations,
        input_schema,
        output_schema,
        kind,
    })
}

fn tool_input_schema(action_id: &str) -> Option<Value> {
    let schema = match action_id {
        "fetch_capsule" => object_schema(&["job_id"], &[("job_id", integer_schema())]),
        "get_system_snapshot" => object_schema(&[], &[]),
        "get_ci_run_jobs" => object_schema(
            &["repo", "ci_run_id"],
            &[("repo", integer_schema()), ("ci_run_id", integer_schema())],
        ),
        "get_ci_bottlenecks" => object_schema(
            &["repo"],
            &[
                ("repo", integer_schema()),
                ("ref_name", string_schema()),
                ("limit", integer_schema()),
            ],
        ),
        "explain_blockers" => object_schema(
            &["entity_type", "entity_id"],
            &[
                ("entity_type", string_schema()),
                ("entity_id", integer_schema()),
            ],
        ),
        "plan_validation" => object_schema(
            &["repo", "test_ids", "ref_name"],
            &[
                ("repo", integer_schema()),
                ("test_ids", array_schema(string_schema())),
                ("ref_name", string_schema()),
            ],
        ),
        "run_tests" => object_schema(
            &["repo", "target_ref", "test_scope"],
            &[
                ("repo", integer_schema()),
                ("target_ref", string_schema()),
                (
                    "test_scope",
                    enum_schema(&["unit", "integration", "lint", "full"]),
                ),
            ],
        ),
        "propose_patch" => object_schema(
            &[
                "repo",
                "branch_name",
                "base_ref",
                "commit_message",
                "modifications",
            ],
            &[
                ("repo", integer_schema()),
                ("branch_name", string_schema()),
                ("base_ref", string_schema()),
                ("commit_message", string_schema()),
                (
                    "modifications",
                    array_schema(object_schema(
                        &["file_path", "content"],
                        &[("file_path", string_schema()), ("content", string_schema())],
                    )),
                ),
                ("pr_title", string_schema()),
            ],
        ),
        "race_patches" => object_schema(
            &["repo", "base_branch", "commit_message", "hypotheses"],
            &[
                ("repo", integer_schema()),
                ("base_branch", string_schema()),
                ("commit_message", string_schema()),
                (
                    "hypotheses",
                    array_schema(object_schema(
                        &["branch_suffix", "modifications"],
                        &[
                            ("branch_suffix", string_schema()),
                            (
                                "modifications",
                                array_schema(object_schema(
                                    &["file_path", "content"],
                                    &[("file_path", string_schema()), ("content", string_schema())],
                                )),
                            ),
                        ],
                    )),
                ),
            ],
        ),
        "request_merge" => object_schema(
            &["repo", "pr_number", "source_branch", "target_branch"],
            &[
                ("repo", integer_schema()),
                ("pr_number", integer_schema()),
                ("source_branch", string_schema()),
                ("target_branch", string_schema()),
            ],
        ),
        "bug_submit" => object_schema(
            &["report"],
            &[
                ("report", serde_json::json!({"type": "object"})),
                ("idempotency_key", string_schema()),
            ],
        ),
        "bug_list" => object_schema(
            &[],
            &[
                ("project", string_schema()),
                ("status", string_schema()),
                ("sort", string_schema()),
            ],
        ),
        "bug_show" => object_schema(&["bug_id"], &[("bug_id", string_schema())]),
        "bug_ready" => object_schema(&[], &[("project", string_schema())]),
        "bug_update" => object_schema(
            &["bug_id"],
            &[
                ("bug_id", string_schema()),
                ("status", string_schema()),
                ("severity", string_schema()),
                ("priority", string_schema()),
                ("component", string_schema()),
                ("owner", string_schema()),
            ],
        ),
        "bug_record_attempt" => object_schema(
            &["bug_id", "attempt"],
            &[
                ("bug_id", string_schema()),
                ("attempt", serde_json::json!({"type": "object"})),
            ],
        ),
        _ => return None,
    };
    Some(schema)
}

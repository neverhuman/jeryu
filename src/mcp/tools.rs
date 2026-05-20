//! Owner: MCP adapter for external coding agents
//! Proof: `cargo check -p jeryu --message-format=json` and `cargo test -p jeryu --lib mcp`
//! Invariants: The MCP tool catalog must stay in sync with the capability-surface registry
//!             and must map directly onto existing capability intents.

use serde_json::Value;

use super::TOOL_PREFIX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolKind {
    FetchCapsule,
    GetSystemSnapshot,
    GetPipelineJobs,
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
    fn to_json(&self, action_id: &str) -> Value {
        serde_json::json!({
            "name": format!("{TOOL_PREFIX}{action_id}"),
            "title": self.title,
            "description": self.description,
            "inputSchema": self.input_schema,
            "outputSchema": self.output_schema,
            "annotations": self.annotations,
        })
    }

    pub(crate) fn build_intent(&self, args: Value) -> Option<crate::capability::AgentIntent> {
        use crate::capability::AgentIntent;

        match self.kind {
            ToolKind::FetchCapsule => Some(AgentIntent::FetchCapsule {
                job_id: args.get("job_id")?.as_i64()?,
            }),
            ToolKind::GetSystemSnapshot => Some(AgentIntent::GetSystemSnapshot),
            ToolKind::GetPipelineJobs => Some(AgentIntent::GetPipelineJobs {
                project_id: args.get("project_id")?.as_i64()?,
                pipeline_id: args.get("pipeline_id")?.as_i64()?,
            }),
            ToolKind::GetCiBottlenecks => Some(AgentIntent::GetCiBottlenecks {
                project_id: args.get("project_id")?.as_i64()?,
                ref_name: args
                    .get("ref_name")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                limit: args.get("limit").and_then(Value::as_i64),
            }),
            ToolKind::ExplainBlockers => Some(AgentIntent::ExplainBlockers {
                entity_type: args.get("entity_type")?.as_str()?.to_string(),
                entity_id: args.get("entity_id")?.as_i64()?,
            }),
            ToolKind::PlanValidation => Some(AgentIntent::PlanValidation {
                project_id: args.get("project_id")?.as_i64()?,
                test_ids: parse_string_array(args.get("test_ids")?)?,
                ref_name: args.get("ref_name")?.as_str()?.to_string(),
            }),
            ToolKind::RunTests => Some(AgentIntent::RunTests {
                project_id: args.get("project_id")?.as_i64()?,
                target_ref: args.get("target_ref")?.as_str()?.to_string(),
                test_scope: args.get("test_scope")?.as_str()?.to_string(),
            }),
            ToolKind::ProposePatch => Some(AgentIntent::ProposePatch {
                project_id: args.get("project_id")?.as_i64()?,
                branch_name: args.get("branch_name")?.as_str()?.to_string(),
                base_ref: args.get("base_ref")?.as_str()?.to_string(),
                commit_message: args.get("commit_message")?.as_str()?.to_string(),
                modifications: parse_modifications(args.get("modifications")?)?,
                mr_title: args
                    .get("mr_title")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            }),
            ToolKind::RacePatches => Some(AgentIntent::RacePatches {
                project_id: args.get("project_id")?.as_i64()?,
                base_branch: args.get("base_branch")?.as_str()?.to_string(),
                commit_message: args.get("commit_message")?.as_str()?.to_string(),
                hypotheses: parse_hypotheses(args.get("hypotheses")?)?,
            }),
            ToolKind::RequestMerge => Some(AgentIntent::RequestMerge {
                project_id: args.get("project_id")?.as_i64()?,
                mr_iid: args.get("mr_iid")?.as_i64()?,
                source_branch: args.get("source_branch")?.as_str()?.to_string(),
                target_branch: args.get("target_branch")?.as_str()?.to_string(),
            }),
            ToolKind::BugSubmit => Some(AgentIntent::BugSubmit {
                report: serde_json::from_value(args.get("report")?.clone()).ok()?,
                idempotency_key: args
                    .get("idempotency_key")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            }),
            ToolKind::BugList => Some(AgentIntent::BugList {
                project: args
                    .get("project")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                status: args
                    .get("status")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                sort: args
                    .get("sort")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            }),
            ToolKind::BugShow => Some(AgentIntent::BugShow {
                bug_id: args.get("bug_id")?.as_str()?.to_string(),
            }),
            ToolKind::BugReady => Some(AgentIntent::BugReady {
                project: args
                    .get("project")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            }),
            ToolKind::BugUpdate => Some(AgentIntent::BugUpdate {
                bug_id: args.get("bug_id")?.as_str()?.to_string(),
                status: args
                    .get("status")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                severity: args
                    .get("severity")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                priority: args
                    .get("priority")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                component: args
                    .get("component")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
                owner: args
                    .get("owner")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            }),
            ToolKind::BugRecordAttempt => Some(AgentIntent::BugRecordAttempt {
                bug_id: args.get("bug_id")?.as_str()?.to_string(),
                attempt: serde_json::from_value(args.get("attempt")?.clone()).ok()?,
            }),
        }
    }
}

pub fn tool_manifest() -> Vec<Value> {
    use crate::tui::action_registry::{self, Surface};

    action_registry::REGISTRY
        .iter()
        .filter(|entry| entry.surfaces.contains(&Surface::Capability))
        .filter_map(|entry| tool_definition(entry.id).map(|def| def.to_json(entry.id)))
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
        "get_pipeline_jobs" => (
            "Pipeline jobs",
            "Fetch the downstream-expanded job list for a pipeline.",
            tool_annotations(true, false, true, false),
            ToolKind::GetPipelineJobs,
        ),
        "get_ci_bottlenecks" => (
            "CI bottlenecks",
            "Return historical CI bottlenecks for a project and optional ref.",
            tool_annotations(true, false, true, false),
            ToolKind::GetCiBottlenecks,
        ),
        "explain_blockers" => (
            "Explain blockers",
            "Explain why a job, release, or merge is blocked.",
            tool_annotations(true, false, true, false),
            ToolKind::ExplainBlockers,
        ),
        "plan_validation" => (
            "Plan validation",
            "Validate a proposed test plan against selector-miss history.",
            tool_annotations(true, false, true, false),
            ToolKind::PlanValidation,
        ),
        "run_tests" => (
            "Run tests",
            "Create an ephemeral branch and trigger CI for a test scope.",
            tool_annotations(false, false, false, true),
            ToolKind::RunTests,
        ),
        "propose_patch" => (
            "Propose patch",
            "Create a branch, apply a patch, and open an MR.",
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
            "Evaluate whether an MR can be merged through the risk gate.",
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

// ---------------------------------------------------------------------------
// Schema helpers and parsers (extracted to companion)
// ---------------------------------------------------------------------------

#[path = "tools_schema.rs"]
mod tools_schema;
use tools_schema::*;

fn tool_input_schema(action_id: &str) -> Option<Value> {
    let schema = match action_id {
        "fetch_capsule" => object_schema(&["job_id"], &[("job_id", integer_schema())]),
        "get_system_snapshot" => object_schema(&[], &[]),
        "get_pipeline_jobs" => object_schema(
            &["project_id", "pipeline_id"],
            &[
                ("project_id", integer_schema()),
                ("pipeline_id", integer_schema()),
            ],
        ),
        "get_ci_bottlenecks" => object_schema(
            &["project_id"],
            &[
                ("project_id", integer_schema()),
                ("ref_name", string_schema_optional()),
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
            &["project_id", "test_ids", "ref_name"],
            &[
                ("project_id", integer_schema()),
                ("test_ids", array_schema(string_schema())),
                ("ref_name", string_schema()),
            ],
        ),
        "run_tests" => object_schema(
            &["project_id", "target_ref", "test_scope"],
            &[
                ("project_id", integer_schema()),
                ("target_ref", string_schema()),
                (
                    "test_scope",
                    enum_schema(&["unit", "integration", "lint", "full"]),
                ),
            ],
        ),
        "propose_patch" => object_schema(
            &[
                "project_id",
                "branch_name",
                "base_ref",
                "commit_message",
                "modifications",
            ],
            &[
                ("project_id", integer_schema()),
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
                ("mr_title", string_schema_optional()),
            ],
        ),
        "race_patches" => object_schema(
            &["project_id", "base_branch", "commit_message", "hypotheses"],
            &[
                ("project_id", integer_schema()),
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
            &["project_id", "mr_iid", "source_branch", "target_branch"],
            &[
                ("project_id", integer_schema()),
                ("mr_iid", integer_schema()),
                ("source_branch", string_schema()),
                ("target_branch", string_schema()),
            ],
        ),
        "bug_submit" => object_schema(
            &["report"],
            &[
                ("report", serde_json::json!({"type": "object"})),
                ("idempotency_key", string_schema_optional()),
            ],
        ),
        "bug_list" => object_schema(
            &[],
            &[
                ("project", string_schema_optional()),
                ("status", string_schema_optional()),
                ("sort", string_schema_optional()),
            ],
        ),
        "bug_show" => object_schema(&["bug_id"], &[("bug_id", string_schema())]),
        "bug_ready" => object_schema(&[], &[("project", string_schema_optional())]),
        "bug_update" => object_schema(
            &["bug_id"],
            &[
                ("bug_id", string_schema()),
                ("status", string_schema_optional()),
                ("severity", string_schema_optional()),
                ("priority", string_schema_optional()),
                ("component", string_schema_optional()),
                ("owner", string_schema_optional()),
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

//! Per-tool metadata: titles, descriptions, annotations, and assembled definitions.

use super::input_schema::tool_input_schema;
use super::kind::{ToolDefinition, ToolKind};
use crate::tools::schema::tool_annotations;

pub(crate) fn tool_definition(action_id: &str) -> Option<ToolDefinition> {
    let (title, description, annotations, kind) = match action_id {
        "repo_list" => (
            "Repository list",
            "List live Jeryu-managed repositories.",
            tool_annotations(true, false, true, false),
            ToolKind::RepoList,
        ),
        "repo_tree" => (
            "Repository tree",
            "Read a directory tree from a Jeryu-managed gitd mirror.",
            tool_annotations(true, false, true, false),
            ToolKind::RepoTree,
        ),
        "repo_blob" => (
            "Repository blob",
            "Read a file blob from a Jeryu-managed gitd mirror.",
            tool_annotations(true, false, true, false),
            ToolKind::RepoBlob,
        ),
        "repo_search" => (
            "Repository search",
            "Search file contents in a Jeryu-managed gitd mirror.",
            tool_annotations(true, false, true, false),
            ToolKind::RepoSearch,
        ),
        "ecosystem_graph" => (
            "Ecosystem graph",
            "Read the live Jeryu ecosystem repos, tools, and relationships graph.",
            tool_annotations(true, false, true, false),
            ToolKind::EcosystemGraph,
        ),
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

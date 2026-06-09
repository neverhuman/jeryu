//! Per-tool metadata: titles, descriptions, annotations, and assembled definitions.

use super::input_schema::tool_input_schema;
use super::kind::{ToolDefinition, ToolKind};
use crate::tools::schema::tool_annotations;

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
        "workcell.claim" => (
            "Claim workcell",
            "Claim a ready workcell for an agent and startup sync.",
            tool_annotations(false, false, false, true),
            ToolKind::WorkcellClaim,
        ),
        "workcell.status" => (
            "Workcell status",
            "Read the current state of a workcell lease.",
            tool_annotations(true, false, true, false),
            ToolKind::WorkcellStatus,
        ),
        "workcell.repair_live" => (
            "Live workcell repair",
            "Hold a failed workcell tree and start live repair.",
            tool_annotations(false, false, false, true),
            ToolKind::WorkcellRepairLive,
        ),
        "workcell.export_pr" => (
            "Export repair PR",
            "Export the repair branch into a namespaced pull request.",
            tool_annotations(false, false, false, true),
            ToolKind::WorkcellExportPr,
        ),
        "workcell.release" => (
            "Release workcell",
            "Release a claimed or repairing workcell lease.",
            tool_annotations(false, false, false, true),
            ToolKind::WorkcellRelease,
        ),
        "agent_work.start" => (
            "Start agent work",
            "Start a high-level live agent run through the Jeryu API.",
            tool_annotations(false, false, false, true),
            ToolKind::AgentWorkStart,
        ),
        "agent_work.status" => (
            "Agent work status",
            "Read a high-level live agent-run snapshot.",
            tool_annotations(true, false, true, false),
            ToolKind::AgentWorkStatus,
        ),
        "agent_work.control" => (
            "Control agent work",
            "Send a live control command to a PTY-backed agent run.",
            tool_annotations(false, false, false, true),
            ToolKind::AgentWorkControl,
        ),
        "agent_work.events" => (
            "Agent work events",
            "Read cursor-safe agent-run events after a sequence number.",
            tool_annotations(true, false, true, false),
            ToolKind::AgentWorkEvents,
        ),
        "agent_work.export_pr" => (
            "Export agent work PR",
            "Export a finished workcell-backed agent run into a pull request.",
            tool_annotations(false, false, false, true),
            ToolKind::AgentWorkExportPr,
        ),
        "code.symbols.search" => (
            "Search code symbols",
            "Search indexed workspace symbols by name, crate, or file.",
            tool_annotations(true, false, true, false),
            ToolKind::CodeSymbolsSearch,
        ),
        "code.definition" => (
            "Code definition",
            "Resolve an indexed symbol definition.",
            tool_annotations(true, false, true, false),
            ToolKind::CodeDefinition,
        ),
        "code.impact" => (
            "Code impact",
            "Return codegraph impact for changed repo-relative paths.",
            tool_annotations(true, false, true, false),
            ToolKind::CodeImpact,
        ),
        "code.crate.reverse_deps" => (
            "Crate reverse dependencies",
            "Return crates that directly depend on a workspace crate.",
            tool_annotations(true, false, true, false),
            ToolKind::CodeCrateReverseDeps,
        ),
        "code.references" => (
            "Code references",
            "Return indexed references for a symbol.",
            tool_annotations(true, false, true, false),
            ToolKind::CodeReferences,
        ),
        "codegraph.query" => (
            "Codegraph query",
            "Return a provenance-bearing codegraph impact pack for a repo/ref query.",
            tool_annotations(true, false, true, false),
            ToolKind::CodegraphQuery,
        ),
        "codegraph.tool_build.status" => (
            "Tool-build status",
            "Return status for the fast repeated-code cluster index.",
            tool_annotations(true, false, true, false),
            ToolKind::CodegraphToolBuildStatus,
        ),
        "codegraph.tool_build.clusters" => (
            "Tool-build clusters",
            "Return ranked repeated-code clusters for possible Jankurai tool building.",
            tool_annotations(true, false, true, false),
            ToolKind::CodegraphToolBuildClusters,
        ),
        "codegraph.tool_build.feedback" => (
            "Tool-build feedback",
            "Record auditable ignore feedback for a repeated-code cluster.",
            tool_annotations(false, false, true, true),
            ToolKind::CodegraphToolBuildFeedback,
        ),
        "control_plane.status" => (
            "Control-plane status",
            "Return the full live Jeryu control-plane intelligence snapshot.",
            tool_annotations(true, false, true, false),
            ToolKind::ControlPlaneStatus,
        ),
        "control_plane.priorities" => (
            "Control-plane priorities",
            "Return deterministic rules-v1 priority insights with evidence.",
            tool_annotations(true, false, true, false),
            ToolKind::ControlPlanePriorities,
        ),
        "repo_graph.clusters" => (
            "Repository graph clusters",
            "Return plot-ready repository graph clusters and insights.",
            tool_annotations(true, false, true, false),
            ToolKind::RepoGraphClusters,
        ),
        "repo_graph.query" => (
            "Repository graph query",
            "Query plot-ready repository graph nodes, edges, clusters, and insights.",
            tool_annotations(true, false, true, false),
            ToolKind::RepoGraphQuery,
        ),
        "remote.status" => (
            "Remote mirror status",
            "Return optional read-only mirror state and explicit degradation evidence.",
            tool_annotations(true, false, true, false),
            ToolKind::RemoteStatus,
        ),
        "artifacts.latest" => (
            "Latest artifacts",
            "Return latest build/release artifact evidence and explicit absence states.",
            tool_annotations(true, false, true, false),
            ToolKind::ArtifactsLatest,
        ),
        "runner_fabric.status" => (
            "Runner fabric status",
            "Return local runner fabric capacity plus optional mirror runner evidence.",
            tool_annotations(true, false, true, false),
            ToolKind::RunnerFabricStatus,
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

//! Tool kinds, the per-tool definition record, and argument normalization.

use serde_json::Value;

use crate::TOOL_PREFIX;
use crate::backend::ToolDescriptor;
use crate::tools::schema::*;

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
    WorkcellClaim,
    WorkcellStatus,
    WorkcellRepairLive,
    WorkcellExportPr,
    WorkcellRelease,
    AgentWorkStart,
    AgentWorkStatus,
    AgentWorkControl,
    AgentWorkEvents,
    AgentWorkExportPr,
    CodeSymbolsSearch,
    CodeDefinition,
    CodeImpact,
    CodeCrateReverseDeps,
    CodeReferences,
    CodegraphQuery,
    CodegraphToolBuildStatus,
    CodegraphToolBuildClusters,
    CodegraphToolBuildFeedback,
    ControlPlaneStatus,
    ControlPlanePriorities,
    RepoGraphClusters,
    RepoGraphQuery,
    RemoteStatus,
    ArtifactsLatest,
    RunnerFabricStatus,
}

#[derive(Debug, Clone)]
pub(crate) struct ToolDefinition {
    pub(super) title: &'static str,
    pub(super) description: &'static str,
    pub(super) annotations: Value,
    pub(super) input_schema: Value,
    pub(super) output_schema: Value,
    pub(super) kind: ToolKind,
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
            ToolKind::WorkcellClaim => serde_json::json!({
                "agent_id": s("agent_id")?,
                "workspace_root": s("workspace_root")?,
                "repo_roots": args.get("repo_roots")?.clone(),
                "branch_budget": i("branch_budget")?,
                "runner_id": s("runner_id")?,
                "runner_epoch": i("runner_epoch")?,
                "git_status_summary": s("git_status_summary")?,
                "ci_snapshot_age_ms": args.get("ci_snapshot_age_ms").and_then(Value::as_i64),
                "startup": args.get("startup")?.clone(),
            }),
            ToolKind::WorkcellStatus => serde_json::json!({
                "workcell_id": s("workcell_id")?,
            }),
            ToolKind::WorkcellRepairLive => serde_json::json!({
                "agent_id": s("agent_id")?,
                "workspace_root": s("workspace_root")?,
                "repo_roots": args.get("repo_roots")?.clone(),
                "branch_budget": i("branch_budget")?,
                "runner_id": s("runner_id")?,
                "runner_epoch": i("runner_epoch")?,
                "git_status_summary": s("git_status_summary")?,
                "ci_snapshot_age_ms": args.get("ci_snapshot_age_ms").and_then(Value::as_i64),
                "startup": args.get("startup")?.clone(),
                "failed_run_id": s("failed_run_id")?,
                "failed_receipt_id": s("failed_receipt_id")?,
                "failure_log_digest": s("failure_log_digest")?,
            }),
            ToolKind::WorkcellExportPr => serde_json::json!({
                "workcell_id": s("workcell_id")?,
                "runner_epoch": i("runner_epoch")?,
                "branch_suffix": s("branch_suffix")?,
                "owner": s("owner")?,
                "repo": s("repo")?,
                "author": s("author")?,
                "target_branch": opt_s("target_branch"),
                "title": opt_s("title"),
                "body": opt_s("body"),
            }),
            ToolKind::WorkcellRelease => serde_json::json!({
                "workcell_id": s("workcell_id")?,
                "runner_epoch": i("runner_epoch")?,
            }),
            ToolKind::AgentWorkStart => {
                if args.get("source").is_none() || args.get("program").is_none() {
                    return None;
                }
                args.clone()
            }
            ToolKind::AgentWorkStatus => serde_json::json!({
                "agent_run_id": s("agent_run_id")?,
            }),
            ToolKind::AgentWorkControl => serde_json::json!({
                "agent_run_id": s("agent_run_id")?,
                "command": args.get("command")?.clone(),
            }),
            ToolKind::AgentWorkEvents => serde_json::json!({
                "agent_run_id": s("agent_run_id")?,
                "after_seq": args.get("after_seq").and_then(Value::as_i64),
                "limit": args.get("limit").and_then(Value::as_i64),
            }),
            ToolKind::AgentWorkExportPr => serde_json::json!({
                "agent_run_id": s("agent_run_id")?,
                "owner": s("owner")?,
                "repo": s("repo")?,
                "author": s("author")?,
                "branch_suffix": opt_s("branch_suffix"),
                "target_branch": opt_s("target_branch"),
                "title": s("title")?,
                "body": opt_s("body"),
            }),
            ToolKind::CodeSymbolsSearch => serde_json::json!({
                "query": s("query")?,
                "repo": opt_s("repo"),
                "limit": args.get("limit").and_then(Value::as_i64),
            }),
            ToolKind::CodeDefinition => serde_json::json!({
                "symbol": s("symbol")?,
                "repo": opt_s("repo"),
            }),
            ToolKind::CodeImpact => serde_json::json!({
                "changed_paths": parse_string_array(args.get("changed_paths")?)?,
                "repo": opt_s("repo"),
            }),
            ToolKind::CodeCrateReverseDeps => serde_json::json!({
                "crate_name": s("crate_name")?,
                "repo": opt_s("repo"),
            }),
            ToolKind::CodeReferences => serde_json::json!({
                "symbol": s("symbol")?,
                "repo": opt_s("repo"),
            }),
            ToolKind::CodegraphQuery => serde_json::json!({
                "repo": opt_s("repo"),
                "ref": opt_s("ref"),
                "changed_paths": args
                    .get("changed_paths")
                    .and_then(parse_string_array)
                    .unwrap_or_default(),
                "intent": opt_s("intent"),
                "question": opt_s("question"),
                "symbol": opt_s("symbol"),
                "crate_name": opt_s("crate_name"),
                "limit": args.get("limit").and_then(Value::as_i64),
                "max_tokens": args.get("max_tokens").and_then(Value::as_i64),
            }),
            ToolKind::CodegraphToolBuildStatus => serde_json::json!({
                "repo": opt_s("repo"),
            }),
            ToolKind::CodegraphToolBuildClusters => serde_json::json!({
                "repo": opt_s("repo"),
                "limit": args.get("limit").and_then(Value::as_i64),
                "include_ignored": args
                    .get("include_ignored")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }),
            ToolKind::CodegraphToolBuildFeedback => serde_json::json!({
                "cluster_id": s("cluster_id")?,
                "reason": s("reason")?,
                "ignored_by": opt_s("ignored_by").unwrap_or_else(|| "mcp".to_string()),
            }),
            ToolKind::ControlPlaneStatus => serde_json::json!({}),
            ToolKind::ControlPlanePriorities => serde_json::json!({
                "limit": args.get("limit").and_then(Value::as_i64),
            }),
            ToolKind::RepoGraphClusters => serde_json::json!({
                "cluster_kind": opt_s("cluster_kind"),
                "limit": args.get("limit").and_then(Value::as_i64),
            }),
            ToolKind::RepoGraphQuery => serde_json::json!({
                "repo": opt_s("repo"),
                "cluster_kind": opt_s("cluster_kind"),
                "query": opt_s("query"),
                "limit": args.get("limit").and_then(Value::as_i64),
            }),
            ToolKind::RemoteStatus => serde_json::json!({
                "remote": opt_s("remote"),
            }),
            ToolKind::ArtifactsLatest => serde_json::json!({
                "repo": opt_s("repo"),
            }),
            ToolKind::RunnerFabricStatus => serde_json::json!({}),
        };
        Some(out)
    }
}

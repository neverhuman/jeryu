//! Tool catalog: the static list of tool ids, descriptors, schemas, and arg normalization.

mod catalog;
mod schema;

pub use crate::backend::ToolDescriptor;
pub(crate) use catalog::tool_definition;

/// Build every catalog descriptor (used by `ToolBackend::list` impls).
pub(crate) fn catalog() -> Vec<ToolDescriptor> {
    catalog::catalog()
}

/// Static source-of-truth for the catalog (replaces the source's `action_registry::REGISTRY`
/// filtered by `Surface::Capability`). Exactly the 42 tool ids, in manifest order.
pub(crate) const CATALOG: &[&str] = &[
    "fetch_capsule",
    "get_system_snapshot",
    "get_ci_run_jobs",
    "get_ci_bottlenecks",
    "explain_blockers",
    "plan_validation",
    "run_tests",
    "propose_patch",
    "race_patches",
    "request_merge",
    "bug_submit",
    "bug_list",
    "bug_show",
    "bug_ready",
    "bug_update",
    "bug_record_attempt",
    "workcell.claim",
    "workcell.status",
    "workcell.repair_live",
    "workcell.export_pr",
    "workcell.release",
    "agent_work.start",
    "agent_work.status",
    "agent_work.control",
    "agent_work.events",
    "agent_work.export_pr",
    "code.symbols.search",
    "code.definition",
    "code.impact",
    "code.crate.reverse_deps",
    "code.references",
    "codegraph.query",
    "codegraph.tool_build.status",
    "codegraph.tool_build.clusters",
    "codegraph.tool_build.feedback",
    "control_plane.status",
    "control_plane.priorities",
    "repo_graph.clusters",
    "repo_graph.query",
    "remote.status",
    "artifacts.latest",
    "runner_fabric.status",
];

/// Return every catalog descriptor as MCP-shaped JSON for `tools/list`.
pub fn tool_manifest() -> Vec<serde_json::Value> {
    catalog::catalog()
        .iter()
        .map(ToolDescriptor::to_mcp_json)
        .collect()
}

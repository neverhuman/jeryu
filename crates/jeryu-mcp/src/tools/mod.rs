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
/// filtered by `Surface::Capability`). Exactly the 16 tool ids, in manifest order.
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
];

/// Return every catalog descriptor as MCP-shaped JSON for `tools/list`.
pub fn tool_manifest() -> Vec<serde_json::Value> {
    catalog::catalog()
        .iter()
        .map(ToolDescriptor::to_mcp_json)
        .collect()
}

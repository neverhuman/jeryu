pub mod agent_submit;
pub mod bug;
pub mod node;
pub mod exec;
pub mod git;
pub mod host;
pub mod install;
pub mod job;
pub mod pipeline;
pub mod pool;
pub mod release;
pub mod remote;
pub mod repo;
pub mod secrets;
pub mod settings;
pub mod system;
pub mod test;

/// Returns the configured default GitLab project id (`settings.release.default_project_id`).
/// Prefer `resolve_project_id` at CLI dispatch boundaries; use this only when no explicit
/// argument is available (e.g. when building an internal query without a CLI opt).
pub(crate) fn default_release_project_id() -> i64 {
    jeryu::settings::get().release.default_project_id
}

/// Resolves an optional CLI `--project-id` argument to a concrete id.
/// When the caller omits `--project-id`, the value comes from
/// `settings.release.default_project_id` (default: 48).
/// This is the single resolution point for all CLI dispatch paths.
pub(crate) fn resolve_project_id(explicit: Option<i64>) -> i64 {
    match explicit {
        Some(id) => id,
        None => default_release_project_id(),
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReleaseSettings {
    /// Path to the release artifact repository root on disk.
    /// Equivalent to the previous JERYU_RELEASE_REPO_ROOT env var.
    pub repo_root: Option<String>,
    /// Default GitLab project ID for release tracking.
    pub default_project_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SandboxSettings {
    /// Enable strict network namespace isolation in the custom executor sandbox.
    /// Equivalent to the previous JERYU_STRICT_SANDBOX env var (presence = enabled).
    pub strict_network_isolation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiSettings {
    /// Polling interval for background sync in milliseconds.
    pub sync_interval_ms: u64,
    /// Number of recent jobs to keep in the live jobs list.
    pub recent_jobs_limit: usize,
    /// Number of recent evidence records to display.
    pub recent_evidence_limit: usize,
    /// Number of audit events to keep in memory.
    pub audit_events_limit: usize,
}

impl Default for ReleaseSettings {
    fn default() -> Self {
        Self {
            repo_root: Some("/home/ubuntu/dougx".into()),
            default_project_id: 2,
        }
    }
}

impl Default for TuiSettings {
    fn default() -> Self {
        Self {
            sync_interval_ms: 5000,
            recent_jobs_limit: 50,
            recent_evidence_limit: 100,
            audit_events_limit: 50,
        }
    }
}

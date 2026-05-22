use anyhow::Result;
use std::env;
use tracing::info;

use super::support::{
    ensure_custom_executor_tools, env_bool_or_default, env_i64_or_default, env_string_or_default,
};

/// Handles `jeryu exec prepare`
/// Provisions the actual job container sandbox.
pub async fn run_prepare() -> Result<()> {
    let job_id = env_string_or_default("CUSTOM_ENV_CI_JOB_ID", "unknown");
    let project_dir = env_string_or_default("CUSTOM_ENV_CI_PROJECT_DIR", "/tmp/jeryu-job");

    info!(
        job_id,
        project_dir, "Driver: preparing custom execution sandbox"
    );

    let sandbox_path = format!("{}-sandbox", project_dir);

    if super::support::fast_clone(&project_dir, &sandbox_path).is_err() {
        let _ = std::fs::create_dir_all(&sandbox_path);
    }

    crate::honeypot::seed_sandbox(&sandbox_path);

    Ok(())
}

#[path = "stage_run.rs"]
mod stage_run;
pub use stage_run::run_stage;

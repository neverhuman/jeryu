#![doc = "Job file loading helpers."]

use jeryu_runner_core::JobRequest;
use jeryu_runner_core::error::{RunnerError, RunnerResult};
use std::fs;
use std::path::Path;

/// Load a key-value job file.
pub fn load_job_file(path: impl AsRef<Path>) -> RunnerResult<JobRequest> {
    let path = path.as_ref();
    let content = fs::read_to_string(path).map_err(|err| {
        RunnerError::new(
            "job_file_read_failed",
            format!("failed to read '{}': {err}", path.display()),
        )
    })?;
    JobRequest::from_key_value(&content)
}

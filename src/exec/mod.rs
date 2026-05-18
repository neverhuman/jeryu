//! Owner: Custom Executor & Sandbox Isolation
//! Proof: `cargo test -p jeryu -- exec`
//! Invariants: Quarantine-on-tripwire; capsule capture on failure; CAS exact-hit skip
//!
//! This module acts as the plugin interface for `gitlab-runner` when configured
//! as `executor = "custom"`. It handles the lifecycle of the actual job execution:
//! configuration, provisioning the sandbox, running the user script, and cleaning up.

mod cleanup;
mod stage;
mod stage_cache;
mod support;

pub use cleanup::*;
pub use stage::*;
pub use support::*;

use anyhow::Result;
use std::path::{Component, Path};

/// Handles `jeryu exec config`
/// Tells GitLab Runner what capabilities this driver supports.
pub fn run_config() -> Result<()> {
    let config_json = r#"{
  "builds_dir": "/builds",
  "cache_dir": "/cache",
  "builds_dir_is_shared": false,
  "driver": {
    "name": "jeryu God Mode Driver",
    "version": "1.0.0"
  }
}"#;
    // Driver config MUST be written to stdout for gitlab-runner to parse
    println!("{}", config_json);
    Ok(())
}

pub fn validate_script_path(script_path: &str) -> Result<()> {
    let path = Path::new(script_path);

    if script_path.trim().is_empty() {
        anyhow::bail!("custom executor script path must not be empty");
    }

    if path.is_absolute() {
        anyhow::bail!("custom executor script path must be relative");
    }

    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::Prefix(_) | Component::RootDir
        )
    }) {
        anyhow::bail!("custom executor script path must not traverse out of the sandbox");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_script_path;

    #[test]
    fn validate_script_path_accepts_relative_paths_only() {
        assert!(validate_script_path("build.sh").is_ok());
        assert!(validate_script_path("ci/build_script.sh").is_ok());
        assert!(validate_script_path("").is_err());
        assert!(validate_script_path("/tmp/build.sh").is_err());
        assert!(validate_script_path("../build.sh").is_err());
    }
}

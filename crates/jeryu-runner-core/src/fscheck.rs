#![doc = "Filesystem and ambient capability guard checks."]

use crate::error::{RunnerError, RunnerResult};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Host paths that must never be mounted into a job.
pub const DENIED_HOST_PATHS: &[&str] = &[
    "/var/run/docker.sock",
    "/run/docker.sock",
    "/var/run/podman/podman.sock",
    "/run/podman/podman.sock",
    "/root/.ssh",
    "/home/.ssh",
];

/// Environment variables that must never be forwarded from the host.
pub const DENIED_ENV_VARS: &[&str] = &[
    "SSH_AUTH_SOCK",
    "DOCKER_HOST",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "AWS_SESSION_TOKEN",
    "GOOGLE_APPLICATION_CREDENTIALS",
    "AZURE_CLIENT_SECRET",
];

/// Check a path against known host capability exposures.
pub fn deny_dangerous_host_path(path: &Path) -> RunnerResult<()> {
    let path_string = path.display().to_string();
    if DENIED_HOST_PATHS
        .iter()
        .any(|denied| path_string == *denied || path_string.starts_with(&format!("{denied}/")))
    {
        return Err(RunnerError::new(
            "host_path_denied",
            format!("path '{}' is not allowed in runner sandbox", path.display()),
        ));
    }
    Ok(())
}

/// Validate that all mount source paths are safe.
pub fn validate_mount_sources(paths: &[PathBuf]) -> RunnerResult<()> {
    for path in paths {
        deny_dangerous_host_path(path)?;
    }
    Ok(())
}

/// Drop denied env vars from a candidate environment and return kept vars.
pub fn sanitize_env(env: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    env.iter()
        .filter(|(key, _)| !DENIED_ENV_VARS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn denies_docker_socket() {
        let err = deny_dangerous_host_path(Path::new("/var/run/docker.sock"))
            .err()
            .unwrap_or_else(|| panic!("expected denial"));
        assert_eq!(err.code(), "host_path_denied");
    }

    #[test]
    fn denies_socket_and_ssh_path_prefixes() {
        for path in [
            "/var/run/docker.sock",
            "/run/podman/podman.sock",
            "/root/.ssh/id_rsa",
            "/home/.ssh/config",
        ] {
            let err = deny_dangerous_host_path(Path::new(path))
                .err()
                .unwrap_or_else(|| panic!("expected denial for {path}"));
            assert_eq!(err.code(), "host_path_denied");
        }
        deny_dangerous_host_path(Path::new("/tmp/jeryu-work"))
            .unwrap_or_else(|err| panic!("{err}"));
    }

    #[test]
    fn strips_ssh_agent() {
        let mut env = BTreeMap::new();
        env.insert("SSH_AUTH_SOCK".to_string(), "/tmp/agent".to_string());
        env.insert("RUST_LOG".to_string(), "info".to_string());
        let sanitized = sanitize_env(&env);
        assert!(!sanitized.contains_key("SSH_AUTH_SOCK"));
        assert_eq!(sanitized.get("RUST_LOG"), Some(&"info".to_string()));
    }

    proptest! {
        #[test]
        fn sanitize_env_never_forwards_denied_vars(value in "\\PC*") {
            let mut env = BTreeMap::new();
            for denied in DENIED_ENV_VARS {
                env.insert((*denied).to_string(), value.clone());
            }
            env.insert("RUST_LOG".to_string(), value);

            let sanitized = sanitize_env(&env);

            for denied in DENIED_ENV_VARS {
                prop_assert!(!sanitized.contains_key(*denied));
            }
            prop_assert!(sanitized.contains_key("RUST_LOG"));
        }
    }
}

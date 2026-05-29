use super::*;

#[test]
fn docker_runner_bootstrap_preserves_shared_cache_mount() {
    let script = runner_bootstrap_cmd_docker();
    assert!(!script.contains("find /cache"));
    assert!(!script.contains("rm -rf --"));
}

#[test]
fn custom_runner_bootstrap_preserves_shared_cache_mount() {
    let script = runner_bootstrap_cmd_custom();
    assert!(!script.contains("find /cache"));
    assert!(!script.contains("rm -rf --"));
    assert!(!contains_bytes(
        &script,
        &[112, 121, 116, 104, 111, 110, 51]
    ));
    assert!(!contains_bytes(&script, &[112, 121, 116, 104, 111, 110]));
    assert!(!contains_bytes(&script, &[112, 121, 51, 45, 112, 105, 112]));
}

#[test]
fn current_exe_mount_source_uses_existing_path() {
    let path = current_exe_mount_source(Ok(PathBuf::from("/tmp/jeryu")));
    assert_eq!(path, PathBuf::from("/tmp/jeryu"));
}

#[test]
fn current_exe_mount_source_falls_back_to_default() {
    let path = current_exe_mount_source(Err(std::io::Error::other("missing exe")));
    assert_eq!(path, PathBuf::from("/usr/local/bin/jeryu"));
}

#[test]
fn compose_up_targets_only_gitlab_and_vault() {
    assert_eq!(compose_up_targets(), ["gitlab", "vault"]);
}

#[test]
fn runner_manager_host_config_bounds_local_runner_resources() {
    let host_config = runner_manager_host_config(Vec::new());

    let log_config = host_config.log_config.expect("log config");
    assert_eq!(log_config.typ.as_deref(), Some("json-file"));
    let log_limits = log_config.config.expect("log limits");
    assert_eq!(log_limits.get("max-size").map(String::as_str), Some("50m"));
    assert_eq!(log_limits.get("max-file").map(String::as_str), Some("3"));

    assert_eq!(host_config.memory, Some(RUNNER_MEMORY_BYTES));
    assert_eq!(host_config.memory_swap, Some(RUNNER_MEMORY_BYTES));
    assert_eq!(host_config.nano_cpus, Some(RUNNER_NANO_CPUS));

    let nofile = host_config
        .ulimits
        .expect("ulimits")
        .into_iter()
        .find(|limit| limit.name.as_deref() == Some("nofile"))
        .expect("nofile ulimit");
    assert_eq!(nofile.soft, Some(RUNNER_NOFILE_LIMIT));
    assert_eq!(nofile.hard, Some(RUNNER_NOFILE_LIMIT));

    let restart = host_config.restart_policy.expect("restart policy");
    assert_eq!(restart.name, Some(RestartPolicyNameEnum::UNLESS_STOPPED));
}

fn contains_bytes(haystack: &str, needle: &[u8]) -> bool {
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window == needle)
}

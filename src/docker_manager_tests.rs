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

fn contains_bytes(haystack: &str, needle: &[u8]) -> bool {
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window == needle)
}

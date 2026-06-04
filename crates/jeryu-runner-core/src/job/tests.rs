use super::*;

#[test]
fn parses_key_value_job() {
    let job = JobRequest::from_key_value(
        r#"
        job_id=job_1
        repo_id=jeryu/jeryu
        commit_sha=abc123
        workspace=/tmp/jeryu-work
        command=/bin/echo
        args=hello,world
        trust_tier=T1
        requested_runner=native-rust-hot
        env.RUST_LOG=info
        "#,
    )
    .unwrap_or_else(|err| panic!("{err}"));
    assert_eq!(job.args, vec!["hello", "world"]);
    assert_eq!(job.trust_tier, TrustTier::T1ProtectedInternal);
    assert_eq!(job.requested_runner, Some(RunnerClass::NativeRustHot));
}

#[test]
fn parses_proxy_only_network_policy() {
    let job = JobRequest::from_key_value(
        r#"
        job_id=job_1
        repo_id=jeryu/jeryu
        commit_sha=abc123
        workspace=/tmp/jeryu-work
        command=/bin/echo
        network_policy=egress-proxy-only
        "#,
    )
    .unwrap_or_else(|err| panic!("{err}"));
    assert_eq!(job.network_policy, NetworkPolicy::EgressProxyOnly);
    assert_eq!(job.network_policy.as_str(), "egress-proxy-only");
}

#[test]
fn rejects_relative_workspace() {
    let err = JobRequest::from_key_value(
        r#"
        job_id=job_1
        repo_id=jeryu/jeryu
        commit_sha=abc123
        workspace=relative
        command=/bin/echo
        "#,
    )
    .err()
    .unwrap_or_else(|| panic!("expected validation failure"));
    assert_eq!(err.code(), "invalid_workspace");
}

#[test]
fn rejects_dangerous_workspace_paths() {
    let err = JobRequest::from_key_value(
        r#"
        job_id=job_1
        repo_id=jeryu/jeryu
        commit_sha=abc123
        workspace=/var/run/docker.sock
        command=/bin/echo
        "#,
    )
    .err()
    .unwrap_or_else(|| panic!("expected host path denial"));
    assert_eq!(err.code(), "host_path_denied");
    assert!(err.message().contains("/var/run/docker.sock"));
}

#[test]
fn rejects_dangerous_workspace_path_children() {
    let err = JobRequest::from_key_value(
        r#"
        job_id=job_1
        repo_id=jeryu/jeryu
        commit_sha=abc123
        workspace=/root/.ssh/id_rsa
        command=/bin/echo
        "#,
    )
    .err()
    .unwrap_or_else(|| panic!("expected host path denial"));
    assert_eq!(err.code(), "host_path_denied");
    assert!(err.message().contains("/root/.ssh/id_rsa"));
}

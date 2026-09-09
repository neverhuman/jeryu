#[path = "../../../tests/support/test_workspace.rs"]
mod test_workspace;

use jeryu_runner_core::job::{JobRequest, NetworkPolicy, SecretPolicy, TokenPolicy};
use jeryu_runner_core::policy::{CacheWritePolicy, select_runner};
use jeryu_runner_core::receipt::ReceiptStatus;
use jeryu_runner_core::trust::{RunnerClass, TrustTier};
use jeryu_runnerd::{DispatchEngine, DispatchMode};
use std::sync::Mutex;
use test_workspace::TestWorkspace;

static EXECUTION_GUARD: Mutex<()> = Mutex::new(());

fn job(tier: TrustTier) -> (TestWorkspace, JobRequest) {
    let workspace = TestWorkspace::new("phase4");
    let request = JobRequest {
        job_id: "job".to_string(),
        repo_id: "jeryu/jeryu".to_string(),
        commit_sha: "abc123".to_string(),
        workspace: workspace.path().to_path_buf(),
        command: "/bin/echo".to_string(),
        args: vec!["ok".to_string()],
        env: Default::default(),
        trust_tier: tier,
        requested_runner: None,
        network_policy: NetworkPolicy::Deny,
        secret_policy: SecretPolicy::Default,
        token_policy: TokenPolicy::ReadOnly,
        timeout_ms: 1000,
        fork: matches!(tier, TrustTier::T4ForkPr),
    };
    (workspace, request)
}

#[test]
fn untrusted_jobs_never_run_native_hot() {
    for tier in [TrustTier::T4ForkPr, TrustTier::T5PublicUntrusted] {
        let (_workspace, mut request) = job(tier);
        request.requested_runner = Some(RunnerClass::NativeRustHot);
        let err = select_runner(&request)
            .err()
            .unwrap_or_else(|| panic!("expected policy denial"));
        assert_eq!(err.code(), "runner_policy_denied");
    }
}

#[test]
fn fork_pr_secret_denial_and_cache_denial() {
    let (_workspace, request) = job(TrustTier::T4ForkPr);
    let decision = select_runner(&request).unwrap_or_else(|err| panic!("{err}"));
    assert!(!decision.allow_secrets);
    assert_eq!(decision.cache_write_policy, CacheWritePolicy::Deny);
}

#[test]
fn network_deny_by_default_for_forks() {
    let (_workspace, request) = job(TrustTier::T4ForkPr);
    let decision = select_runner(&request).unwrap_or_else(|err| panic!("{err}"));
    assert_eq!(decision.network_policy, NetworkPolicy::Deny);
}

#[test]
fn fork_pr_egress_request_is_denied() {
    let (_workspace, mut request) = job(TrustTier::T4ForkPr);
    request.network_policy = NetworkPolicy::EgressOnly;
    let err = select_runner(&request)
        .err()
        .unwrap_or_else(|| panic!("expected network denial"));
    assert_eq!(err.code(), "network_policy_denied");
}

#[test]
fn runner_crash_or_missing_command_produces_failed_receipt() {
    let _guard = EXECUTION_GUARD.lock().unwrap();
    let (_workspace, mut request) = job(TrustTier::T1ProtectedInternal);
    request.command = "/definitely/not/a/command".to_string();
    let receipt = DispatchEngine::new().dispatch(&request, DispatchMode::Run);
    assert_eq!(receipt.status, ReceiptStatus::Failed);
    assert!(receipt.message.contains("process_start_failed"));
}

#[test]
fn policy_denial_still_produces_receipt() {
    let (_workspace, mut request) = job(TrustTier::T5PublicUntrusted);
    request.requested_runner = Some(RunnerClass::NativeRustHot);
    let receipt = DispatchEngine::new().dispatch(&request, DispatchMode::Explain);
    assert_eq!(receipt.status, ReceiptStatus::Denied);
    assert_eq!(receipt.runner_class, "denied");
}

#[test]
fn trusted_native_runner_executes_smoke_command() {
    let _guard = EXECUTION_GUARD.lock().unwrap();
    let (_workspace, request) = job(TrustTier::T1ProtectedInternal);
    let receipt = DispatchEngine::new().dispatch(&request, DispatchMode::Run);
    assert_eq!(receipt.status, ReceiptStatus::Passed);
    assert_eq!(receipt.runner_class, RunnerClass::NativeRustHot.as_str());
}

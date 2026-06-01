#![doc = "Dispatch engine for jeryu_runnerd."]

mod adapter;
mod engine;

pub use adapter::protocol_to_core_job;
pub use engine::{DispatchEngine, DispatchMode, ProtocolDispatchContext};

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_ci_ir::{ArtifactPath, ArtifactWhen, CacheMode, CacheMount, Step};
    use jeryu_runner_core::JobRequest as CoreJobRequest;
    use jeryu_runner_core::job::{NetworkPolicy, SecretPolicy, TokenPolicy};
    use jeryu_runner_core::receipt::ReceiptStatus;
    use jeryu_runner_core::trust::{RunnerClass, TrustTier};
    use jeryu_runner_protocol::JobRequest as ProtocolJobRequest;
    use std::path::PathBuf;

    fn job(tier: TrustTier) -> CoreJobRequest {
        CoreJobRequest {
            job_id: "job".to_string(),
            repo_id: "repo".to_string(),
            commit_sha: "abc".to_string(),
            workspace: PathBuf::from("/tmp/work"),
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
        }
    }

    fn context(tier: TrustTier) -> ProtocolDispatchContext {
        ProtocolDispatchContext {
            repo_id: "repo".to_string(),
            commit_sha: "abc".to_string(),
            workspace: PathBuf::from("/tmp/work"),
            trust_tier: tier,
            fork: matches!(tier, TrustTier::T4ForkPr),
            network_policy: NetworkPolicy::Deny,
            secret_policy: SecretPolicy::None,
            token_policy: TokenPolicy::ReadOnly,
        }
    }

    fn protocol_request(class: jeryu_ci_ir::RunnerClass) -> ProtocolJobRequest {
        let mut request = ProtocolJobRequest::new("pipeline", "run", "lease", "job", class);
        request.assign_runner("runner-a", 1);
        request.steps.push(Step::run("s1", "test", "cargo test"));
        request
            .env
            .insert("RUST_LOG".to_string(), "info".to_string());
        request.timeout_seconds = 90;
        request
    }

    #[test]
    fn explain_native_returns_planned_receipt() {
        let receipt = DispatchEngine::new()
            .dispatch(&job(TrustTier::T1ProtectedInternal), DispatchMode::Explain);
        assert_eq!(receipt.status, ReceiptStatus::Planned);
        assert_eq!(receipt.runner_class, RunnerClass::NativeRustHot.as_str());
    }

    #[test]
    fn denied_policy_still_receipts() {
        let mut request = job(TrustTier::T5PublicUntrusted);
        request.requested_runner = Some(RunnerClass::NativeRustHot);
        let receipt = DispatchEngine::new().dispatch(&request, DispatchMode::Explain);
        assert_eq!(receipt.status, ReceiptStatus::Denied);
        assert_eq!(receipt.runner_class, "denied");
    }

    #[test]
    fn protocol_adapter_builds_core_job_from_single_run_step() {
        let mut request = protocol_request(jeryu_ci_ir::RunnerClass::NativeRustClean);
        request.steps[0]
            .env
            .insert("CARGO_TERM_COLOR".to_string(), "always".to_string());

        let job = protocol_to_core_job(&request, &context(TrustTier::T2InternalBranch))
            .unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(job.job_id, "job");
        assert_eq!(job.repo_id, "repo");
        assert_eq!(job.commit_sha, "abc");
        assert_eq!(job.command, "/bin/sh");
        assert_eq!(job.args, vec!["-lc", "cargo test"]);
        assert_eq!(job.env["RUST_LOG"], "info");
        assert_eq!(job.env["CARGO_TERM_COLOR"], "always");
        assert_eq!(job.timeout_ms, 90_000);
        assert_eq!(job.requested_runner, Some(RunnerClass::NativeRustClean));
        assert_eq!(job.trust_tier, TrustTier::T2InternalBranch);
    }

    #[test]
    fn protocol_dispatch_runs_through_runner_policy() {
        let request = protocol_request(jeryu_ci_ir::RunnerClass::NativeRustClean);
        let receipt = DispatchEngine::new()
            .try_dispatch_protocol(
                &request,
                &context(TrustTier::T2InternalBranch),
                DispatchMode::Explain,
            )
            .unwrap_or_else(|err| panic!("{err}"));

        assert_eq!(receipt.status, ReceiptStatus::Planned);
        assert_eq!(receipt.runner_class, RunnerClass::NativeRustClean.as_str());
    }

    #[test]
    fn protocol_adapter_rejects_identity_mismatch() {
        let mut request = protocol_request(jeryu_ci_ir::RunnerClass::NativeRustClean);
        request.request_id = "tampered".to_string();

        let err = protocol_to_core_job(&request, &context(TrustTier::T2InternalBranch))
            .err()
            .unwrap_or_else(|| panic!("expected adapter denial"));

        assert_eq!(err.code(), "protocol_adapter_denied");
        assert!(err.message().contains("request_id"));
    }

    #[test]
    fn protocol_adapter_rejects_missing_runner_epoch() {
        let mut request = protocol_request(jeryu_ci_ir::RunnerClass::NativeRustClean);
        request.runner_epoch = 0;

        let err = protocol_to_core_job(&request, &context(TrustTier::T2InternalBranch))
            .err()
            .unwrap_or_else(|| panic!("expected adapter denial"));

        assert_eq!(err.code(), "protocol_adapter_denied");
        assert!(err.message().contains("runner_epoch"));
    }

    #[test]
    fn protocol_adapter_rejects_multiple_steps() {
        let mut request = protocol_request(jeryu_ci_ir::RunnerClass::NativeRustClean);
        request.steps.push(Step::run("s2", "lint", "cargo clippy"));

        let err = protocol_to_core_job(&request, &context(TrustTier::T2InternalBranch))
            .err()
            .unwrap_or_else(|| panic!("expected adapter denial"));

        assert_eq!(err.code(), "protocol_adapter_denied");
        assert!(err.message().contains("exactly one"));
    }

    #[test]
    fn protocol_adapter_rejects_action_steps() {
        let mut request = protocol_request(jeryu_ci_ir::RunnerClass::NativeRustClean);
        request.steps[0] = Step::uses("s1", "checkout", "actions/checkout");

        let err = protocol_to_core_job(&request, &context(TrustTier::T2InternalBranch))
            .err()
            .unwrap_or_else(|| panic!("expected adapter denial"));

        assert_eq!(err.code(), "protocol_adapter_denied");
        assert!(err.message().contains("action-style"));
    }

    #[test]
    fn protocol_adapter_rejects_working_directory_until_core_supports_it() {
        let mut request = protocol_request(jeryu_ci_ir::RunnerClass::NativeRustClean);
        request.steps[0].working_directory = Some("crates/jeryu_runnerd".to_string());

        let err = protocol_to_core_job(&request, &context(TrustTier::T2InternalBranch))
            .err()
            .unwrap_or_else(|| panic!("expected adapter denial"));

        assert_eq!(err.code(), "protocol_adapter_denied");
        assert!(err.message().contains("working directories"));
    }

    #[test]
    fn protocol_adapter_rejects_unsupported_runner_classes() {
        for class in [
            jeryu_ci_ir::RunnerClass::CrategraphDelta,
            jeryu_ci_ir::RunnerClass::NextestCapsule,
            jeryu_ci_ir::RunnerClass::MergeSpec,
            jeryu_ci_ir::RunnerClass::K8sOci,
            jeryu_ci_ir::RunnerClass::Custom("native-rust-hot".to_string()),
            jeryu_ci_ir::RunnerClass::Custom("docker".to_string()),
        ] {
            let request = protocol_request(class);
            let err = protocol_to_core_job(&request, &context(TrustTier::T2InternalBranch))
                .err()
                .unwrap_or_else(|| panic!("expected adapter denial"));
            assert_eq!(err.code(), "protocol_adapter_denied");
        }
    }

    #[test]
    fn protocol_adapter_rejects_runner_policy_denial() {
        let request = protocol_request(jeryu_ci_ir::RunnerClass::NativeRustHot);

        let err = protocol_to_core_job(&request, &context(TrustTier::T5PublicUntrusted))
            .err()
            .unwrap_or_else(|| panic!("expected policy denial"));

        assert_eq!(err.code(), "runner_policy_denied");
    }

    #[test]
    fn protocol_adapter_rejects_dangerous_context_workspace() {
        let request = protocol_request(jeryu_ci_ir::RunnerClass::NativeRustClean);
        let mut context = context(TrustTier::T2InternalBranch);
        context.workspace = PathBuf::from("/var/run/docker.sock");

        let err = protocol_to_core_job(&request, &context)
            .err()
            .unwrap_or_else(|| panic!("expected dangerous workspace denial"));

        assert_eq!(err.code(), "host_path_denied");
        assert!(err.message().contains("not allowed in runner sandbox"));
    }

    #[test]
    fn protocol_adapter_rejects_zero_and_overflow_timeout() {
        let mut request = protocol_request(jeryu_ci_ir::RunnerClass::NativeRustClean);
        request.timeout_seconds = 0;
        let err = protocol_to_core_job(&request, &context(TrustTier::T2InternalBranch))
            .err()
            .unwrap_or_else(|| panic!("expected zero timeout denial"));
        assert_eq!(err.code(), "protocol_adapter_denied");

        request.timeout_seconds = u64::MAX;
        let err = protocol_to_core_job(&request, &context(TrustTier::T2InternalBranch))
            .err()
            .unwrap_or_else(|| panic!("expected overflow timeout denial"));
        assert_eq!(err.code(), "protocol_adapter_denied");
    }

    #[test]
    fn protocol_adapter_rejects_env_conflict_invalid_and_denied_keys() {
        let mut request = protocol_request(jeryu_ci_ir::RunnerClass::NativeRustClean);
        request.steps[0]
            .env
            .insert("RUST_LOG".to_string(), "debug".to_string());
        let err = protocol_to_core_job(&request, &context(TrustTier::T2InternalBranch))
            .err()
            .unwrap_or_else(|| panic!("expected env conflict denial"));
        assert!(err.message().contains("conflicting"));

        request = protocol_request(jeryu_ci_ir::RunnerClass::NativeRustClean);
        request
            .env
            .insert("bad-name".to_string(), "value".to_string());
        let err = protocol_to_core_job(&request, &context(TrustTier::T2InternalBranch))
            .err()
            .unwrap_or_else(|| panic!("expected invalid env denial"));
        assert!(err.message().contains("invalid environment"));

        request = protocol_request(jeryu_ci_ir::RunnerClass::NativeRustClean);
        request
            .env
            .insert("SSH_AUTH_SOCK".to_string(), "/tmp/agent.sock".to_string());
        let err = protocol_to_core_job(&request, &context(TrustTier::T2InternalBranch))
            .err()
            .unwrap_or_else(|| panic!("expected ambient env denial"));
        assert!(err.message().contains("denied"));
    }

    #[test]
    fn protocol_adapter_rejects_metadata_that_jeryu_runner_core_cannot_preserve() {
        let mut request = protocol_request(jeryu_ci_ir::RunnerClass::NativeRustClean);
        request.cache_mounts.push(CacheMount {
            name: "cargo".to_string(),
            path: "target".to_string(),
            mode: CacheMode::ReadOnly,
            fingerprint: "hash".to_string(),
        });

        let err = protocol_to_core_job(&request, &context(TrustTier::T2InternalBranch))
            .err()
            .unwrap_or_else(|| panic!("expected cache denial"));
        assert!(err.message().contains("cache mounts"));

        request.cache_mounts.clear();
        request.artifact_paths.push(ArtifactPath {
            name: "logs".to_string(),
            paths: vec!["target/logs".to_string()],
            when: ArtifactWhen::Always,
            retention_days: 1,
        });
        let err = protocol_to_core_job(&request, &context(TrustTier::T2InternalBranch))
            .err()
            .unwrap_or_else(|| panic!("expected artifact denial"));
        assert!(err.message().contains("artifact paths"));
    }
}

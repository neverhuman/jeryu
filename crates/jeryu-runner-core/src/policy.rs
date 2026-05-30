#![doc = "Runner selection and policy decisions."]

use crate::error::{RunnerError, RunnerResult};
use crate::job::{JobRequest, NetworkPolicy, SecretPolicy, TokenPolicy};
use crate::trust::{RunnerClass, TrustTier};

/// Cache write policy chosen for a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheWritePolicy {
    /// No compiled-cache write is allowed.
    Deny,
    /// Quarantine writes only; requires promotion receipt.
    Quarantine,
    /// Trusted writes allowed after protected green policy.
    TrustedAfterGreen,
}

impl CacheWritePolicy {
    /// Stable label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deny => "deny",
            Self::Quarantine => "quarantine",
            Self::TrustedAfterGreen => "trusted-after-green",
        }
    }
}

/// Full policy decision made by jeryu_runnerd before dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDecision {
    /// Selected runner class.
    pub runner_class: RunnerClass,
    /// Effective network policy.
    pub network_policy: NetworkPolicy,
    /// Whether secrets are allowed.
    pub allow_secrets: bool,
    /// Effective token policy.
    pub token_policy: TokenPolicy,
    /// Cache write policy.
    pub cache_write_policy: CacheWritePolicy,
    /// Explainable reasons for the decision.
    pub reasons: Vec<String>,
}

impl PolicyDecision {
    /// True if the decision routes to native execution.
    pub fn is_native(&self) -> bool {
        self.runner_class.is_native()
    }
}

/// Select a runner and effective policies for a job.
pub fn select_runner(job: &JobRequest) -> RunnerResult<PolicyDecision> {
    let mut reasons = vec![format!("trust_tier={}", job.trust_tier.as_str())];
    if job.fork {
        reasons.push("fork=true".to_string());
    }

    let requested = job.requested_runner;
    let runner_class = choose_runner(job, requested, &mut reasons)?;
    assert_native_invariants(job.trust_tier, runner_class)?;

    let network_policy = effective_network(job, runner_class, &mut reasons)?;
    let allow_secrets = effective_secrets(job, &mut reasons);
    let token_policy = effective_token(job, &mut reasons);
    let cache_write_policy = effective_jeryu_cache_policy(job.trust_tier, &mut reasons);

    Ok(PolicyDecision {
        runner_class,
        network_policy,
        allow_secrets,
        token_policy,
        cache_write_policy,
        reasons,
    })
}

fn choose_runner(
    job: &JobRequest,
    requested: Option<RunnerClass>,
    reasons: &mut Vec<String>,
) -> RunnerResult<RunnerClass> {
    if let Some(requested) = requested {
        validate_requested(job.trust_tier, requested)?;
        reasons.push(format!("requested_runner={}", requested.as_str()));
        return Ok(requested);
    }

    let default = match job.trust_tier {
        TrustTier::T0ReleaseHermetic => RunnerClass::ReleaseHermetic,
        TrustTier::T1ProtectedInternal => RunnerClass::NativeRustHot,
        TrustTier::T2InternalBranch => RunnerClass::NativeRustClean,
        TrustTier::T3AgentAuthored => RunnerClass::AgentGuard,
        TrustTier::T4ForkPr => RunnerClass::MicroVmRust,
        TrustTier::T5PublicUntrusted => RunnerClass::MicroVmRust,
    };
    reasons.push(format!("default_runner={}", default.as_str()));
    Ok(default)
}

fn validate_requested(tier: TrustTier, runner: RunnerClass) -> RunnerResult<()> {
    match (tier, runner) {
        (TrustTier::T4ForkPr | TrustTier::T5PublicUntrusted, RunnerClass::NativeRustHot)
        | (TrustTier::T4ForkPr | TrustTier::T5PublicUntrusted, RunnerClass::NativeRustClean)
        | (TrustTier::T4ForkPr | TrustTier::T5PublicUntrusted, RunnerClass::AgentGuard)
        | (TrustTier::T4ForkPr | TrustTier::T5PublicUntrusted, RunnerClass::ReleaseHermetic) => {
            Err(RunnerError::new(
                "runner_policy_denied",
                format!("{} cannot run on {runner}", tier.as_str()),
            ))
        }
        (TrustTier::T5PublicUntrusted, RunnerClass::OciDocker) => Err(RunnerError::new(
            "runner_policy_denied",
            "T5_PUBLIC_UNTRUSTED must use microvm-rust",
        )),
        (TrustTier::T3AgentAuthored, RunnerClass::NativeRustHot) => Err(RunnerError::new(
            "runner_policy_denied",
            "T3_AGENT_AUTHORED must use agent-guard or stronger isolation",
        )),
        _ => Ok(()),
    }
}

fn assert_native_invariants(tier: TrustTier, runner: RunnerClass) -> RunnerResult<()> {
    if runner.is_native() && !tier.permits_native() {
        return Err(RunnerError::new(
            "runner_policy_denied",
            format!(
                "{} does not permit native runner class {runner}",
                tier.as_str()
            ),
        ));
    }
    if runner == RunnerClass::NativeRustHot && tier != TrustTier::T1ProtectedInternal {
        return Err(RunnerError::new(
            "runner_policy_denied",
            format!("native-rust-hot is reserved for T1_PROTECTED_INTERNAL, got {tier}"),
        ));
    }
    Ok(())
}

fn effective_network(
    job: &JobRequest,
    runner: RunnerClass,
    reasons: &mut Vec<String>,
) -> RunnerResult<NetworkPolicy> {
    match job.trust_tier {
        TrustTier::T4ForkPr | TrustTier::T5PublicUntrusted => {
            if job.network_policy != NetworkPolicy::Deny {
                return Err(RunnerError::new(
                    "network_policy_denied",
                    "fork/public jobs are network-deny by default in Phase 4",
                ));
            }
            reasons.push("network=deny-untrusted".to_string());
            Ok(NetworkPolicy::Deny)
        }
        TrustTier::T0ReleaseHermetic => {
            if job.network_policy != NetworkPolicy::Deny {
                return Err(RunnerError::new(
                    "network_policy_denied",
                    "release-hermetic jobs require network deny",
                ));
            }
            reasons.push("network=deny-release-hermetic".to_string());
            Ok(NetworkPolicy::Deny)
        }
        TrustTier::T3AgentAuthored => {
            if job.network_policy == NetworkPolicy::EgressOnly {
                return Err(RunnerError::new(
                    "network_policy_denied",
                    "agent jobs may not request egress in Phase 4",
                ));
            }
            reasons.push(format!("network={}", job.network_policy.as_str()));
            Ok(job.network_policy)
        }
        _ => {
            if runner == RunnerClass::NativeRustHot
                && job.network_policy == NetworkPolicy::EgressOnly
            {
                reasons.push("network=egress-native-hot-explicit".to_string());
            } else {
                reasons.push(format!("network={}", job.network_policy.as_str()));
            }
            Ok(job.network_policy)
        }
    }
}

fn effective_secrets(job: &JobRequest, reasons: &mut Vec<String>) -> bool {
    let allowed = match job.secret_policy {
        SecretPolicy::None => false,
        SecretPolicy::Default => job.trust_tier.permits_default_secrets(),
        SecretPolicy::Requested => job.trust_tier.permits_default_secrets(),
    };
    reasons.push(format!("secrets_allowed={allowed}"));
    allowed
}

fn effective_token(job: &JobRequest, reasons: &mut Vec<String>) -> TokenPolicy {
    let policy = match job.trust_tier {
        TrustTier::T4ForkPr | TrustTier::T5PublicUntrusted => TokenPolicy::ReadOnly,
        TrustTier::T3AgentAuthored => match job.token_policy {
            TokenPolicy::ScopedWrite => TokenPolicy::ScopedWrite,
            _ => TokenPolicy::ReadOnly,
        },
        _ => job.token_policy,
    };
    reasons.push(format!("token_policy={}", policy.as_str()));
    policy
}

fn effective_jeryu_cache_policy(tier: TrustTier, reasons: &mut Vec<String>) -> CacheWritePolicy {
    let policy = match tier {
        TrustTier::T0ReleaseHermetic | TrustTier::T1ProtectedInternal => {
            CacheWritePolicy::TrustedAfterGreen
        }
        TrustTier::T2InternalBranch | TrustTier::T3AgentAuthored => CacheWritePolicy::Quarantine,
        TrustTier::T4ForkPr | TrustTier::T5PublicUntrusted => CacheWritePolicy::Deny,
    };
    reasons.push(format!("cache_write={}", policy.as_str()));
    policy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::JobRequest;
    use std::path::PathBuf;

    fn job(tier: TrustTier) -> JobRequest {
        JobRequest {
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

    #[test]
    fn t1_defaults_to_native_hot() {
        let decision = select_runner(&job(TrustTier::T1ProtectedInternal))
            .unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(decision.runner_class, RunnerClass::NativeRustHot);
    }

    #[test]
    fn fork_pr_defaults_to_microvm_and_no_secrets() {
        let decision =
            select_runner(&job(TrustTier::T4ForkPr)).unwrap_or_else(|err| panic!("{err}"));
        assert_eq!(decision.runner_class, RunnerClass::MicroVmRust);
        assert!(!decision.allow_secrets);
        assert_eq!(decision.cache_write_policy, CacheWritePolicy::Deny);
    }

    #[test]
    fn public_untrusted_cannot_request_oci() {
        let mut request = job(TrustTier::T5PublicUntrusted);
        request.requested_runner = Some(RunnerClass::OciDocker);
        let err = select_runner(&request)
            .err()
            .unwrap_or_else(|| panic!("expected error"));
        assert_eq!(err.code(), "runner_policy_denied");
    }
}

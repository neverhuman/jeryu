//! The dispatch engine itself: routing a job through runner policy to the
//! concrete native / micro-VM / OCI runner and producing a [`Receipt`].

use jeryu_runner_core::JobRequest as CoreJobRequest;
use jeryu_runner_core::error::RunnerResult;
use jeryu_runner_core::job::{NetworkPolicy, SecretPolicy, TokenPolicy};
use jeryu_runner_core::policy::select_runner;
use jeryu_runner_core::receipt::Receipt;
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_runner_core::trust::{RunnerClass, TrustTier};
use jeryu_runner_microvm::MicroVmRunner;
use jeryu_runner_native::NativeRunner;
use jeryu_runner_oci::OciRunner;
use jeryu_runner_protocol::JobRequest as ProtocolJobRequest;
use std::path::PathBuf;

use super::adapter::protocol_to_core_job;

/// Dispatch mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchMode {
    /// Explain policy and sandbox plan without execution.
    Explain,
    /// Execute where enabled.
    Run,
}

/// Host context required before a protocol job can enter runner policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolDispatchContext {
    /// Repository id or slug.
    pub repo_id: String,
    /// Commit SHA bound to this execution.
    pub commit_sha: String,
    /// Prepared absolute workspace path.
    pub workspace: PathBuf,
    /// Scheduler-authorized trust tier.
    pub trust_tier: TrustTier,
    /// True when the job originates from a fork.
    pub fork: bool,
    /// Scheduler-authorized network policy.
    pub network_policy: NetworkPolicy,
    /// Scheduler-authorized secret policy.
    pub secret_policy: SecretPolicy,
    /// Scheduler-authorized token policy.
    pub token_policy: TokenPolicy,
}

/// jeryu_runnerd dispatch engine.
#[derive(Debug, Default, Clone)]
pub struct DispatchEngine {
    native: NativeRunner,
    microvm: MicroVmRunner,
    oci: OciRunner,
}

impl DispatchEngine {
    /// Create a dispatch engine.
    pub fn new() -> Self {
        Self {
            native: NativeRunner::new(),
            microvm: MicroVmRunner::new(),
            oci: OciRunner::new(),
        }
    }

    /// Dispatch a job and return a receipt.
    pub fn dispatch(&self, job: &CoreJobRequest, mode: DispatchMode) -> Receipt {
        match self.try_dispatch(job, mode) {
            Ok(receipt) => receipt,
            Err(err) => Receipt::denied(job, err.to_string()),
        }
    }

    /// Convert a protocol request with host context and dispatch it.
    pub fn try_dispatch_protocol(
        &self,
        request: &ProtocolJobRequest,
        context: &ProtocolDispatchContext,
        mode: DispatchMode,
    ) -> RunnerResult<Receipt> {
        let job = protocol_to_core_job(request, context)?;
        self.try_dispatch(&job, mode)
    }

    fn try_dispatch(&self, job: &CoreJobRequest, mode: DispatchMode) -> RunnerResult<Receipt> {
        let decision = select_runner(job)?;
        let plan = SandboxPlan::from_decision(&job.workspace, &decision);
        match (mode, decision.runner_class) {
            (DispatchMode::Explain, RunnerClass::NativeRustHot)
            | (DispatchMode::Explain, RunnerClass::NativeRustClean)
            | (DispatchMode::Explain, RunnerClass::AgentGuard)
            | (DispatchMode::Explain, RunnerClass::ReleaseHermetic) => {
                self.native.plan_only(job, &decision, &plan)
            }
            (DispatchMode::Explain, RunnerClass::MicroVmRust) => {
                self.microvm.execute(job, &decision, &plan)
            }
            (DispatchMode::Explain, RunnerClass::OciDocker) => {
                self.oci.execute(job, &decision, &plan)
            }
            (DispatchMode::Run, RunnerClass::NativeRustHot)
            | (DispatchMode::Run, RunnerClass::NativeRustClean)
            | (DispatchMode::Run, RunnerClass::AgentGuard)
            | (DispatchMode::Run, RunnerClass::ReleaseHermetic) => {
                self.native.execute(job, &decision, &plan)
            }
            (DispatchMode::Run, RunnerClass::MicroVmRust) => {
                self.microvm.execute(job, &decision, &plan)
            }
            (DispatchMode::Run, RunnerClass::OciDocker) => self.oci.execute(job, &decision, &plan),
        }
    }
}

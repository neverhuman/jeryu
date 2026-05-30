//! Synthetic benchmark harness used for deterministic replay tests.

use crate::competitors::{all_competitors, all_jitforge_runners};
use crate::models::{
    stable_digest, CacheState, Competitor, JitForgeRunner, ScenarioClass, TrustTier,
};
use crate::receipt::BenchmarkReceipt;

/// Workload profile with deterministic durations.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkloadProfile {
    pub fixture: &'static str,
    pub scenario: ScenarioClass,
    pub baseline_container_ms: u64,
    pub target_speedup: f64,
    pub cache_state: CacheState,
    pub trust_tier: TrustTier,
}

impl WorkloadProfile {
    /// Build a benchmark receipt against a chosen competitor and JitForge runner.
    pub fn receipt_for(&self, competitor: Competitor, runner: JitForgeRunner) -> BenchmarkReceipt {
        let competitor_duration_ms = competitor_duration(self.baseline_container_ms, competitor);
        let jitforge_duration_ms =
            jitforge_duration(self.baseline_container_ms, self.target_speedup, runner);
        let pipeline_hash = stable_digest(&[self.fixture, self.scenario.slug(), runner.slug()]);
        BenchmarkReceipt::new(
            competitor,
            runner,
            self.fixture,
            self.scenario,
            "32c/128gb/nvme",
            self.cache_state,
            self.trust_tier,
            "abc123phase10",
            pipeline_hash,
            competitor_duration_ms,
            jitforge_duration_ms,
            0,
        )
    }
}

/// In-memory benchmark harness.
#[derive(Clone, Debug, Default)]
pub struct BenchmarkHarness {
    workloads: Vec<WorkloadProfile>,
}

impl BenchmarkHarness {
    /// Create a harness with no workloads.
    pub const fn empty() -> Self {
        Self {
            workloads: Vec::new(),
        }
    }

    /// Add a workload.
    pub fn with_workload(mut self, workload: WorkloadProfile) -> Self {
        self.workloads.push(workload);
        self
    }

    /// Workloads in this harness.
    pub fn workloads(&self) -> &[WorkloadProfile] {
        &self.workloads
    }

    /// Generate receipts for every required competitor using the Rust-native hot runner.
    pub fn provider_neutral_comparison_receipts(&self) -> Vec<BenchmarkReceipt> {
        self.workloads
            .iter()
            .flat_map(|workload| {
                all_competitors().into_iter().map(move |competitor| {
                    workload.receipt_for(competitor, JitForgeRunner::NativeRustHot)
                })
            })
            .collect()
    }

    /// Generate native-vs-OCI receipts for the neutral container baseline.
    pub fn native_vs_oci_receipts(&self) -> Vec<BenchmarkReceipt> {
        self.workloads
            .iter()
            .flat_map(|workload| {
                all_jitforge_runners().into_iter().map(move |runner| {
                    workload.receipt_for(Competitor::BaselineRunnerContainer, runner)
                })
            })
            .collect()
    }
}

/// Phase 10 benchmark harness matching the product spec win conditions.
pub fn sample_phase10_harness() -> BenchmarkHarness {
    BenchmarkHarness::empty()
        .with_workload(WorkloadProfile {
            fixture: "rust-small",
            scenario: ScenarioClass::TrustedNoOpRustPr,
            baseline_container_ms: 40_000,
            target_speedup: 10.0,
            cache_state: CacheState::WarmProjectCache,
            trust_tier: TrustTier::InternalBranch,
        })
        .with_workload(WorkloadProfile {
            fixture: "rust-small",
            scenario: ScenarioClass::SmallWarmRustPr,
            baseline_container_ms: 75_000,
            target_speedup: 5.0,
            cache_state: CacheState::WarmProjectCache,
            trust_tier: TrustTier::InternalBranch,
        })
        .with_workload(WorkloadProfile {
            fixture: "rust-medium",
            scenario: ScenarioClass::MediumAffectedCratePr,
            baseline_container_ms: 180_000,
            target_speedup: 3.2,
            cache_state: CacheState::WarmProjectCache,
            trust_tier: TrustTier::InternalBranch,
        })
        .with_workload(WorkloadProfile {
            fixture: "rust-large",
            scenario: ScenarioClass::LargeAffectedCratePr,
            baseline_container_ms: 540_000,
            target_speedup: 2.2,
            cache_state: CacheState::WarmTenantSourceCache,
            trust_tier: TrustTier::ProtectedInternal,
        })
        .with_workload(WorkloadProfile {
            fixture: "rust-large",
            scenario: ScenarioClass::TestOnlySharded,
            baseline_container_ms: 300_000,
            target_speedup: 3.1,
            cache_state: CacheState::WarmProjectCache,
            trust_tier: TrustTier::InternalBranch,
        })
        .with_workload(WorkloadProfile {
            fixture: "merge-queue",
            scenario: ScenarioClass::MergeQueueMedian,
            baseline_container_ms: 600_000,
            target_speedup: 2.4,
            cache_state: CacheState::WarmProjectCache,
            trust_tier: TrustTier::ProtectedInternal,
        })
}

fn competitor_duration(baseline_container_ms: u64, competitor: Competitor) -> u64 {
    match competitor {
        Competitor::BaselineRunnerContainer => baseline_container_ms,
        Competitor::BaselineRunnerShell => baseline_container_ms.saturating_mul(82) / 100,
        Competitor::BaselineRunnerKubernetes => baseline_container_ms.saturating_mul(118) / 100,
        Competitor::GitHubActionsSelfHosted => baseline_container_ms.saturating_mul(105) / 100,
        Competitor::GiteaActions => baseline_container_ms.saturating_mul(96) / 100,
        Competitor::ForgejoActions => baseline_container_ms.saturating_mul(95) / 100,
    }
}

fn jitforge_duration(
    baseline_container_ms: u64,
    target_speedup: f64,
    runner: JitForgeRunner,
) -> u64 {
    let native_hot = (baseline_container_ms as f64 / target_speedup).max(1.0) as u64;
    match runner {
        JitForgeRunner::NativeRustHot => native_hot,
        JitForgeRunner::NativeRustClean => native_hot.saturating_mul(125) / 100,
        JitForgeRunner::MicroVmRust => native_hot.saturating_mul(170) / 100,
        JitForgeRunner::OciDocker => native_hot.saturating_mul(260) / 100,
        JitForgeRunner::K8sOci => native_hot.saturating_mul(310) / 100,
    }
}

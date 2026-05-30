//! Competitor matrix for Phase 10 benchmarks.

use crate::models::{Competitor, JitForgeRunner};

/// All external systems required by Phase 10 benchmark replay.
pub const fn all_competitors() -> [Competitor; 6] {
    [
        Competitor::BaselineRunnerContainer,
        Competitor::BaselineRunnerShell,
        Competitor::BaselineRunnerKubernetes,
        Competitor::GitHubActionsSelfHosted,
        Competitor::GiteaActions,
        Competitor::ForgejoActions,
    ]
}

/// All JitForge runner modes included in native-vs-OCI scorecards.
pub const fn all_jitforge_runners() -> [JitForgeRunner; 5] {
    [
        JitForgeRunner::NativeRustHot,
        JitForgeRunner::NativeRustClean,
        JitForgeRunner::MicroVmRust,
        JitForgeRunner::OciDocker,
        JitForgeRunner::K8sOci,
    ]
}

/// Whether a competitor is one of the neutral baseline runner modes.
pub const fn is_baseline_runner(competitor: Competitor) -> bool {
    matches!(
        competitor,
        Competitor::BaselineRunnerContainer
            | Competitor::BaselineRunnerShell
            | Competitor::BaselineRunnerKubernetes
    )
}

/// Whether a JitForge runner is the Rust-native fast path.
pub const fn is_native_fast_path(runner: JitForgeRunner) -> bool {
    matches!(
        runner,
        JitForgeRunner::NativeRustHot | JitForgeRunner::NativeRustClean
    )
}

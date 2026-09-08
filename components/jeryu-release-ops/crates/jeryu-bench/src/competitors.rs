//! Competitor matrix for Phase 10 benchmarks.

use crate::models::{Competitor, JeryuRunner};

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

/// All Jeryu runner modes included in native-vs-OCI scorecards.
pub const fn all_jeryu_runners() -> [JeryuRunner; 5] {
    [
        JeryuRunner::NativeRustHot,
        JeryuRunner::NativeRustClean,
        JeryuRunner::MicroVmRust,
        JeryuRunner::OciDocker,
        JeryuRunner::K8sOci,
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

/// Whether a Jeryu runner is the Rust-native fast path.
pub const fn is_native_fast_path(runner: JeryuRunner) -> bool {
    matches!(
        runner,
        JeryuRunner::NativeRustHot | JeryuRunner::NativeRustClean
    )
}

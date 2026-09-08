use super::types::{AffectedPackage, CiCommand, ImpactReason, PlannerOptions, RunnerClass};
use crate::features::FeatureSelection;
use crate::sccache::TrustTier;
use std::collections::BTreeSet;

pub(super) fn is_test_path(path: &str) -> bool {
    path.starts_with("tests/")
        || path.contains("/tests/")
        || path.ends_with("_test.rs")
        || path.ends_with("_tests.rs")
}

pub(super) fn proof_lanes_for(reasons: &BTreeSet<ImpactReason>) -> BTreeSet<String> {
    let mut lanes = BTreeSet::new();
    lanes.insert("changed-fast".to_string());
    if reasons.contains(&ImpactReason::PublicApiChange) {
        lanes.insert("public-api".to_string());
    }
    if reasons.iter().any(|reason| {
        matches!(
            reason,
            ImpactReason::BuildScriptChange
                | ImpactReason::ProcMacroChange
                | ImpactReason::CargoLockChange
                | ImpactReason::NativeDependencyChange
                | ImpactReason::ManifestChange
                | ImpactReason::UnknownPathFailClosed
        )
    }) {
        lanes.insert("invalidation".to_string());
    }
    if reasons.contains(&ImpactReason::SecuritySensitiveChange) {
        lanes.insert("security".to_string());
    }
    lanes
}

pub(super) fn runner_class_for(
    options: &PlannerOptions,
    reasons: &BTreeSet<ImpactReason>,
    fail_closed: bool,
) -> RunnerClass {
    if options.release_lane || options.trust_tier == TrustTier::ReleaseHermetic {
        return RunnerClass::ReleaseHermetic;
    }
    if matches!(
        options.trust_tier,
        TrustTier::ForkPullRequest | TrustTier::PublicUntrusted
    ) {
        return RunnerClass::MicroVmRust;
    }
    if options.trust_tier == TrustTier::AgentAuthored {
        return RunnerClass::AgentGuard;
    }
    if fail_closed
        || reasons.iter().any(|reason| {
            matches!(
                reason,
                ImpactReason::SecuritySensitiveChange
                    | ImpactReason::CargoLockChange
                    | ImpactReason::BuildScriptChange
                    | ImpactReason::NativeDependencyChange
            )
        })
    {
        return RunnerClass::NativeRustClean;
    }
    RunnerClass::NativeRustHot
}

pub(super) fn commands_for(
    packages: &[AffectedPackage],
    reasons: &BTreeSet<ImpactReason>,
    features: &FeatureSelection,
) -> Vec<CiCommand> {
    let mut commands = Vec::new();
    let package_args: Vec<String> = packages
        .iter()
        .flat_map(|package| ["--package".to_string(), package.name.clone()])
        .collect();
    let feature_args = features.cargo_args();

    let mut check = vec![
        "cargo".to_string(),
        "check".to_string(),
        "--workspace".to_string(),
        "--all-targets".to_string(),
    ];
    check.extend(feature_args.clone());
    commands.push(CiCommand {
        lane: "check".to_string(),
        argv: check,
    });

    let mut nextest = vec![
        "cargo".to_string(),
        "nextest".to_string(),
        "run".to_string(),
    ];
    if package_args.is_empty() {
        nextest.push("--workspace".to_string());
    } else {
        nextest.extend(package_args);
    }
    nextest.extend(feature_args);
    commands.push(CiCommand {
        lane: "nextest".to_string(),
        argv: nextest,
    });

    if reasons.contains(&ImpactReason::PublicApiChange) {
        commands.push(CiCommand {
            lane: "semver".to_string(),
            argv: vec![
                "cargo".to_string(),
                "semver-checks".to_string(),
                "check-release".to_string(),
            ],
        });
    }
    if reasons.contains(&ImpactReason::CargoLockChange) {
        commands.push(CiCommand {
            lane: "dependency-audit".to_string(),
            argv: vec!["cargo".to_string(), "deny".to_string(), "check".to_string()],
        });
    }
    commands
}

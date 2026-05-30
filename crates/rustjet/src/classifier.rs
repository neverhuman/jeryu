use crate::changes::ChangeSet;
use crate::features::FeatureSelection;
use crate::graph::WorkspaceGraph;
use crate::manifest::PackageId;
use crate::pathset::{is_markdown, is_security_sensitive};
use crate::public_api::PublicApiDetector;
use crate::sccache::{SccachePolicy, TrustTier};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImpactReason {
    DocumentationOnly,
    PrivateImplementationChange,
    PublicApiChange,
    TestOnlyChange,
    BuildScriptChange,
    ProcMacroChange,
    CargoLockChange,
    ManifestChange,
    NativeDependencyChange,
    SecuritySensitiveChange,
    UnknownPathFailClosed,
}

impl ImpactReason {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::DocumentationOnly => "documentation-only",
            Self::PrivateImplementationChange => "private-implementation-change",
            Self::PublicApiChange => "public-api-change",
            Self::TestOnlyChange => "test-only-change",
            Self::BuildScriptChange => "build-script-change",
            Self::ProcMacroChange => "proc-macro-change",
            Self::CargoLockChange => "cargo-lock-change",
            Self::ManifestChange => "manifest-change",
            Self::NativeDependencyChange => "native-dependency-change",
            Self::SecuritySensitiveChange => "security-sensitive-change",
            Self::UnknownPathFailClosed => "unknown-path-fail-closed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerClass {
    NativeRustHot,
    NativeRustClean,
    AgentGuard,
    MicroVmRust,
    ReleaseHermetic,
}

impl RunnerClass {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NativeRustHot => "native-rust-hot",
            Self::NativeRustClean => "native-rust-clean",
            Self::AgentGuard => "agent-guard",
            Self::MicroVmRust => "microvm-rust",
            Self::ReleaseHermetic => "release-hermetic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectedPackage {
    pub name: PackageId,
    pub reasons: BTreeSet<ImpactReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CiCommand {
    pub lane: String,
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectedPlan {
    pub affected_packages: Vec<AffectedPackage>,
    pub reasons: BTreeSet<ImpactReason>,
    pub proof_lanes: BTreeSet<String>,
    pub commands: Vec<CiCommand>,
    pub runner_class: RunnerClass,
    pub sccache_mode: String,
    pub fail_closed: bool,
}

impl AffectedPlan {
    #[must_use]
    pub fn affected_package_names(&self) -> BTreeSet<String> {
        self.affected_packages
            .iter()
            .map(|package| package.name.clone())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannerOptions {
    pub trust_tier: TrustTier,
    pub feature_selection: FeatureSelection,
    pub release_lane: bool,
}

impl Default for PlannerOptions {
    fn default() -> Self {
        Self {
            trust_tier: TrustTier::InternalBranch,
            feature_selection: FeatureSelection::default(),
            release_lane: false,
        }
    }
}

pub struct AffectedPlanner<'a> {
    graph: &'a WorkspaceGraph,
    public_api_detector: PublicApiDetector,
}

impl<'a> AffectedPlanner<'a> {
    #[must_use]
    pub fn new(graph: &'a WorkspaceGraph) -> Self {
        Self {
            graph,
            public_api_detector: PublicApiDetector::new(),
        }
    }

    #[must_use]
    pub fn plan(&self, changes: &ChangeSet, options: &PlannerOptions) -> AffectedPlan {
        let mut package_reasons: BTreeMap<PackageId, BTreeSet<ImpactReason>> = BTreeMap::new();
        let mut global_reasons = BTreeSet::new();
        let mut fail_closed = false;

        if changes.is_empty() {
            self.mark_all(&mut package_reasons, ImpactReason::UnknownPathFailClosed);
            global_reasons.insert(ImpactReason::UnknownPathFailClosed);
            fail_closed = true;
        }

        for changed in changes.paths() {
            let path = &changed.path;
            if path == "Cargo.lock" {
                self.mark_all(&mut package_reasons, ImpactReason::CargoLockChange);
                global_reasons.insert(ImpactReason::CargoLockChange);
                continue;
            }
            if is_security_sensitive(path) {
                self.mark_all(&mut package_reasons, ImpactReason::SecuritySensitiveChange);
                global_reasons.insert(ImpactReason::SecuritySensitiveChange);
                fail_closed = true;
                continue;
            }
            if is_markdown(path) {
                global_reasons.insert(ImpactReason::DocumentationOnly);
                continue;
            }

            let Some(package) = self.graph.package_for_path(path) else {
                self.mark_all(&mut package_reasons, ImpactReason::UnknownPathFailClosed);
                global_reasons.insert(ImpactReason::UnknownPathFailClosed);
                fail_closed = true;
                continue;
            };

            let inside = package.path_inside_package(path).unwrap_or(path);
            if inside == "Cargo.toml" {
                self.mark_with_reverse(
                    &mut package_reasons,
                    &package.name,
                    ImpactReason::ManifestChange,
                    true,
                );
                global_reasons.insert(ImpactReason::ManifestChange);
                continue;
            }
            if inside == "build.rs"
                || inside.starts_with("native/")
                || inside.starts_with("vendor/native/")
            {
                let reason = if inside == "build.rs" {
                    ImpactReason::BuildScriptChange
                } else {
                    ImpactReason::NativeDependencyChange
                };
                self.mark_with_reverse(&mut package_reasons, &package.name, reason.clone(), true);
                global_reasons.insert(reason);
                continue;
            }
            if package.is_proc_macro && inside.ends_with(".rs") {
                self.mark_with_reverse(
                    &mut package_reasons,
                    &package.name,
                    ImpactReason::ProcMacroChange,
                    true,
                );
                global_reasons.insert(ImpactReason::ProcMacroChange);
                continue;
            }
            if is_test_path(inside) {
                self.mark_one(
                    &mut package_reasons,
                    &package.name,
                    ImpactReason::TestOnlyChange,
                );
                global_reasons.insert(ImpactReason::TestOnlyChange);
                continue;
            }
            if self.public_api_detector.detect(package, inside).is_some() {
                self.mark_with_reverse(
                    &mut package_reasons,
                    &package.name,
                    ImpactReason::PublicApiChange,
                    true,
                );
                global_reasons.insert(ImpactReason::PublicApiChange);
                continue;
            }
            if inside.ends_with(".rs") {
                self.mark_with_reverse(
                    &mut package_reasons,
                    &package.name,
                    ImpactReason::PrivateImplementationChange,
                    false,
                );
                global_reasons.insert(ImpactReason::PrivateImplementationChange);
                continue;
            }

            self.mark_one(
                &mut package_reasons,
                &package.name,
                ImpactReason::PrivateImplementationChange,
            );
            global_reasons.insert(ImpactReason::PrivateImplementationChange);
        }

        if package_reasons.is_empty() && global_reasons.contains(&ImpactReason::DocumentationOnly) {
            return self.docs_only_plan(options);
        }

        let mut affected_packages: Vec<_> = package_reasons
            .into_iter()
            .map(|(name, reasons)| AffectedPackage { name, reasons })
            .collect();
        affected_packages.sort_by(|a, b| a.name.cmp(&b.name));

        let proof_lanes = proof_lanes_for(&global_reasons);
        let runner_class = runner_class_for(options, &global_reasons, fail_closed);
        let sccache = SccachePolicy::default().decide(options.trust_tier, options.release_lane);
        let commands = commands_for(
            &affected_packages,
            &global_reasons,
            &options.feature_selection,
        );

        AffectedPlan {
            affected_packages,
            reasons: global_reasons,
            proof_lanes,
            commands,
            runner_class,
            sccache_mode: sccache.mode.as_str().to_string(),
            fail_closed,
        }
    }

    fn docs_only_plan(&self, options: &PlannerOptions) -> AffectedPlan {
        let mut reasons = BTreeSet::new();
        reasons.insert(ImpactReason::DocumentationOnly);
        let mut proof_lanes = BTreeSet::new();
        proof_lanes.insert("docs".to_string());
        let sccache = SccachePolicy::default().decide(options.trust_tier, options.release_lane);
        AffectedPlan {
            affected_packages: Vec::new(),
            reasons,
            proof_lanes,
            commands: vec![CiCommand {
                lane: "docs".to_string(),
                argv: vec!["cargo".to_string(), "test".to_string(), "--doc".to_string()],
            }],
            runner_class: RunnerClass::NativeRustHot,
            sccache_mode: sccache.mode.as_str().to_string(),
            fail_closed: false,
        }
    }

    fn mark_one(
        &self,
        package_reasons: &mut BTreeMap<PackageId, BTreeSet<ImpactReason>>,
        package: &str,
        reason: ImpactReason,
    ) {
        package_reasons
            .entry(package.to_string())
            .or_default()
            .insert(reason);
    }

    fn mark_all(
        &self,
        package_reasons: &mut BTreeMap<PackageId, BTreeSet<ImpactReason>>,
        reason: ImpactReason,
    ) {
        for package in self.graph.package_names() {
            package_reasons
                .entry(package)
                .or_default()
                .insert(reason.clone());
        }
    }

    fn mark_with_reverse(
        &self,
        package_reasons: &mut BTreeMap<PackageId, BTreeSet<ImpactReason>>,
        package: &str,
        reason: ImpactReason,
        transitive: bool,
    ) {
        self.mark_one(package_reasons, package, reason.clone());
        let reverse = if transitive {
            self.graph.transitive_reverse_dependencies_of(package)
        } else {
            self.graph.direct_reverse_dependencies_of(package)
        };
        for dependent in reverse {
            package_reasons
                .entry(dependent)
                .or_default()
                .insert(reason.clone());
        }
    }
}

fn is_test_path(path: &str) -> bool {
    path.starts_with("tests/")
        || path.contains("/tests/")
        || path.ends_with("_test.rs")
        || path.ends_with("_tests.rs")
}

fn proof_lanes_for(reasons: &BTreeSet<ImpactReason>) -> BTreeSet<String> {
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

fn runner_class_for(
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

fn commands_for(
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

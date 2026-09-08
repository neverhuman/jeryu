mod derive;
mod types;

pub use types::{
    AffectedPackage, AffectedPlan, CiCommand, ImpactReason, PlannerOptions, RunnerClass,
};

use crate::changes::ChangeSet;
use crate::graph::WorkspaceGraph;
use crate::manifest::PackageId;
use crate::pathset::{is_markdown, is_security_sensitive};
use crate::public_api::PublicApiDetector;
use crate::sccache::SccachePolicy;
use std::collections::{BTreeMap, BTreeSet};

use derive::{commands_for, is_test_path, proof_lanes_for, runner_class_for};

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

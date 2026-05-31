use crate::features::FeatureSelection;
use crate::manifest::PackageId;
use crate::sccache::TrustTier;
use std::collections::BTreeSet;

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

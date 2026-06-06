#![doc = "Sandbox policy model for native, microVM, and OCI runners."]

use crate::job::NetworkPolicy;
use crate::policy::{CacheWritePolicy, PolicyDecision};
use crate::trust::RunnerClass;
use std::path::PathBuf;

/// cgroups v2 resource limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CgroupLimits {
    /// Maximum memory in bytes.
    pub memory_max_bytes: u64,
    /// CPU weight in cgroups v2 scale.
    pub cpu_weight: u16,
    /// Maximum process count.
    pub pids_max: u32,
    /// IO weight in cgroups v2 scale.
    pub io_weight: u16,
}

impl CgroupLimits {
    /// Conservative default limits for Phase 4 CI jobs.
    pub fn default_for(class: RunnerClass) -> Self {
        match class {
            RunnerClass::NativeRustHot => Self {
                memory_max_bytes: 8 * 1024 * 1024 * 1024,
                cpu_weight: 200,
                pids_max: 2048,
                io_weight: 200,
            },
            RunnerClass::NativeRustClean | RunnerClass::AgentGuard => Self {
                memory_max_bytes: 6 * 1024 * 1024 * 1024,
                cpu_weight: 150,
                pids_max: 1536,
                io_weight: 150,
            },
            RunnerClass::ReleaseHermetic => Self {
                memory_max_bytes: 8 * 1024 * 1024 * 1024,
                cpu_weight: 100,
                pids_max: 1024,
                io_weight: 100,
            },
            RunnerClass::MicroVmRust | RunnerClass::OciDocker => Self {
                memory_max_bytes: 4 * 1024 * 1024 * 1024,
                cpu_weight: 100,
                pids_max: 1024,
                io_weight: 100,
            },
        }
    }
}

/// Seccomp profile identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeccompProfile {
    /// Stable profile name.
    pub name: String,
    /// Default action.
    pub default_action: String,
    /// Explicitly allowed syscall groups.
    pub allow_groups: Vec<String>,
}

impl SeccompProfile {
    /// Default seccomp policy for a runner class.
    pub fn default_for(class: RunnerClass) -> Self {
        let mut groups = vec![
            "process-basic".to_string(),
            "file-readwrite-workspace".to_string(),
            "futex".to_string(),
            "time".to_string(),
        ];
        if matches!(
            class,
            RunnerClass::NativeRustHot | RunnerClass::NativeRustClean
        ) {
            groups.push("rust-build-tooling".to_string());
        }
        Self {
            name: format!("{}-phase4-seccomp", class.as_str()),
            default_action: "kill-process".to_string(),
            allow_groups: groups,
        }
    }
}

/// Landlock rule for a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandlockRule {
    /// Path to restrict/allow.
    pub path: PathBuf,
    /// Read access.
    pub read: bool,
    /// Write access.
    pub write: bool,
    /// Execute access.
    pub execute: bool,
}

/// Mount entry visible to a sandbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountSpec {
    /// Source path on host.
    pub source: PathBuf,
    /// Target path inside sandbox.
    pub target: PathBuf,
    /// True if mounted read-only.
    pub read_only: bool,
}

/// Complete sandbox plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxPlan {
    /// Runner class.
    pub runner_class: RunnerClass,
    /// Whether to create a user namespace.
    pub user_namespace: bool,
    /// Whether to create a mount namespace.
    pub mount_namespace: bool,
    /// Whether to create a PID namespace.
    pub pid_namespace: bool,
    /// Effective network policy.
    pub network_policy: NetworkPolicy,
    /// cgroups v2 limits.
    pub cgroup_limits: CgroupLimits,
    /// seccomp policy.
    pub seccomp: SeccompProfile,
    /// Landlock rules.
    pub landlock_rules: Vec<LandlockRule>,
    /// Mount plan.
    pub mounts: Vec<MountSpec>,
    /// Secret exposure allowed.
    pub allow_secrets: bool,
    /// Cache write policy.
    pub cache_write_policy: CacheWritePolicy,
    /// When `true`, the job MUST run under enforced cgroup-v2 limits: if no
    /// delegated cgroup-v2 subtree is available, the sandbox resolves to
    /// `Unavailable` and refuses to launch (fail-closed). Defaults to `false`
    /// so existing CI/build jobs and the escape suite degrade exactly as before.
    pub require_cgroup: bool,
}

impl SandboxPlan {
    /// Build a safe default sandbox plan from a policy decision.
    pub fn from_decision(workspace: impl Into<PathBuf>, decision: &PolicyDecision) -> Self {
        let workspace = workspace.into();
        let runner_class = decision.runner_class;
        let mut landlock_rules = vec![LandlockRule {
            path: workspace.clone(),
            read: true,
            write: true,
            execute: true,
        }];
        landlock_rules.push(LandlockRule {
            path: PathBuf::from("/nix/store"),
            read: true,
            write: false,
            execute: true,
        });
        landlock_rules.push(LandlockRule {
            path: PathBuf::from("/usr"),
            read: true,
            write: false,
            execute: true,
        });
        landlock_rules.push(LandlockRule {
            path: PathBuf::from("/dev/null"),
            read: true,
            write: true,
            execute: false,
        });
        for path in local_toolchain_roots() {
            landlock_rules.push(LandlockRule {
                path,
                read: true,
                write: false,
                execute: true,
            });
        }

        let mut mounts = vec![
            MountSpec {
                source: workspace.clone(),
                target: PathBuf::from("/workspace"),
                read_only: false,
            },
            MountSpec {
                source: PathBuf::from("/usr"),
                target: PathBuf::from("/usr"),
                read_only: true,
            },
        ];
        mounts.extend(local_toolchain_roots().into_iter().map(|path| MountSpec {
            source: path.clone(),
            target: path,
            read_only: true,
        }));

        Self {
            runner_class,
            user_namespace: true,
            mount_namespace: true,
            pid_namespace: true,
            network_policy: decision.network_policy,
            cgroup_limits: CgroupLimits::default_for(runner_class),
            seccomp: SeccompProfile::default_for(runner_class),
            landlock_rules,
            mounts,
            allow_secrets: decision.allow_secrets,
            cache_write_policy: decision.cache_write_policy,
            // Default OFF: a plain build/CI job degrades (does not fail closed)
            // when cgroups cannot be enforced. Agent jobs opt IN explicitly via
            // [`SandboxPlan::with_require_cgroup`].
            require_cgroup: false,
        }
    }

    /// Builder: require enforced cgroup-v2 limits for this job.
    ///
    /// When set to `true`, [`crate::sandbox::SandboxPlan`] callers that go
    /// through the capability resolver get `EnforcementLevel::Unavailable` (and
    /// thus a refused launch) on any host lacking a delegated cgroup-v2 subtree,
    /// rather than the default degraded-skip. Agent drivers set this so a code
    /// generator can never run without real memory/pids confinement.
    #[must_use]
    pub fn with_require_cgroup(mut self, require: bool) -> Self {
        self.require_cgroup = require;
        self
    }

    /// Render a compact explain string for receipts and logs.
    pub fn explain(&self) -> String {
        format!(
            "runner={} net={} secrets={} cache={} seccomp={}",
            self.runner_class.as_str(),
            self.network_policy.as_str(),
            self.allow_secrets,
            self.cache_write_policy.as_str(),
            self.seccomp.name
        )
    }
}

fn local_toolchain_roots() -> Vec<PathBuf> {
    [
        "/home/ubuntu/.cargo/bin",
        "/home/ubuntu/.rustup",
        "/home/ubuntu/.local/bin",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::NetworkPolicy;
    use crate::policy::{CacheWritePolicy, PolicyDecision};
    use crate::trust::RunnerClass;
    use std::path::Path;

    fn decision(runner_class: RunnerClass) -> PolicyDecision {
        PolicyDecision {
            runner_class,
            network_policy: NetworkPolicy::Deny,
            allow_secrets: false,
            token_policy: crate::job::TokenPolicy::ReadOnly,
            cache_write_policy: CacheWritePolicy::Deny,
            reasons: vec!["test".to_string()],
        }
    }

    #[test]
    fn default_native_plan_has_fail_closed_kernel_contract() {
        let workspace = PathBuf::from("/tmp/jeryu-work");
        let plan = SandboxPlan::from_decision(&workspace, &decision(RunnerClass::NativeRustClean));

        assert!(plan.user_namespace);
        assert!(plan.mount_namespace);
        assert!(plan.pid_namespace);
        assert_eq!(plan.seccomp.default_action, "kill-process");
        assert!(
            plan.seccomp
                .allow_groups
                .iter()
                .any(|group| group == "process-basic")
        );
        assert!(
            plan.seccomp
                .allow_groups
                .iter()
                .any(|group| group == "file-readwrite-workspace")
        );
        assert!(plan.cgroup_limits.memory_max_bytes > 0);
        assert!(plan.cgroup_limits.cpu_weight > 0);
        assert!(plan.cgroup_limits.pids_max > 0);
        assert!(plan.cgroup_limits.io_weight > 0);
    }

    #[test]
    fn require_cgroup_defaults_off_and_builder_sets_it() {
        let workspace = PathBuf::from("/tmp/jeryu-work");
        let plan = SandboxPlan::from_decision(&workspace, &decision(RunnerClass::NativeRustClean));
        // Default OFF so existing CI/build jobs + the escape suite are unchanged.
        assert!(!plan.require_cgroup);

        let strict = plan.clone().with_require_cgroup(true);
        assert!(strict.require_cgroup);
        // The builder flips only this flag; the rest of the plan is untouched.
        assert_eq!(strict.runner_class, plan.runner_class);
        assert_eq!(strict.landlock_rules, plan.landlock_rules);
    }

    #[test]
    fn default_plan_limits_writes_to_workspace() {
        let workspace = PathBuf::from("/tmp/jeryu-work");
        let plan = SandboxPlan::from_decision(&workspace, &decision(RunnerClass::NativeRustClean));

        let workspace_rule = plan
            .landlock_rules
            .iter()
            .find(|rule| rule.path == workspace)
            .unwrap_or_else(|| panic!("expected workspace Landlock rule"));
        assert!(workspace_rule.read);
        assert!(workspace_rule.write);
        assert!(workspace_rule.execute);

        for system_path in ["/usr", "/nix/store"] {
            let rule = plan
                .landlock_rules
                .iter()
                .find(|rule| rule.path == Path::new(system_path))
                .unwrap_or_else(|| panic!("expected {system_path} Landlock rule"));
            assert!(rule.read);
            assert!(!rule.write);
            assert!(rule.execute);
        }

        for tool_path in [
            "/home/ubuntu/.cargo/bin",
            "/home/ubuntu/.rustup",
            "/home/ubuntu/.local/bin",
        ] {
            let rule = plan
                .landlock_rules
                .iter()
                .find(|rule| rule.path == Path::new(tool_path))
                .unwrap_or_else(|| panic!("expected {tool_path} Landlock rule"));
            assert!(rule.read);
            assert!(!rule.write);
            assert!(rule.execute);
            let mount = plan
                .mounts
                .iter()
                .find(|mount| mount.source == Path::new(tool_path))
                .unwrap_or_else(|| panic!("expected {tool_path} mount"));
            assert!(mount.read_only);
        }

        let dev_null_rule = plan
            .landlock_rules
            .iter()
            .find(|rule| rule.path == Path::new("/dev/null"))
            .unwrap_or_else(|| panic!("expected /dev/null Landlock rule"));
        assert!(dev_null_rule.read);
        assert!(dev_null_rule.write);
        assert!(!dev_null_rule.execute);

        let usr_mount = plan
            .mounts
            .iter()
            .find(|mount| mount.source == Path::new("/usr"))
            .unwrap_or_else(|| panic!("expected /usr mount"));
        assert_eq!(usr_mount.target, PathBuf::from("/usr"));
        assert!(usr_mount.read_only);
    }
}

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

        let mounts = vec![
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
        }
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

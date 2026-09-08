use super::*;
use jeryu_runner_core::job::NetworkPolicy;
use jeryu_runner_core::policy::{CacheWritePolicy, PolicyDecision};
use jeryu_runner_core::trust::RunnerClass;

fn plan() -> SandboxPlan {
    let decision = PolicyDecision {
        runner_class: RunnerClass::NativeRustClean,
        network_policy: NetworkPolicy::Deny,
        allow_secrets: false,
        token_policy: jeryu_runner_core::job::TokenPolicy::ReadOnly,
        cache_write_policy: CacheWritePolicy::Deny,
        reasons: vec!["test".to_string()],
    };
    SandboxPlan::from_decision("/tmp/jeryu-cap-test", &decision)
}

/// A plan that REQUIRES enforced cgroups (agent-job posture).
fn strict_plan() -> SandboxPlan {
    plan().with_require_cgroup(true)
}

#[test]
fn probe_is_self_consistent() {
    let caps = SandboxCapabilities::probe();
    // no_new_privs is the only primitive we *require* to exist on a sane
    // Linux box; everything else is host-dependent and may be false here.
    // We do not assert it true (CI may differ) but the summary must render.
    let _ = caps.summary();
    // Landlock ABI, when present, must be a positive version.
    if let Some(abi) = caps.landlock_abi {
        assert!(abi >= 1, "landlock abi must be >= 1 when reported");
    }
}

#[test]
fn missing_no_new_privs_is_unavailable() {
    let caps = SandboxCapabilities {
        user_namespace: false,
        mount_namespace: false,
        pid_namespace: false,
        landlock_abi: Some(4),
        seccomp_bpf: true,
        cgroup_v2_subtree: Some(PathBuf::from("/sys/fs/cgroup/x")),
        no_new_privs: false,
    };
    assert!(matches!(
        caps.enforcement_level(&plan()),
        EnforcementLevel::Unavailable { .. }
    ));
}

#[test]
fn all_primitives_present_is_enforced() {
    let caps = SandboxCapabilities {
        user_namespace: true,
        mount_namespace: true,
        pid_namespace: true,
        landlock_abi: Some(4),
        seccomp_bpf: true,
        cgroup_v2_subtree: Some(PathBuf::from("/sys/fs/cgroup/x")),
        no_new_privs: true,
    };
    assert_eq!(caps.enforcement_level(&plan()), EnforcementLevel::Enforced);
}

#[test]
fn require_cgroup_without_subtree_is_unavailable_fail_closed() {
    // THIS host's posture: no delegated cgroup subtree, but landlock/seccomp
    // present and no_new_privs available. A require_cgroup plan MUST fail
    // closed (Unavailable), never degrade.
    let caps = SandboxCapabilities {
        user_namespace: false,
        mount_namespace: false,
        pid_namespace: false,
        landlock_abi: Some(4),
        seccomp_bpf: true,
        cgroup_v2_subtree: None,
        no_new_privs: true,
    };
    match caps.enforcement_level(&strict_plan()) {
        EnforcementLevel::Unavailable { reason } => {
            assert!(
                reason.contains("cgroup"),
                "reason should name cgroups, got {reason:?}"
            );
        }
        other => panic!("require_cgroup must fail closed, got {other:?}"),
    }
    // A non-require_cgroup plan on the SAME caps must only DEGRADE (cgroup_v2
    // named missing), proving the gate is opt-in and does not break CI jobs.
    match caps.enforcement_level(&plan()) {
        EnforcementLevel::Degraded { missing } => {
            assert!(missing.contains(&"cgroup_v2".to_string()));
        }
        other => panic!("non-require plan must degrade, got {other:?}"),
    }
}

#[test]
fn require_cgroup_with_subtree_is_enforced() {
    // When a delegated subtree DOES exist, a require_cgroup plan enforces.
    let caps = SandboxCapabilities {
        user_namespace: true,
        mount_namespace: true,
        pid_namespace: true,
        landlock_abi: Some(4),
        seccomp_bpf: true,
        cgroup_v2_subtree: Some(PathBuf::from("/sys/fs/cgroup/x")),
        no_new_privs: true,
    };
    assert_eq!(
        caps.enforcement_level(&strict_plan()),
        EnforcementLevel::Enforced
    );
}

#[test]
fn blocked_userns_degrades_with_named_missing() {
    // Mirrors THIS host: no userns, but landlock/seccomp/cgroup present.
    let caps = SandboxCapabilities {
        user_namespace: false,
        mount_namespace: false,
        pid_namespace: false,
        landlock_abi: Some(4),
        seccomp_bpf: true,
        cgroup_v2_subtree: Some(PathBuf::from("/sys/fs/cgroup/x")),
        no_new_privs: true,
    };
    match caps.enforcement_level(&plan()) {
        EnforcementLevel::Degraded { missing } => {
            assert!(missing.contains(&"user_namespace".to_string()));
            assert!(missing.contains(&"mount_namespace".to_string()));
            assert!(missing.contains(&"pid_namespace".to_string()));
            assert!(!missing.contains(&"landlock".to_string()));
            assert!(!missing.contains(&"seccomp".to_string()));
            assert!(!missing.contains(&"cgroup_v2".to_string()));
        }
        other => panic!("expected degraded, got {other:?}"),
    }
}

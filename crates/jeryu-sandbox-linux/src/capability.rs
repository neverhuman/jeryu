//! Real, probe-based detection of the kernel sandbox primitives that are
//! actually enforceable on THIS host right now.
//!
//! Every field is set from a real syscall / throwaway-child attempt, never from
//! a sysctl alone: the host's `apparmor_restrict_unprivileged_userns=1` lies by
//! omission (the sysctl `unprivileged_userns_clone` reads `1` yet `unshare`
//! still fails when writing `uid_map`), so we attempt the operation and observe
//! the kernel's verdict.

use jeryu_runner_core::sandbox::SandboxPlan;
use std::path::PathBuf;
use std::process::Command;

/// Snapshot of the kernel sandbox primitives available to the current,
/// unprivileged process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxCapabilities {
    /// `CLONE_NEWUSER` works for an unprivileged process (uid_map writable).
    pub user_namespace: bool,
    /// `CLONE_NEWNS` (mount namespace) can be unshared.
    pub mount_namespace: bool,
    /// `CLONE_NEWPID` (pid namespace) can be unshared.
    pub pid_namespace: bool,
    /// Landlock ABI version reported by the kernel, if Landlock is enabled.
    pub landlock_abi: Option<i32>,
    /// seccomp-bpf filters can be installed (with `no_new_privs` set).
    pub seccomp_bpf: bool,
    /// A writable, controller-delegated cgroups-v2 subtree was found.
    pub cgroup_v2_subtree: Option<PathBuf>,
    /// `PR_SET_NO_NEW_PRIVS` can be set.
    pub no_new_privs: bool,
}

/// Resolved enforcement posture for a concrete [`SandboxPlan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnforcementLevel {
    /// All primitives the plan requires are available and will be applied.
    Enforced,
    /// Some required primitives are missing; the named ones will be skipped
    /// honestly (never silently faked).
    Degraded {
        /// Human-readable names of the missing-but-degradable primitives.
        missing: Vec<String>,
    },
    /// A non-negotiable primitive (`no_new_privs`) is unavailable; the sandbox
    /// MUST NOT run a job because it cannot fail closed.
    Unavailable {
        /// Why the sandbox cannot be applied at all.
        reason: String,
    },
}

impl EnforcementLevel {
    /// Stable label for receipts and the enforcement.json artifact.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Enforced => "enforced",
            Self::Degraded { .. } => "degraded",
            Self::Unavailable { .. } => "unavailable",
        }
    }
}

impl SandboxCapabilities {
    /// Probe the host with real throwaway-child syscalls.
    ///
    /// This is intentionally cheap enough to run once and cache. The namespace
    /// probes fork a child that attempts the unshare and reports back through
    /// its exit status, so a denied unshare never disturbs the parent.
    pub fn probe() -> Self {
        Self {
            user_namespace: probe_unshare(libc::CLONE_NEWUSER),
            mount_namespace: probe_unshare(libc::CLONE_NEWNS),
            pid_namespace: probe_unshare(libc::CLONE_NEWPID),
            landlock_abi: probe_landlock_abi(),
            seccomp_bpf: probe_seccomp(),
            cgroup_v2_subtree: probe_cgroup_subtree(),
            no_new_privs: probe_no_new_privs(),
        }
    }

    /// Resolve the enforcement level for a concrete plan.
    ///
    /// `no_new_privs` is non-negotiable: without it neither seccomp nor a
    /// fail-closed posture can be guaranteed, so its absence is `Unavailable`.
    /// Everything the plan asks for but the kernel denies is reported as a
    /// degraded-skip so the caller can decide whether to proceed.
    pub fn enforcement_level(&self, plan: &SandboxPlan) -> EnforcementLevel {
        if !self.no_new_privs {
            return EnforcementLevel::Unavailable {
                reason: "PR_SET_NO_NEW_PRIVS unavailable: cannot fail closed".to_string(),
            };
        }

        // cgroups: the plan always carries nonzero limits. If the plan REQUIRES
        // enforced cgroups (agent jobs) and no delegated subtree exists, we fail
        // CLOSED — the resolver returns Unavailable and `spawn_sandboxed` refuses
        // to launch, with no launch-path change. Plain build/CI jobs keep the
        // historical degraded-skip below.
        if plan.require_cgroup && self.cgroup_v2_subtree.is_none() {
            return EnforcementLevel::Unavailable {
                reason: "cgroup limits required for this job but no delegated cgroup-v2 subtree is available".into(),
            };
        }

        let mut missing = Vec::new();

        // cgroups: the plan always carries nonzero limits; if no delegated
        // subtree exists we cannot enforce them.
        if self.cgroup_v2_subtree.is_none() {
            missing.push("cgroup_v2".to_string());
        }
        // Landlock: the plan always carries workspace rules.
        if !plan.landlock_rules.is_empty() && self.landlock_abi.is_none() {
            missing.push("landlock".to_string());
        }
        // seccomp: the plan default action is fail-closed; without seccomp-bpf
        // we cannot install it.
        if !self.seccomp_bpf {
            missing.push("seccomp".to_string());
        }
        // Namespaces are requested by the plan but the host blocks them.
        if plan.user_namespace && !self.user_namespace {
            missing.push("user_namespace".to_string());
        }
        if plan.mount_namespace && !self.mount_namespace {
            missing.push("mount_namespace".to_string());
        }
        if plan.pid_namespace && !self.pid_namespace {
            missing.push("pid_namespace".to_string());
        }

        if missing.is_empty() {
            EnforcementLevel::Enforced
        } else {
            EnforcementLevel::Degraded { missing }
        }
    }

    /// Compact one-line summary for receipts and logs.
    pub fn summary(&self) -> String {
        format!(
            "userns={} mountns={} pidns={} landlock={} seccomp={} cgroup_v2={} no_new_privs={}",
            self.user_namespace,
            self.mount_namespace,
            self.pid_namespace,
            self.landlock_abi
                .map(|abi| format!("abi{abi}"))
                .unwrap_or_else(|| "none".to_string()),
            self.seccomp_bpf,
            self.cgroup_v2_subtree.is_some(),
            self.no_new_privs,
        )
    }
}

/// Fork a throwaway child that attempts `unshare(flags)` and report whether it
/// succeeded. For `CLONE_NEWUSER` the kernel only fully commits once `uid_map`
/// is writable, so the child writes it too; an unprivileged userns that is
/// blocked by AppArmor fails exactly here.
fn probe_unshare(flags: i32) -> bool {
    // SAFETY-NOTE: `unshare` only mutates the calling thread's namespaces. We
    // run it in a throwaway child (via Command/std fork machinery) so the parent
    // is never affected. We use a tiny helper binary path through /proc to keep
    // the child minimal: a fork()+unshare()+_exit() done through nix.
    use nix::sched::{CloneFlags, unshare};
    use nix::sys::wait::{WaitStatus, waitpid};
    use nix::unistd::{ForkResult, fork};

    let clone_flags = CloneFlags::from_bits_truncate(flags);

    // SAFETY: the child performs only async-signal-safe operations (unshare,
    // a single write to a /proc file, and _exit). It never returns into Rust
    // teardown, so no allocator or destructor races with the parent.
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            let code = if unshare(clone_flags).is_err() {
                1
            } else if flags & libc::CLONE_NEWUSER != 0 {
                // Confirm the userns is usable by writing the identity uid_map.
                if std::fs::write("/proc/self/uid_map", b"0 0 1\n").is_ok() {
                    0
                } else {
                    2
                }
            } else {
                0
            };
            // SAFETY: _exit avoids running parent destructors / flushing shared
            // buffers from the forked child.
            unsafe { libc::_exit(code) };
        }
        Ok(ForkResult::Parent { child }) => {
            matches!(waitpid(child, None), Ok(WaitStatus::Exited(_, 0)))
        }
        Err(_) => false,
    }
}

/// Ask the kernel for the Landlock ABI version via `landlock_create_ruleset`
/// with the `LANDLOCK_CREATE_RULESET_VERSION` flag. Returns `None` when
/// Landlock is unsupported or disabled (`ENOSYS`/`EOPNOTSUPP`).
fn probe_landlock_abi() -> Option<i32> {
    use landlock::ABI;

    // The landlock crate exposes a best-effort runtime probe through the
    // ruleset compatibility machinery; we mirror its raw syscall to obtain the
    // concrete ABI integer for the receipt. LANDLOCK_CREATE_RULESET_VERSION = 1.
    const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1 << 0;
    // SAFETY: passing a null ruleset attr with the VERSION flag is the kernel's
    // documented ABI-query form; it has no side effects and returns the version.
    let abi = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<libc::c_void>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    if abi <= 0 {
        return None;
    }
    // Clamp to the highest ABI the linked landlock crate understands so callers
    // never request access bits the crate cannot express.
    let max_known = ABI::V6 as i64;
    Some(abi.min(max_known) as i32)
}

/// Detect whether a seccomp-bpf filter can be installed by installing a
/// permissive (allow-all) filter in a throwaway child. Installing seccomp
/// requires `PR_SET_NO_NEW_PRIVS` for unprivileged processes, so the child sets
/// it first.
fn probe_seccomp() -> bool {
    use nix::sys::wait::{WaitStatus, waitpid};
    use nix::unistd::{ForkResult, fork};
    use seccompiler::{
        BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
        SeccompRule, TargetArch, apply_filter,
    };
    use std::collections::BTreeMap;

    let arch = match std::env::consts::ARCH {
        "x86_64" => TargetArch::x86_64,
        "aarch64" => TargetArch::aarch64,
        "riscv64" => TargetArch::riscv64,
        _ => return false,
    };

    // Build a benign filter that still installs a real BPF program. seccompiler
    // rejects identical match/mismatch actions, so we allow-by-default and add a
    // single harmless conditional rule (write to fd 0 -> Log) just to make the
    // program well-formed. This proves the kernel accepts seccomp(2) without
    // restricting the probe child's behavior.
    let benign_rule = SeccompCondition::new(
        0,
        SeccompCmpArgLen::Dword,
        SeccompCmpOp::Eq,
        u64::MAX, // never-true: no fd equals u64::MAX
    )
    .and_then(|cond| SeccompRule::new(vec![cond]));
    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();
    if let Ok(rule) = benign_rule {
        rules.insert(libc::SYS_write, vec![rule]);
    }

    // SAFETY: the child only sets no_new_privs, installs the benign filter,
    // then _exit()s. No shared parent state is touched.
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            // SAFETY: prctl(PR_SET_NO_NEW_PRIVS) is async-signal-safe.
            let nnp = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
            if nnp != 0 {
                unsafe { libc::_exit(1) };
            }
            // Default allow, matched rule logs: distinct actions, real program.
            let filter = SeccompFilter::new(rules, SeccompAction::Allow, SeccompAction::Log, arch);
            let code = match filter.and_then(BpfProgram::try_from) {
                Ok(prog) => {
                    if apply_filter(&prog).is_ok() {
                        0
                    } else {
                        2
                    }
                }
                Err(_) => 3,
            };
            // SAFETY: _exit is async-signal-safe and ends the child immediately.
            unsafe { libc::_exit(code) };
        }
        Ok(ForkResult::Parent { child }) => {
            matches!(waitpid(child, None), Ok(WaitStatus::Exited(_, 0)))
        }
        Err(_) => false,
    }
}

/// Find a writable cgroups-v2 subtree that already has (or can be granted) the
/// `memory` and `pids` controllers via delegation.
///
/// The current process's own cgroup (`/proc/self/cgroup`) is frequently a
/// read-only `session-N.scope`. On a systemd user session the actually-delegated
/// tree is the SIBLING `user@<uid>.service` under the same `user-<uid>.slice`,
/// NOT an ancestor of the session scope — so a pure leaf->root walk misses it.
/// We therefore probe both the ancestor chain AND the user-manager service path
/// derived from the `user-<uid>.slice` ancestor, and pick the first directory we
/// can really create a child under.
fn probe_cgroup_subtree() -> Option<PathBuf> {
    const MOUNT: &str = "/sys/fs/cgroup";
    let rel = current_cgroup_rel()?;
    let needed = ["memory", "pids"];

    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1. The ancestor chain of our own leaf cgroup (deepest first below).
    let mut acc = PathBuf::from(MOUNT);
    let mut ancestors = vec![acc.clone()];
    for component in rel.trim_matches('/').split('/') {
        if component.is_empty() {
            continue;
        }
        acc.push(component);
        ancestors.push(acc.clone());
        // 2. Sibling user-manager service: when we pass a `user-<uid>.slice`,
        //    its delegated `user@<uid>.service` child is the real writable tree.
        if let Some(uid) = component
            .strip_suffix(".slice")
            .and_then(|slice| slice.strip_prefix("user-"))
        {
            candidates.push(acc.join(format!("user@{uid}.service")));
        }
    }
    // Prefer the deepest ancestor first, then the user-manager service paths.
    candidates.extend(ancestors.into_iter().rev());

    for dir in candidates {
        if !cgroup_has_controllers(&dir, &needed) {
            continue;
        }
        // The load-bearing test is not "can I mkdir" but "can a child process
        // actually JOIN a leaf here" — cgroup-v2 delegation lets us create
        // directories under a tree whose ancestor is NOT delegated, yet refuses
        // the process migration (EACCES) because moving a process needs write on
        // the common ancestor's cgroup.procs. On a systemd session this is
        // exactly the trap: `user@<uid>.service` is writable for mkdir but a
        // process pinned in `session-N.scope` cannot migrate into it. We must
        // report cgroup enforcement as available ONLY when the join succeeds.
        if cgroup_subtree_is_enforceable(&dir) {
            return Some(dir);
        }
    }
    None
}

fn current_cgroup_rel() -> Option<String> {
    let content = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    // cgroup-v2 line is `0::<path>`.
    content
        .lines()
        .find_map(|line| line.strip_prefix("0::").map(|p| p.to_string()))
}

fn cgroup_has_controllers(dir: &std::path::Path, needed: &[&str]) -> bool {
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_default();
    let available: String = format!(
        "{} {}",
        read("cgroup.controllers"),
        read("cgroup.subtree_control")
    );
    let tokens: Vec<&str> = available.split_whitespace().collect();
    needed.iter().all(|c| tokens.contains(c))
}

/// Prove a subtree can actually ENFORCE limits by (1) creating a leaf cgroup
/// under it and (2) having a throwaway child migrate itself into that leaf. Only
/// when the child's write to `cgroup.procs` succeeds do we trust the subtree:
/// directory mode bits and a successful mkdir are NOT sufficient evidence.
fn cgroup_subtree_is_enforceable(dir: &std::path::Path) -> bool {
    use nix::sys::wait::{WaitStatus, waitpid};
    use nix::unistd::{ForkResult, fork};

    // Ensure controllers are delegated to children before we create a leaf.
    let _ = std::fs::write(dir.join("cgroup.subtree_control"), b"+pids +memory");

    let leaf = dir.join(format!("jeryu-cap-probe-{}.scope", std::process::id()));
    let _ = std::fs::remove_dir(&leaf);
    if std::fs::create_dir(&leaf).is_err() {
        return false;
    }
    let procs = leaf.join("cgroup.procs");

    // SAFETY: child only writes its pid to cgroup.procs and _exit()s.
    let joined = match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            let ok = std::fs::write(&procs, std::process::id().to_string()).is_ok();
            // SAFETY: _exit is async-signal-safe and ends the child immediately.
            unsafe { libc::_exit(if ok { 0 } else { 1 }) };
        }
        Ok(ForkResult::Parent { child }) => {
            matches!(waitpid(child, None), Ok(WaitStatus::Exited(_, 0)))
        }
        Err(_) => false,
    };

    // The child has exited, so the leaf is empty again and can be removed.
    let _ = std::fs::remove_dir(&leaf);
    joined
}

/// `PR_SET_NO_NEW_PRIVS` is process-global and irreversible, so we probe it in
/// a throwaway child rather than poisoning the runner process itself.
fn probe_no_new_privs() -> bool {
    use nix::sys::wait::{WaitStatus, waitpid};
    use nix::unistd::{ForkResult, fork};

    // SAFETY: the child only calls prctl + _exit, both async-signal-safe.
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            // SAFETY: prctl(PR_SET_NO_NEW_PRIVS) and _exit are async-signal-safe.
            let rc = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
            unsafe { libc::_exit(if rc == 0 { 0 } else { 1 }) };
        }
        Ok(ForkResult::Parent { child }) => {
            matches!(waitpid(child, None), Ok(WaitStatus::Exited(_, 0)))
        }
        Err(_) => false,
    }
}

/// Run a tiny real `cargo build` of a throwaway crate under the supplied
/// `prepare` closure (applied in the child via `pre_exec`) and report whether
/// the toolchain survived. Used by the seccomp toolchain-survival gate so a
/// too-strict allowlist can never silently break real builds.
///
/// Returns `Ok(true)` when the build exits zero, `Ok(false)` when the build was
/// killed/failed (e.g. the filter denied a needed syscall), and `Err` only when
/// the harness itself could not be set up.
pub fn toolchain_survives_build<F>(prepare: F) -> std::io::Result<bool>
where
    F: Fn() -> std::io::Result<()> + Send + Sync + 'static,
{
    use std::io::Write;
    use std::os::unix::process::CommandExt;

    let dir = std::env::temp_dir().join(format!(
        "jeryu-toolchain-probe-{}-{}",
        std::process::id(),
        jeryu_runner_core::receipt::now_ms()
    ));
    std::fs::create_dir_all(dir.join("src"))?;
    {
        let mut cargo = std::fs::File::create(dir.join("Cargo.toml"))?;
        cargo.write_all(
            b"[package]\nname = \"probe\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[[bin]]\nname = \"probe\"\npath = \"src/main.rs\"\n",
        )?;
        let mut main = std::fs::File::create(dir.join("src/main.rs"))?;
        main.write_all(b"fn main() { println!(\"ok\"); }\n")?;
    }

    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--offline")
        .current_dir(&dir)
        .env("CARGO_TARGET_DIR", dir.join("target"));
    // Apply the same primitive the real launch path would, inside the child.
    // SAFETY: `prepare` is documented to perform only async-signal-safe work.
    unsafe {
        cmd.pre_exec(prepare);
    }
    let status = cmd.status();
    let _ = std::fs::remove_dir_all(&dir);
    Ok(status.map(|s| s.success()).unwrap_or(false))
}

#[cfg(test)]
mod tests {
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
}

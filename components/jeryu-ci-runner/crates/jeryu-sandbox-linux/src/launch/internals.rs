//! Private fork-safe sandbox payload construction and child setup.

use super::cgroup::create_cgroup;
use super::pty::wire_pty_slave;
use super::*;

/// Build the fork-safe payload in the parent: create the cgroup, precompile the
/// seccomp BPF program, and capture Landlock rules + ABI.
pub(super) fn build_payload(
    plan: &SandboxPlan,
    caps: &SandboxCapabilities,
) -> SandboxResult<SandboxPayload> {
    let cgroup_procs = match &caps.cgroup_v2_subtree {
        Some(parent) => Some(create_cgroup(
            parent,
            &plan.cgroup_limits,
            plan.require_cgroup,
        )?),
        None => None,
    };

    let landlock = match caps.landlock_abi {
        Some(abi) if !plan.landlock_rules.is_empty() => Some(LandlockPayload {
            abi,
            rules: plan.landlock_rules.clone(),
        }),
        _ => None,
    };

    let seccomp_bpf = if caps.seccomp_bpf {
        Some(
            compile_seccomp(plan)
                .map_err(|err| SandboxError::new("seccomp_compile_failed", err.to_string()))?,
        )
    } else {
        None
    };

    Ok(SandboxPayload {
        cgroup_procs,
        apply_user_ns: plan.user_namespace && caps.user_namespace,
        apply_mount_ns: plan.mount_namespace && caps.mount_namespace,
        apply_pid_ns: plan.pid_namespace && caps.pid_namespace,
        landlock,
        seccomp_bpf,
        pty_slave_fd: None,
        rlimits: RlimitFallback {
            memory_max_bytes: plan.cgroup_limits.memory_max_bytes,
        },
    })
}

/// Compile the seccomp allowlist into a BPF program in the parent (allocates),
/// so `pre_exec` only has to call the no-alloc `apply_filter`.
fn compile_seccomp(
    plan: &SandboxPlan,
) -> Result<seccompiler::BpfProgram, Box<dyn std::error::Error>> {
    use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, TargetArch};

    let arch = match std::env::consts::ARCH {
        "x86_64" => TargetArch::x86_64,
        "aarch64" => TargetArch::aarch64,
        "riscv64" => TargetArch::riscv64,
        other => return Err(format!("unsupported arch for seccomp: {other}").into()),
    };

    // Default action: deny by error, NOT kill. The plan's stated default is
    // "kill-process", but an Errno(EPERM) default keeps the toolchain alive when
    // it probes an optional syscall while still blocking the escape vectors we
    // explicitly do not allow (AF_INET sockets are blocked by argument match).
    // The escape suite proves AF_INET is actually denied.
    let rules = crate::seccomp_rules::build_rules(&plan.seccomp.allow_groups, arch)?;

    let filter = SeccompFilter::new(
        rules,
        // mismatch (syscall not in allowlist) -> EPERM, survivable by tooling.
        SeccompAction::Errno(libc::EPERM as u32),
        // match (syscall in allowlist) -> allow.
        SeccompAction::Allow,
        arch,
    )?;
    Ok(BpfProgram::try_from(filter)?)
}

/// The body that runs inside the forked child. MUST stay fail-closed: any error
/// returned here aborts the spawn before exec.
pub(super) fn apply_in_child(payload: &SandboxPayload) -> std::io::Result<()> {
    // 1. Session / process-group setup for the watchdog's group-kill. A PTY
    //    child needs its own SESSION (setsid) to acquire a controlling tty; a
    //    piped child just needs its own process group (setpgid). Either way the
    //    child becomes a group leader, so the watchdog's kill(-pgid) reaps it.
    if payload.pty_slave_fd.is_some() {
        // SAFETY: setsid() creates a new session + process group led by this
        // process; no pointer args, no shared state, async-signal-safe.
        if unsafe { libc::setsid() } == -1 {
            return Err(IoError::last_os_error());
        }
    } else {
        // SAFETY: setpgid(0, 0) only moves the calling process into a new process
        // group; it touches no shared state and is async-signal-safe.
        if unsafe { libc::setpgid(0, 0) } != 0 {
            return Err(IoError::last_os_error());
        }
    }

    // 1b. PTY: make the slave our controlling terminal and wire 0/1/2 to it.
    //     Runs before seccomp (step 6), so TIOCSCTTY/dup2 are unrestricted setup.
    if let Some(slave) = payload.pty_slave_fd {
        wire_pty_slave(slave)?;
    }

    // 2. Join the cgroup so limits bind before exec. Writing our pid to
    //    cgroup.procs migrates us. Failure here is fail-closed: if the plan
    //    expected cgroup enforcement and we cannot join, refuse to exec.
    if let Some(procs) = &payload.cgroup_procs {
        let pid = std::process::id().to_string();
        write_proc_file(procs, pid.as_bytes())?;
    }

    // 2b. setrlimit fallback (best-effort, NON-fatal): a backstop for memory and
    //     process count when cgroups are unavailable or only partially applied.
    //     This is NOT a substitute for cgroups — the fail-closed gate in
    //     capability.rs is the real protection for agent jobs — so a failed
    //     setrlimit never aborts the spawn.
    apply_rlimit_fallback(&payload.rlimits);

    // 3. PR_SET_NO_NEW_PRIVS — always, non-negotiable.
    // SAFETY: prctl(PR_SET_NO_NEW_PRIVS, 1, ...) sets a per-thread flag with no
    // pointer args; always safe and async-signal-safe.
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err(IoError::last_os_error());
    }

    // 4. Namespaces — only where caps allowed (already gated in build_payload).
    let mut clone_flags = 0;
    if payload.apply_user_ns {
        clone_flags |= libc::CLONE_NEWUSER;
    }
    if payload.apply_mount_ns {
        clone_flags |= libc::CLONE_NEWNS;
    }
    if payload.apply_pid_ns {
        clone_flags |= libc::CLONE_NEWPID;
    }
    // SAFETY: unshare() only detaches the calling thread's namespaces for the
    // requested (capability-gated) flags; no pointer args, no shared state.
    if clone_flags != 0 && unsafe { libc::unshare(clone_flags) } != 0 {
        return Err(IoError::last_os_error());
    }

    // 5. Landlock — workspace-only-writable ruleset.
    if let Some(landlock) = &payload.landlock {
        apply_landlock(landlock).map_err(|err| IoError::new(ErrorKind::PermissionDenied, err))?;
    }

    // 6. seccomp — last, so our own setup syscalls above were unrestricted.
    if let Some(bpf) = &payload.seccomp_bpf {
        seccompiler::apply_filter(bpf)
            .map_err(|err| IoError::new(ErrorKind::PermissionDenied, err.to_string()))?;
    }

    Ok(())
}

/// Best-effort `setrlimit` backstop applied inside the forked child.
///
/// This is a *fallback*, not a replacement for cgroups: the real protection for
/// agent jobs is the fail-closed gate in [`crate::capability::SandboxCapabilities::enforcement_level`],
/// which refuses to launch a `require_cgroup` job on a host without a delegated
/// cgroup-v2 subtree. Where a job is allowed to run degraded (cgroups missing
/// but not required), the address-space rlimit still caps the most egregious
/// memory balloons. Failures are swallowed: a too-tight limit must never break a
/// degraded-but-legitimate job, and this syscall (`prlimit64`) is already in the
/// seccomp baseline.
///
/// `RLIMIT_AS` (address-space) is notoriously hostile to V8/Node and JIT runtimes
/// that reserve huge virtual ranges they never fault in, so we apply a GENEROUS
/// multiple of the cgroup memory ceiling rather than the ceiling itself — the
/// goal is to stop an unbounded balloon, not to mirror the exact RSS cap. We also
/// skip `RLIMIT_AS` entirely if the multiple would overflow.
///
/// Do NOT mirror `pids.max` with `RLIMIT_NPROC`: that limit is per real Unix
/// user, not per sandbox. On a busy shared runner account it can make `/bin/sh`
/// unable to fork before the job starts. PID containment is enforced by cgroups
/// when the job requires it; non-agent degraded CI must remain runnable.
fn apply_rlimit_fallback(limits: &RlimitFallback) {
    // Address space: 4x the cgroup memory ceiling, clamped to avoid overflow and
    // to stay clear of breaking JIT/V8 virtual reservations. Skipped if zero or
    // if the headroom math overflows.
    if let Some(as_limit) = limits.memory_max_bytes.checked_mul(4)
        && as_limit > 0
    {
        set_one_rlimit(libc::RLIMIT_AS, as_limit);
    }
}

/// Set a single soft+hard rlimit, ignoring failure (best-effort).
fn set_one_rlimit(resource: libc::__rlimit_resource_t, value: u64) {
    // Clamp to rlim_t to avoid truncation surprises where rlim_t is narrower
    // than u64; `unwrap_or(MAX)` saturates instead of overflowing.
    let v = libc::rlim_t::try_from(value).unwrap_or(libc::rlim_t::MAX);
    let limit = libc::rlimit {
        rlim_cur: v,
        rlim_max: v,
    };
    // SAFETY: `setrlimit` reads a pointer to the fully-initialized stack-local
    // `limit`, mutates only this process's resource limits, and is async-signal-safe
    // for the fork/exec window; the result is intentionally ignored (best-effort).
    unsafe { libc::setrlimit(resource, &limit) };
}

/// Apply the Landlock ruleset inside the child. Opening the path FDs here is a
/// pure syscall; the landlock crate allocates internally, but the child is
/// single-threaded between fork and exec so this is the established safe pattern.
fn apply_landlock(payload: &LandlockPayload) -> Result<(), String> {
    use landlock::{
        ABI, Access, AccessFs, BitFlags, PathBeneath, PathFd, Ruleset, RulesetAttr,
        RulesetCreatedAttr,
    };

    let abi = match payload.abi {
        1 => ABI::V1,
        2 => ABI::V2,
        3 => ABI::V3,
        4 => ABI::V4,
        5 => ABI::V5,
        _ => ABI::V6,
    };

    let mut ruleset = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))
        .map_err(|e| e.to_string())?
        .create()
        .map_err(|e| e.to_string())?;

    for rule in &payload.rules {
        // Compute the access bits this rule grants, honoring read/write/execute
        // independently so an exec-only or read-only-NO-exec rule is expressed
        // faithfully.
        //
        // LATENT-BUG FIX: the previous code derived access from read/write only
        // and NEVER consulted `rule.execute`. On Landlock ABI >= 2 `Execute` is a
        // distinct access right; worse, this landlock crate's `from_read` bundles
        // `Execute` into the read set, so a `read: true, execute: false` rule
        // would WRONGLY permit exec, and an exec-only rule was silently dropped by
        // the old `!read && !write` skip. We now build the set explicitly and mask
        // `Execute` out unless the rule grants it.
        let mut access: BitFlags<AccessFs> = BitFlags::empty();
        if rule.read {
            access |= AccessFs::from_read(abi);
        }
        if rule.write {
            access |= AccessFs::from_write(abi);
        }
        // `Execute` is part of `from_read` in this crate, so reconcile it against
        // the rule's explicit execute bit: add it for exec-granting rules (gated
        // on the ABI that defines a separate execute right) and remove it from a
        // read rule that does not grant exec.
        if rule.execute && abi as i32 >= ABI::V2 as i32 {
            access |= AccessFs::Execute;
        } else if !rule.execute {
            access &= !BitFlags::from(AccessFs::Execute);
        }
        // A rule that grants no access at all yields no rule rather than an
        // empty PathBeneath the kernel would reject.
        if access.is_empty() {
            continue;
        }
        // A non-existent path simply yields no rule rather than failing the
        // whole ruleset (e.g. /nix/store may be absent on this host).
        let Ok(fd) = PathFd::new(&rule.path) else {
            continue;
        };
        ruleset = ruleset
            .add_rule(PathBeneath::new(fd, access))
            .map_err(|e| e.to_string())?;
    }

    ruleset.restrict_self().map_err(|e| e.to_string())?;
    Ok(())
}

/// Write to a `/proc` or cgroup control file, returning a useful error.
fn write_proc_file(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
    file.write_all(data)
}

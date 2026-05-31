//! Translate the [`jeryu_runner_core::sandbox::SeccompProfile`] allow-group
//! names into a concrete seccomp allowlist.
//!
//! The default action installed by [`crate::launch`] is `Errno(EPERM)` for any
//! syscall NOT in this map, so the allowlist must be broad enough that a real
//! `cargo build` / `rustc` invocation survives, yet still leave the escape
//! vectors closed:
//!
//! * `socket(2)` is allowlisted ONLY for local domains (`AF_UNIX`, `AF_NETLINK`),
//!   so an `AF_INET` socket attempt returns `EPERM` even though `socket` appears
//!   in the table. This is what the escape suite proves.
//! * everything not derived from a group name in the plan is denied by default.
//!
//! Syscall numbers come from `libc::SYS_*` so the map is explicit and arch-aware
//! via the linked libc, rather than relying on a private name table.

use seccompiler::{
    SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter, SeccompRule, TargetArch,
};
use std::collections::BTreeMap;

/// `AF_INET` / `AF_INET6` are the network domains we must keep closed; the
/// escape suite asserts an `AF_INET` socket is denied.
const AF_UNIX: u64 = libc::AF_UNIX as u64;
const AF_NETLINK: u64 = libc::AF_NETLINK as u64;

type RuleMap = BTreeMap<i64, Vec<SeccompRule>>;

/// Build the rule map for the given allow-group names. Unknown group names are
/// ignored (forward-compatible) rather than fatal.
pub fn build_rules(
    groups: &[String],
    _arch: TargetArch,
) -> Result<RuleMap, Box<dyn std::error::Error>> {
    let mut allowed: Vec<i64> = Vec::new();
    let mut conditional: RuleMap = BTreeMap::new();

    // Baseline that EVERY native profile needs to run any ELF at all and let the
    // runtime tear down cleanly. Without these the dynamic loader cannot even
    // reach `main`.
    push_all(&mut allowed, BASELINE);
    // Legacy / arch-specific syscalls that glibc and the cargo/tokio runtime use
    // on x86_64 (bare `open`/`stat`/`mkdir`/`rename`/`unlink`, `epoll_wait`,
    // `dup2`, `vfork`, ...). Gated so other arches that lack these numbers still
    // compile. These were derived from an `strace -f` of a real `cargo build`.
    push_all(&mut allowed, LEGACY_ARCH);

    for group in groups {
        match group.as_str() {
            "process-basic" => push_all(&mut allowed, PROCESS_BASIC),
            "file-readwrite-workspace" => push_all(&mut allowed, FILE_RW),
            "futex" => push_all(&mut allowed, FUTEX_GROUP),
            "time" => push_all(&mut allowed, TIME_GROUP),
            "rust-build-tooling" => {
                push_all(&mut allowed, RUST_BUILD);
                // cargo/rustc legitimately use local sockets (jobserver,
                // AF_UNIX). Allow `socket` ONLY for local domains; AF_INET is
                // denied by falling through to the default EPERM.
                add_local_socket_rule(&mut conditional)?;
            }
            _ => { /* forward-compatible: ignore unknown groups */ }
        }
    }

    allowed.sort_unstable();
    allowed.dedup();

    // Unconditional allows become empty-condition rules (match => allow).
    let mut rules: RuleMap = conditional;
    for nr in allowed {
        rules.entry(nr).or_default(); // empty Vec<SeccompRule> == unconditional
    }
    Ok(rules)
}

/// Add a conditional `socket` rule: allow only `AF_UNIX` and `AF_NETLINK`
/// (arg0). Any other domain (notably `AF_INET`/`AF_INET6`) misses the rule and
/// hits the default `EPERM`.
fn add_local_socket_rule(rules: &mut RuleMap) -> Result<(), Box<dyn std::error::Error>> {
    let mut socket_rules = Vec::new();
    for domain in [AF_UNIX, AF_NETLINK] {
        socket_rules.push(SeccompRule::new(vec![SeccompCondition::new(
            0, // arg0 == domain
            SeccompCmpArgLen::Dword,
            SeccompCmpOp::Eq,
            domain,
        )?])?);
    }
    rules.insert(libc::SYS_socket, socket_rules);
    Ok(())
}

fn push_all(target: &mut Vec<i64>, src: &[i64]) {
    target.extend_from_slice(src);
}

/// Convenience: compile a complete fail-closed filter from group names. Used by
/// the toolchain-survival test and by [`crate::launch`].
pub fn compile(
    groups: &[String],
    arch: TargetArch,
) -> Result<seccompiler::BpfProgram, Box<dyn std::error::Error>> {
    use seccompiler::{BpfProgram, SeccompAction};
    let rules = build_rules(groups, arch)?;
    let filter = SeccompFilter::new(
        rules,
        SeccompAction::Errno(libc::EPERM as u32),
        SeccompAction::Allow,
        arch,
    )?;
    Ok(BpfProgram::try_from(filter)?)
}

// ---- syscall groups -------------------------------------------------------
// Numbers via libc::SYS_* keep these arch-correct and reviewable.

/// Loader + clean teardown baseline. Required to reach `main` and exit.
const BASELINE: &[i64] = &[
    libc::SYS_read,
    libc::SYS_write,
    libc::SYS_close,
    libc::SYS_mmap,
    libc::SYS_munmap,
    libc::SYS_mprotect,
    libc::SYS_brk,
    libc::SYS_rt_sigaction,
    libc::SYS_rt_sigprocmask,
    libc::SYS_rt_sigreturn,
    libc::SYS_exit,
    libc::SYS_exit_group,
    libc::SYS_arch_prctl,
    libc::SYS_set_tid_address,
    libc::SYS_set_robust_list,
    libc::SYS_rseq,
    libc::SYS_prlimit64,
    libc::SYS_getrandom,
    libc::SYS_sigaltstack,
    libc::SYS_gettid,
    libc::SYS_getpid,
];

/// Process/thread basics: spawn helpers, signals, scheduling.
const PROCESS_BASIC: &[i64] = &[
    libc::SYS_clone,
    libc::SYS_clone3,
    libc::SYS_execve,
    libc::SYS_execveat,
    libc::SYS_wait4,
    libc::SYS_kill,
    libc::SYS_tgkill,
    libc::SYS_getppid,
    libc::SYS_getuid,
    libc::SYS_geteuid,
    libc::SYS_getgid,
    libc::SYS_getegid,
    libc::SYS_setpgid,
    libc::SYS_getpgrp,
    libc::SYS_sched_getaffinity,
    libc::SYS_sched_yield,
    libc::SYS_prctl,
    libc::SYS_pipe2,
    libc::SYS_dup,
    libc::SYS_dup3,
    libc::SYS_fcntl,
    libc::SYS_poll,
    libc::SYS_ppoll,
    libc::SYS_epoll_create1,
    libc::SYS_epoll_ctl,
    libc::SYS_epoll_pwait,
    libc::SYS_eventfd2,
    libc::SYS_uname,
    libc::SYS_sysinfo,
    libc::SYS_membarrier,
];

/// Filesystem read/write within the workspace (Landlock confines the paths).
const FILE_RW: &[i64] = &[
    libc::SYS_openat,
    libc::SYS_openat2,
    libc::SYS_pread64,
    libc::SYS_pwrite64,
    libc::SYS_readv,
    libc::SYS_writev,
    libc::SYS_lseek,
    libc::SYS_newfstatat,
    libc::SYS_statx,
    libc::SYS_fstat,
    libc::SYS_getdents64,
    libc::SYS_getcwd,
    libc::SYS_chdir,
    libc::SYS_fchdir,
    libc::SYS_mkdirat,
    libc::SYS_unlinkat,
    libc::SYS_renameat,
    libc::SYS_renameat2,
    libc::SYS_linkat,
    libc::SYS_symlinkat,
    libc::SYS_readlinkat,
    libc::SYS_fchmod,
    libc::SYS_fchmodat,
    libc::SYS_fchown,
    libc::SYS_fchownat,
    libc::SYS_ftruncate,
    libc::SYS_fsync,
    libc::SYS_fdatasync,
    libc::SYS_fadvise64,
    libc::SYS_faccessat,
    libc::SYS_faccessat2,
    libc::SYS_flock,
    libc::SYS_utimensat,
    libc::SYS_umask,
    libc::SYS_ioctl,
    libc::SYS_copy_file_range,
    libc::SYS_sendfile,
];

/// Futex + thread parking.
const FUTEX_GROUP: &[i64] = &[libc::SYS_futex, libc::SYS_futex_waitv];

/// Legacy / arch-specific syscalls present on x86_64 (and most CISC arches) but
/// NOT on newer arch ABIs (aarch64/riscv64 dropped the bare `open`, `stat`,
/// `mkdir`, etc. in favor of the `*at` forms). Empty on arches that lack them so
/// the crate still compiles everywhere.
#[cfg(target_arch = "x86_64")]
const LEGACY_ARCH: &[i64] = &[
    libc::SYS_access,
    libc::SYS_open,
    libc::SYS_stat,
    libc::SYS_lstat,
    libc::SYS_fstat,
    libc::SYS_mkdir,
    libc::SYS_rmdir,
    libc::SYS_rename,
    libc::SYS_unlink,
    libc::SYS_link,
    libc::SYS_symlink,
    libc::SYS_readlink,
    libc::SYS_chmod,
    libc::SYS_chown,
    libc::SYS_dup2,
    libc::SYS_pipe,
    libc::SYS_poll,
    libc::SYS_epoll_wait,
    libc::SYS_epoll_create,
    libc::SYS_vfork,
    libc::SYS_getdents,
    libc::SYS_creat,
    libc::SYS_select,
    libc::SYS_mknod,
];

/// No legacy bare syscalls on non-x86_64 arches; everything is the `*at` form.
#[cfg(not(target_arch = "x86_64"))]
const LEGACY_ARCH: &[i64] = &[];

/// Clocks/timers.
const TIME_GROUP: &[i64] = &[
    libc::SYS_clock_gettime,
    libc::SYS_clock_getres,
    libc::SYS_clock_nanosleep,
    libc::SYS_nanosleep,
    libc::SYS_gettimeofday,
    libc::SYS_times,
    libc::SYS_timerfd_create,
    libc::SYS_timerfd_settime,
];

/// Extra syscalls rustc/cargo touch: mmap tuning, jobserver pipes/sockets
/// (the `socket` itself is added as a domain-restricted conditional rule),
/// process accounting, and memory advice.
const RUST_BUILD: &[i64] = &[
    libc::SYS_madvise,
    libc::SYS_mremap,
    libc::SYS_socketpair,
    libc::SYS_connect,
    libc::SYS_bind,
    libc::SYS_getsockname,
    libc::SYS_getpeername,
    libc::SYS_sendmsg,
    libc::SYS_recvmsg,
    libc::SYS_sendto,
    libc::SYS_recvfrom,
    libc::SYS_setsockopt,
    libc::SYS_getsockopt,
    libc::SYS_shutdown,
    libc::SYS_accept4,
    libc::SYS_listen,
    libc::SYS_pidfd_open,
    libc::SYS_pidfd_send_signal,
    libc::SYS_waitid,
    libc::SYS_rt_sigtimedwait,
    libc::SYS_restart_syscall,
    libc::SYS_sched_getparam,
    libc::SYS_sched_getscheduler,
    libc::SYS_get_mempolicy,
    libc::SYS_statfs,
    libc::SYS_fstatfs,
    libc::SYS_memfd_create,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn arch() -> TargetArch {
        match std::env::consts::ARCH {
            "aarch64" => TargetArch::aarch64,
            "riscv64" => TargetArch::riscv64,
            _ => TargetArch::x86_64,
        }
    }

    #[test]
    fn baseline_always_present() {
        let rules = build_rules(&[], arch()).unwrap_or_else(|e| panic!("{e}"));
        assert!(rules.contains_key(&libc::SYS_read));
        assert!(rules.contains_key(&libc::SYS_exit_group));
        // socket must NOT be unconditionally allowed without rust-build-tooling.
        assert!(!rules.contains_key(&libc::SYS_socket));
    }

    #[test]
    fn rust_build_group_adds_local_socket_rule() {
        let groups = vec!["rust-build-tooling".to_string()];
        let rules = build_rules(&groups, arch()).unwrap_or_else(|e| panic!("{e}"));
        let socket_rules = rules
            .get(&libc::SYS_socket)
            .unwrap_or_else(|| panic!("expected conditional socket rule"));
        // Conditional, not unconditional: AF_INET must fall through to default.
        assert!(
            !socket_rules.is_empty(),
            "socket must be conditional (AF_UNIX/AF_NETLINK only), never blanket-allowed"
        );
    }

    #[test]
    fn full_native_profile_compiles_to_bpf() {
        let groups = vec![
            "process-basic".to_string(),
            "file-readwrite-workspace".to_string(),
            "futex".to_string(),
            "time".to_string(),
            "rust-build-tooling".to_string(),
        ];
        let prog = compile(&groups, arch()).unwrap_or_else(|e| panic!("{e}"));
        assert!(!prog.is_empty(), "compiled BPF program must be non-empty");
    }

    #[test]
    fn unknown_group_is_ignored() {
        let groups = vec!["totally-made-up-group".to_string()];
        let rules = build_rules(&groups, arch()).unwrap_or_else(|e| panic!("{e}"));
        // Still has baseline, no panic.
        assert!(rules.contains_key(&libc::SYS_read));
    }
}

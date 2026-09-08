//! Security regression for the `pty` seccomp group: a jailed agent driven
//! through a controlling terminal can do benign tty ioctls but CANNOT
//! `ioctl(TIOCSTI)` — terminal stdin injection, the classic controlling-tty
//! escape (forge bytes into the tty's input queue to drive a parent shell).
//!
//! Each check runs in a forked child that applies the real seccomp filter and
//! attempts the syscall. The harness maps the child's exit code to a verdict
//! (`Blocked` = exit 0, `Escaped` = exit 1, `Skipped` = exit 2); here exit 0 /
//! `Blocked` is used to mean "the asserted security property held".

use jeryu_sandbox_linux::capability::SandboxCapabilities;
use jeryu_sandbox_linux::launch::open_pty;
use jeryu_sandbox_linux::seccomp_rules;
use jeryu_sandbox_linux::{EscapeVerdict, run_in_forked_child};
use seccompiler::TargetArch;
use std::os::fd::AsRawFd;

fn arch() -> TargetArch {
    match std::env::consts::ARCH {
        "aarch64" => TargetArch::aarch64,
        "riscv64" => TargetArch::riscv64,
        _ => TargetArch::x86_64,
    }
}

/// A representative agent PTY profile (the same groups the in-cell driver uses).
fn pty_groups() -> Vec<String> {
    [
        "process-basic",
        "file-readwrite-workspace",
        "futex",
        "time",
        "pty",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[test]
fn tiocsti_injection_is_denied_by_seccomp() {
    let caps = SandboxCapabilities::probe();
    if !caps.seccomp_bpf {
        eprintln!("SKIP tiocsti: seccomp-bpf unavailable on this host");
        return;
    }
    let bpf = seccomp_rules::compile(&pty_groups(), arch()).expect("compile pty profile");

    let verdict = run_in_forked_child(move || {
        // Allocate the PTY BEFORE the filter (openpty uses gated syscalls).
        let Ok((master, slave)) = open_pty() else {
            return 2; // skipped: could not allocate a pty
        };
        let slave_fd = slave.as_raw_fd();
        let _keep_open = (master, slave); // both ends stay open for the ioctl

        // seccomp requires no_new_privs (or CAP_SYS_ADMIN) before apply.
        if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
            return 2;
        }
        if seccompiler::apply_filter(&bpf).is_err() {
            return 2;
        }

        // The filter intercepts the syscall before the kernel tty handler, so a
        // -1/EPERM return proves SECCOMP denied it (not the kernel's own TIOCSTI
        // policy, which would surface a different errno).
        let ch: libc::c_char = b'x' as libc::c_char;
        let rc = unsafe { libc::ioctl(slave_fd, libc::TIOCSTI, &ch) };
        let denied_by_seccomp =
            rc == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
        u8::from(!denied_by_seccomp) // 0 == blocked (property held), 1 == injected
    });

    assert_eq!(
        verdict,
        EscapeVerdict::Blocked,
        "ioctl(TIOCSTI) must be denied (EPERM) by the pty seccomp profile"
    );
}

#[test]
fn benign_tty_ioctl_is_still_allowed() {
    let caps = SandboxCapabilities::probe();
    if !caps.seccomp_bpf {
        eprintln!("SKIP benign-ioctl: seccomp-bpf unavailable on this host");
        return;
    }
    let bpf = seccomp_rules::compile(&pty_groups(), arch()).expect("compile pty profile");

    let verdict = run_in_forked_child(move || {
        let Ok((master, slave)) = open_pty() else {
            return 2;
        };
        let slave_fd = slave.as_raw_fd();
        let _keep_open = (master, slave);

        if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
            return 2;
        }
        if seccompiler::apply_filter(&bpf).is_err() {
            return 2;
        }

        // TIOCGWINSZ (query window size) is a benign tty ioctl the agent's CLI
        // legitimately uses; the `arg1 != TIOCSTI` condition must still let it
        // through. exit 0 == it succeeded (property held).
        let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::ioctl(slave_fd, libc::TIOCGWINSZ, &mut ws) };
        u8::from(rc != 0) // 0 == allowed (property held), 1 == wrongly blocked
    });

    assert_eq!(
        verdict,
        EscapeVerdict::Blocked,
        "benign tty ioctl (TIOCGWINSZ) must still be allowed under the pty profile"
    );
}

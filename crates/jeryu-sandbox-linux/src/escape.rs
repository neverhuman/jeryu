//! Forked-child runner for the dockerless escape suite.
//!
//! Each escape attempt runs in an isolated forked child so a real sandbox
//! breach — or a kill by seccomp/cgroup — cannot corrupt the test process. The
//! child returns a status byte (0 => blocked, 1 => escaped, 2 => skipped) and
//! `_exit`s; the parent `waitpid`s and maps the result to a [`EscapeVerdict`].
//! This lives in the crate (not the test) because forking + isolating a child
//! is sandbox infrastructure, not a test artifact.

use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, fork};

/// Verdict for one escape attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeVerdict {
    /// The escape was blocked by the kernel primitive (the desired outcome).
    Blocked,
    /// The escape SUCCEEDED — a true sandbox breach. Always a failure.
    Escaped,
    /// The primitive is genuinely unavailable on this host; skipped honestly.
    Skipped,
}

impl EscapeVerdict {
    /// Stable lowercase label for receipts.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Escaped => "escaped",
            Self::Skipped => "skipped",
        }
    }
}

/// Body for a fork-bomb grandchild: sleep briefly (so it stays alive long
/// enough to count against the cgroup `pids.max`), then exit without running
/// any parent teardown. Used by the fork-bomb escape attempt.
pub fn run_busy_grandchild() -> ! {
    // SAFETY: `usleep` and `_exit` are async-signal-safe; the grandchild touches
    // no shared parent state before exiting.
    unsafe {
        libc::usleep(50_000);
        libc::_exit(0);
    }
}

/// Fork, run `body` in the child, and classify the child's exit in the parent.
///
/// `body` returns a status byte: `0` => the escape was blocked, `1` => it
/// succeeded (breach), `2` => the child chose to skip (primitive unavailable).
/// A child killed by a signal (seccomp `KillProcess`, cgroup OOM) counts as
/// blocked — the escape did not complete.
pub fn run_in_forked_child<F: FnOnce() -> u8>(body: F) -> EscapeVerdict {
    // SAFETY: `fork()` is called from a single-threaded test/CLI context; the
    // child branch below performs only the escape attempt and an
    // async-signal-safe `_exit`, never running parent destructors.
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            let code = body();
            // SAFETY: `_exit` is async-signal-safe and ends the child at once.
            unsafe { libc::_exit(code as i32) };
        }
        Ok(ForkResult::Parent { child }) => match waitpid(child, None) {
            Ok(WaitStatus::Exited(_, 0)) => EscapeVerdict::Blocked,
            Ok(WaitStatus::Exited(_, 2)) => EscapeVerdict::Skipped,
            Ok(WaitStatus::Signaled(_, _, _)) => EscapeVerdict::Blocked,
            _ => EscapeVerdict::Escaped,
        },
        Err(_) => EscapeVerdict::Skipped,
    }
}

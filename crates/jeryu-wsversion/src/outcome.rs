//! Shared gate-outcome type, mirroring `jeryu_repogate::GateOutcome`.
//!
//! The CLI converts a [`GateOutcome`] into printed stdout lines plus a process
//! exit code, so the lib stays pure and side-effect free while the binary owns
//! all I/O. This is the same split `jeryu-repogate` uses for its repo gates.

/// Outcome of a gate: the lines to print on stdout and the process exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateOutcome {
    /// Lines to emit on stdout, in order.
    pub stdout: Vec<String>,
    /// Process exit code (0 = pass).
    pub exit_code: i32,
}

impl GateOutcome {
    /// A passing outcome (exit code 0) carrying the given stdout lines.
    #[must_use]
    pub fn ok(stdout: Vec<String>) -> Self {
        Self {
            stdout,
            exit_code: 0,
        }
    }

    /// A failing outcome (exit code 1) carrying the given stdout lines.
    #[must_use]
    pub fn fail(stdout: Vec<String>) -> Self {
        Self {
            stdout,
            exit_code: 1,
        }
    }
}

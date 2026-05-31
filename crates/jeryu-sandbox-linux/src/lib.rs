#![doc = "The real no-Docker syscall sandbox for Jeryu native runners (Phase A1)."]
//!
//! This crate is the single unsafe island in the runner fabric. It applies the
//! kernel-level sandbox primitives that the [`jeryu_runner_core`] policy model
//! describes:
//!
//! * `PR_SET_NO_NEW_PRIVS` (always, fail-closed)
//! * cgroups-v2 memory/pids/cpu limits in a delegated subtree (when available)
//! * Landlock workspace-only-writable ruleset (ABI-probed)
//! * seccomp default-deny-with-allowlist (toolchain-survival tested)
//! * user/mount/pid namespace unshare (only where the kernel permits it)
//!
//! Enforcement is treated as a first-class, capability-PROBED, receipt-recorded
//! state. The host this was developed on BLOCKS unprivileged user namespaces but
//! ACTIVELY supports Landlock ABI 4, seccomp-bpf, `no_new_privs`, and delegated
//! cgroups-v2 — so [`SandboxCapabilities::probe`] performs real throwaway-child
//! syscalls to detect what is actually enforceable rather than trusting sysctls.
//!
//! The crate NEVER fakes a PASS: when a required primitive is unavailable the
//! enforcement level degrades honestly and the escape suite emits skip-with-receipt.

pub mod capability;
pub mod launch;
pub mod seccomp_rules;
pub mod watchdog;

pub use capability::{EnforcementLevel, SandboxCapabilities};
pub use launch::{
    EnforcementReport, SandboxError, SandboxResult, spawn_sandboxed, verify_enforcement,
};
pub use watchdog::{WatchdogOutcome, run_with_watchdog};

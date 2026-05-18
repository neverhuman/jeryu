//! Owner: jeryu-gcd (always-on disk daemon)
//! Proof: `cargo nextest run -p jeryu-gcd --lib`
//! Invariants: tick_action is pure; preserves active leases below Emergency.
//!
//! The library half of `jeryu-gcd`. The daemon's loop logic lives here so it
//! is unit-testable without spawning the binary or talking to systemd.

use std::time::Duration;

pub use jeryu::cache::{
    DiskPressureLevel, ROOT_DISK_CRITICAL_MIN_FREE_BYTES, ROOT_DISK_EMERGENCY_MIN_FREE_BYTES,
    ROOT_DISK_HEADROOM_MIN_FREE_BYTES, ROOT_DISK_WARNING_MIN_FREE_BYTES, root_disk_pressure_level,
};

/// Default daemon tick. Configurable via `--interval-secs` / `JERYU_GCD_INTERVAL_SECS`.
pub const DEFAULT_INTERVAL_SECS: u64 = 60;

/// Default target free bytes. Slightly above the 80 GiB headroom floor so GC
/// doesn't oscillate at the boundary. Configurable via
/// `--target-free-bytes` / `JERYU_GCD_TARGET_FREE_BYTES`.
pub const DEFAULT_TARGET_FREE_BYTES: u64 = 85 * 1024 * 1024 * 1024;

/// Action a single tick chose, given the current disk-free reading. Pure
/// function of free bytes + (a coarse clock for the Nominal slow-tick path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickAction {
    /// Nominal pressure, no action this tick.
    Idle,
    /// Nominal pressure, time for the scheduled deep sweep (every ~6 h).
    NominalSweep,
    /// Warning tier: 4 h-age GC with 120 GiB budget.
    WarningGc,
    /// Critical tier: 2 h-age GC + orphan worker cleanup.
    CriticalGc,
    /// Emergency tier: 15 m-age GC, reclaim-aggressive sweep, incremental sweep.
    EmergencyGc,
}

impl TickAction {
    pub fn is_blocking(&self) -> bool {
        !matches!(self, Self::Idle)
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::NominalSweep => "nominal-sweep",
            Self::WarningGc => "warning-gc",
            Self::CriticalGc => "critical-gc",
            Self::EmergencyGc => "emergency-gc",
        }
    }
}

/// Decide what this tick should do. Pure function — the daemon's only test
/// dependency. `nominal_sweep_due` is the caller's responsibility (typically
/// "last nominal sweep was >= 6 h ago").
pub fn tick_action(available_bytes: u64, nominal_sweep_due: bool) -> TickAction {
    match root_disk_pressure_level(available_bytes) {
        DiskPressureLevel::Emergency => TickAction::EmergencyGc,
        DiskPressureLevel::Critical => TickAction::CriticalGc,
        DiskPressureLevel::Warning => TickAction::WarningGc,
        DiskPressureLevel::Nominal => {
            if nominal_sweep_due {
                TickAction::NominalSweep
            } else {
                TickAction::Idle
            }
        }
    }
}

/// Nominal sweep cadence (deep sweep when disk is healthy). Default 6 h.
pub const DEFAULT_NOMINAL_SWEEP_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn nominal_idle_when_not_due() {
        // 100 GiB free, far above 80 GiB warning threshold.
        assert_eq!(tick_action(100 * GIB, false), TickAction::Idle);
    }

    #[test]
    fn nominal_sweep_when_due() {
        assert_eq!(tick_action(100 * GIB, true), TickAction::NominalSweep);
    }

    #[test]
    fn warning_triggers_at_70_gib() {
        // 70 GiB free is between Warning (80) and Critical (60).
        assert_eq!(tick_action(70 * GIB, false), TickAction::WarningGc);
    }

    #[test]
    fn critical_triggers_at_50_gib() {
        // 50 GiB free is between Critical (60) and Emergency (40).
        assert_eq!(tick_action(50 * GIB, false), TickAction::CriticalGc);
    }

    #[test]
    fn emergency_triggers_at_30_gib() {
        assert_eq!(tick_action(30 * GIB, false), TickAction::EmergencyGc);
    }

    #[test]
    fn pressure_overrides_due_flag() {
        // Even with nominal_sweep_due=true, real pressure wins.
        assert_eq!(tick_action(30 * GIB, true), TickAction::EmergencyGc);
        assert_eq!(tick_action(50 * GIB, true), TickAction::CriticalGc);
    }

    #[test]
    fn labels_match_actions() {
        assert_eq!(TickAction::Idle.label(), "idle");
        assert_eq!(TickAction::NominalSweep.label(), "nominal-sweep");
        assert_eq!(TickAction::WarningGc.label(), "warning-gc");
        assert_eq!(TickAction::CriticalGc.label(), "critical-gc");
        assert_eq!(TickAction::EmergencyGc.label(), "emergency-gc");
    }

    #[test]
    fn is_blocking_only_for_real_work() {
        assert!(!TickAction::Idle.is_blocking());
        assert!(TickAction::NominalSweep.is_blocking());
        assert!(TickAction::EmergencyGc.is_blocking());
    }
}

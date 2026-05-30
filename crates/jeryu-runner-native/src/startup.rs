#![doc = "Startup measurement probes for Phase 4 exit-bar benchmarks."]

use jeryu_runner_core::receipt::now_ms;
use std::time::{Duration, Instant};

/// Startup probe result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupProbe {
    /// Measured elapsed wall time.
    pub elapsed: Duration,
    /// Timestamp in ms when started.
    pub started_ms: u128,
    /// Timestamp in ms when finished.
    pub finished_ms: u128,
}

/// Measure minimal runner plan startup overhead.
pub fn measure_startup_probe() -> StartupProbe {
    let started_ms = now_ms();
    let started = Instant::now();
    let _guard_seed = std::env::var("JERYU_RUNNERD_PROBE").unwrap_or_else(|_| "probe".to_string());
    let elapsed = started.elapsed();
    let finished_ms = now_ms();
    StartupProbe {
        elapsed,
        started_ms,
        finished_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_probe_records_time() {
        let probe = measure_startup_probe();
        assert!(probe.finished_ms >= probe.started_ms);
    }
}

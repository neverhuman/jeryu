#![doc = "runnerd startup probes used by Phase 4 benchmarking."]

use runner_native::{measure_startup_probe, StartupProbe};

/// Run a native startup probe.
pub fn native_startup_probe() -> StartupProbe {
    measure_startup_probe()
}

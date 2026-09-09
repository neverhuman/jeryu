#![doc = "jeryu_runnerd startup probes used by Phase 4 benchmarking."]

use jeryu_runner_native::{StartupProbe, measure_startup_probe};

/// Run a native startup probe.
pub fn native_startup_probe() -> StartupProbe {
    measure_startup_probe()
}

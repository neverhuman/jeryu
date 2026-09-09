//! Chaos drill model.

/// Supported Phase 10 chaos drill types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DrillKind {
    DatabaseFailover,
    ObjectStoreLatency,
    ControlPlaneRestart,
}

/// One chaos drill plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChaosDrill {
    pub kind: DrillKind,
    pub name: String,
    pub blast_radius: String,
    pub expected_recovery_ms: u64,
}

/// Result of a chaos drill.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChaosResult {
    pub drill: ChaosDrill,
    pub observed_recovery_ms: u64,
    pub data_loss: bool,
    pub receipt: String,
}

impl ChaosDrill {
    /// Standard database failover drill.
    pub fn db_failover() -> Self {
        Self {
            kind: DrillKind::DatabaseFailover,
            name: "db-failover".to_owned(),
            blast_radius: "primary-postgres".to_owned(),
            expected_recovery_ms: 30_000,
        }
    }

    /// Standard object-store latency drill.
    pub fn object_store_latency() -> Self {
        Self {
            kind: DrillKind::ObjectStoreLatency,
            name: "object-store-latency".to_owned(),
            blast_radius: "artifact-cas".to_owned(),
            expected_recovery_ms: 10_000,
        }
    }

    /// Execute deterministic simulation.
    pub fn simulate(&self, observed_recovery_ms: u64, data_loss: bool) -> ChaosResult {
        let receipt = format!(
            "chaos:{}:{}:{}",
            self.name,
            observed_recovery_ms,
            if data_loss { "loss" } else { "no-loss" }
        );
        ChaosResult {
            drill: self.clone(),
            observed_recovery_ms,
            data_loss,
            receipt,
        }
    }
}

impl ChaosResult {
    /// True when the drill meets the recovery and safety gates.
    pub fn passed(&self) -> bool {
        !self.data_loss && self.observed_recovery_ms <= self.drill.expected_recovery_ms
    }
}

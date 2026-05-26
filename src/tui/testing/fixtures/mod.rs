//! Owner: Interactive TUI subsystem — fixture scenarios (U15)
//! Proof: `cargo nextest run -p jeryu --lib tui::testing::fixtures::`
//! Invariants: each scenario is a pure fn `-> TuiReadModel`; calling twice
//! returns bytewise-identical JSON. Shared helpers below; no mutable state.

use chrono::{DateTime, TimeZone, Utc};

use crate::api::entity::{ActionRef, DataFreshness, HealthLevel};
use crate::api::read_model::{ComponentHealth, RunnerHealth, SystemHealth};
use crate::tui::action_registry::RiskTier;

pub mod agents;
pub mod bugs;
pub mod cache;
pub mod incident;
pub mod jankurai;
pub mod mission;
pub mod queue;
pub mod release;
pub mod security;
pub mod vti;
pub mod workflow;

/// Deterministic UTC timestamp for fixture-day (2026-05-26).
pub(crate) fn ts(h: u32, m: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 26, h, m, s).unwrap()
}

/// `DataFreshness` with each source's age in ms.
pub(crate) fn fresh(g: u64, d: u64, dk: u64, c: u64, v: u64, stale: bool) -> DataFreshness {
    DataFreshness {
        gitlab_ms: Some(g),
        db_ms: Some(d),
        docker_ms: Some(dk),
        cache_ms: Some(c),
        vault_ms: Some(v),
        overall_stale: stale,
    }
}

/// Healthy `SystemHealth` with 12 runners online; scenarios patch fields.
pub(crate) fn healthy_system(busy: u32, idle: u32) -> SystemHealth {
    SystemHealth {
        gitlab: ComponentHealth::ok("gitlab", 14),
        database: ComponentHealth::ok("database", 3),
        docker: ComponentHealth::ok("docker", 7),
        cache: ComponentHealth::ok("cache", 5),
        vault: ComponentHealth::ok("vault", 11),
        runners: RunnerHealth {
            online: 12,
            busy,
            idle,
            degraded: 0,
        },
    }
}

/// Marks a component degraded with a status detail.
pub(crate) fn degraded_component(name: &str, detail: &str, latency_ms: u64) -> ComponentHealth {
    ComponentHealth {
        name: name.into(),
        status: HealthLevel::Degraded,
        latency_ms: Some(latency_ms),
        detail: Some(detail.into()),
    }
}

/// Deterministic `ActionRef` literal.
pub(crate) fn action(id: &str, label: &str, risk: RiskTier) -> ActionRef {
    ActionRef {
        action_id: id.into(),
        label: label.into(),
        risk: Some(risk),
    }
}

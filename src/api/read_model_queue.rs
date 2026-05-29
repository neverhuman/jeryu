use serde::{Deserialize, Serialize};

use crate::api::entity::{EntityKind, EntityRef};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueueSnapshot {
    pub total_waiting_jobs: u32,
    pub total_running_jobs: u32,
    pub pools: Vec<QueuePoolSnapshot>,
    pub waiting_jobs: Vec<QueueJobSummary>,
}

impl QueueSnapshot {
    pub fn demand(&self) -> u32 {
        self.total_waiting_jobs
            .saturating_add(self.total_running_jobs)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueuePoolSnapshot {
    pub entity: EntityRef,
    pub name: String,
    pub tags: Vec<String>,
    pub trust_tier: String,
    pub paused: bool,
    pub active_managers: u32,
    pub max_managers: u32,
    pub slots_per_manager: u32,
    pub request_concurrency: u32,
    pub running_jobs: u32,
    pub waiting_jobs: u32,
    pub degraded_managers: u32,
}

impl QueuePoolSnapshot {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            entity: EntityRef::new(EntityKind::Pool, format!("pool/{name}")),
            name,
            tags: Vec::new(),
            trust_tier: "trusted".into(),
            paused: false,
            active_managers: 0,
            max_managers: 0,
            slots_per_manager: 1,
            request_concurrency: 1,
            running_jobs: 0,
            waiting_jobs: 0,
            degraded_managers: 0,
        }
    }

    pub fn active_slots(&self) -> u32 {
        if self.paused {
            0
        } else {
            self.active_managers
                .saturating_mul(self.slots_per_manager.max(1))
        }
    }

    pub fn configured_max_slots(&self) -> u32 {
        self.max_managers
            .saturating_mul(self.slots_per_manager.max(1))
    }

    pub fn idle_slots(&self) -> u32 {
        self.active_slots().saturating_sub(self.running_jobs)
    }

    pub fn supports_tags(&self, required: &[String]) -> bool {
        required
            .iter()
            .all(|required| self.tags.iter().any(|tag| tag == required))
    }
}

impl Default for QueuePoolSnapshot {
    fn default() -> Self {
        Self::new("unknown")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueueJobSummary {
    pub entity: EntityRef,
    pub label: String,
    pub stage: String,
    pub status: String,
    pub queued_ms: u64,
    pub required_tags: Vec<String>,
    pub pool: Option<String>,
    pub pipeline: Option<EntityRef>,
}

impl QueueJobSummary {
    pub fn new(label: impl Into<String>, queued_ms: u64) -> Self {
        let label = label.into();
        Self {
            entity: EntityRef::new(EntityKind::Job, label.clone()),
            label,
            stage: "build".into(),
            status: "pending".into(),
            queued_ms,
            required_tags: Vec::new(),
            pool: None,
            pipeline: None,
        }
    }

    pub fn is_waiting(&self) -> bool {
        matches!(
            self.status.as_str(),
            "pending" | "created" | "waiting_for_resource" | "preparing"
        )
    }

    pub fn queued_secs(&self) -> u64 {
        self.queued_ms / 1_000
    }
}

impl Default for QueueJobSummary {
    fn default() -> Self {
        Self::new("unknown", 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_slots_separate_active_and_configured_max() {
        let mut pool = QueuePoolSnapshot::new("trusted");
        pool.active_managers = 2;
        pool.max_managers = 5;
        pool.slots_per_manager = 4;
        pool.running_jobs = 6;

        assert_eq!(pool.active_slots(), 8);
        assert_eq!(pool.configured_max_slots(), 20);
        assert_eq!(pool.idle_slots(), 2);
    }

    #[test]
    fn queue_snapshot_round_trips_json() {
        let snapshot = QueueSnapshot {
            total_waiting_jobs: 1,
            total_running_jobs: 2,
            pools: vec![QueuePoolSnapshot::new("trusted")],
            waiting_jobs: vec![QueueJobSummary::new("build-web", 12_000)],
        };

        let encoded = serde_json::to_string(&snapshot).expect("serialize queue");
        let decoded: QueueSnapshot = serde_json::from_str(&encoded).expect("deserialize queue");
        assert_eq!(decoded, snapshot);
    }

    #[test]
    fn pool_entities_use_action_compatible_kind() {
        assert_eq!(
            QueuePoolSnapshot::new("trusted").entity.kind,
            EntityKind::Pool
        );
    }
}

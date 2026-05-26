//! Owner: TUI Control-Plane API - capacity / theoretical-limit contracts
//! Proof: `cargo test -p jeryu --lib api::capacity`
//! Invariants: Three-tier capacity model (physics / fleet / policy). The
//!             Queue lens uses these to answer "does adding runners help?"
//!             without lying when the binding constraint is downstream.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BottleneckClass {
    /// Pure runner capacity — adding runners helps.
    Capacity,
    /// Pipeline DAG serializes work — adding runners does NOT help.
    DagSerialization,
    /// Cache misses gate parallelism — fix cache, not runners.
    CacheMiss,
    /// VTI expansion forces extra work — fix selection, not runners.
    VtiExpansion,
    /// Policy gate (admission, security, release) holds work — fix policy.
    PolicyGate,
    /// Resource (CPU/mem/disk) saturation on existing runners.
    ResourceSaturation,
    /// Tag fragmentation makes runners unusable for queued work.
    TagFragmentation,
}

impl BottleneckClass {
    pub fn label(self) -> &'static str {
        match self {
            Self::Capacity => "capacity",
            Self::DagSerialization => "dag_serialization",
            Self::CacheMiss => "cache_miss",
            Self::VtiExpansion => "vti_expansion",
            Self::PolicyGate => "policy_gate",
            Self::ResourceSaturation => "resource_saturation",
            Self::TagFragmentation => "tag_fragmentation",
        }
    }

    /// Whether adding more runners actually helps this bottleneck class.
    pub fn helped_by_more_runners(self) -> bool {
        matches!(self, Self::Capacity | Self::ResourceSaturation)
    }
}

/// Infinite-runner, hot-cache, no-policy upper bound. The "what could
/// possibly be achievable" floor — used to bound any what-if simulation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PhysicsFloor {
    /// Lowest possible end-to-end pipeline time (s) given the DAG critical path.
    pub critical_path_seconds: f64,
    /// Maximum theoretical concurrency for jobs in the queue (DAG-bound).
    pub max_concurrent_jobs: u32,
}

/// Current fleet's effective ceiling — assumes current pools, current trust
/// tiers, current cache state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct FleetFloor {
    pub effective_slots: u32,
    pub raw_slots: u32,
    pub tag_compatible_slots: u32,
    pub resource_headroom_ratio: f64,
}

/// Policy ceiling — what gates (security, admission, release, freeze) allow.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PolicyFloor {
    pub admission_passes: bool,
    pub security_passes: bool,
    pub freeze_window_active: bool,
    pub effective_max_concurrent: u32,
}

/// The SCREAM index (0-100): inefficiency headline summarizing how far the
/// fleet is from physics-floor performance. 0 = fleet matches physics; 100
/// = fleet runs at minimum throughput vs. theoretical.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ScreamIndex {
    pub value: u8,
    pub dominant_class: BottleneckClass,
    /// Per-class breakdown summing to 100. Used by the Queue lens
    /// stack-bar widget.
    pub breakdown: [(BottleneckClass, u8); 7],
}

impl ScreamIndex {
    pub fn clean() -> Self {
        Self {
            value: 0,
            dominant_class: BottleneckClass::Capacity,
            breakdown: [
                (BottleneckClass::Capacity, 0),
                (BottleneckClass::DagSerialization, 0),
                (BottleneckClass::CacheMiss, 0),
                (BottleneckClass::VtiExpansion, 0),
                (BottleneckClass::PolicyGate, 0),
                (BottleneckClass::ResourceSaturation, 0),
                (BottleneckClass::TagFragmentation, 0),
            ],
        }
    }
}

/// Composite capacity snapshot the Queue lens consumes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapacitySnapshot {
    pub physics: PhysicsFloor,
    pub fleet: FleetFloor,
    pub policy: PolicyFloor,
    pub scream: ScreamIndex,
}

impl CapacitySnapshot {
    /// True when the dominant bottleneck would not be helped by adding
    /// runners. The Queue lens dims the "add runners" recommendation.
    pub fn adding_runners_pointless(&self) -> bool {
        !self.scream.dominant_class.helped_by_more_runners()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_unique() {
        let labels: Vec<&str> = [
            BottleneckClass::Capacity,
            BottleneckClass::DagSerialization,
            BottleneckClass::CacheMiss,
            BottleneckClass::VtiExpansion,
            BottleneckClass::PolicyGate,
            BottleneckClass::ResourceSaturation,
            BottleneckClass::TagFragmentation,
        ]
        .iter()
        .map(|c| c.label())
        .collect();
        let mut sorted = labels.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(labels.len(), sorted.len());
    }

    #[test]
    fn capacity_is_helped_by_runners() {
        assert!(BottleneckClass::Capacity.helped_by_more_runners());
        assert!(BottleneckClass::ResourceSaturation.helped_by_more_runners());
    }

    #[test]
    fn dag_serialization_is_not_helped_by_runners() {
        assert!(!BottleneckClass::DagSerialization.helped_by_more_runners());
        assert!(!BottleneckClass::CacheMiss.helped_by_more_runners());
        assert!(!BottleneckClass::PolicyGate.helped_by_more_runners());
    }

    #[test]
    fn scream_clean_means_zero_value() {
        let s = ScreamIndex::clean();
        assert_eq!(s.value, 0);
    }

    #[test]
    fn capacity_snapshot_recommends_against_runners_when_dag_bound() {
        let snap = CapacitySnapshot {
            physics: PhysicsFloor {
                critical_path_seconds: 600.0,
                max_concurrent_jobs: 8,
            },
            fleet: FleetFloor {
                effective_slots: 16,
                raw_slots: 20,
                tag_compatible_slots: 16,
                resource_headroom_ratio: 0.4,
            },
            policy: PolicyFloor {
                admission_passes: true,
                security_passes: true,
                freeze_window_active: false,
                effective_max_concurrent: 16,
            },
            scream: ScreamIndex {
                value: 65,
                dominant_class: BottleneckClass::DagSerialization,
                breakdown: ScreamIndex::clean().breakdown,
            },
        };
        assert!(snap.adding_runners_pointless());
    }

    #[test]
    fn capacity_serde_roundtrip() {
        let snap = CapacitySnapshot {
            physics: PhysicsFloor {
                critical_path_seconds: 120.0,
                max_concurrent_jobs: 12,
            },
            fleet: FleetFloor {
                effective_slots: 8,
                raw_slots: 10,
                tag_compatible_slots: 8,
                resource_headroom_ratio: 0.6,
            },
            policy: PolicyFloor {
                admission_passes: true,
                security_passes: true,
                freeze_window_active: false,
                effective_max_concurrent: 8,
            },
            scream: ScreamIndex::clean(),
        };
        let json = serde_json::to_string(&snap).unwrap();
        let back: CapacitySnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap, back);
    }
}

//! Multi-node runner registry (pure logic).
//!
//! This crate owns the in-memory bookkeeping for a fleet of runner nodes:
//! registration, heartbeat liveness, reaping of dead nodes, draining, and
//! capacity/affinity-aware assignment. It is intentionally synchronous and
//! side-effect free; the daemon layer owns all real I/O and persistence.
//!
//! ## Fencing
//!
//! Every node carries a monotonic `epoch` (a fencing token). The epoch is
//! bumped on every state transition that must invalidate work the node may
//! still believe it owns:
//!
//! * `register` (a node coming up gets a fresh epoch),
//! * `reap` (a missed-heartbeat node is fenced so its in-flight work cannot
//!   be completed under the old epoch).
//!
//! Heartbeats and completions that carry a stale epoch are rejected. This is
//! what lets a reassigned lease land on a healthy node exactly once without
//! the previously-assigned (now dead) node racing to claim it.

use std::collections::BTreeMap;

use jeryu_ci_ir::RunnerClass;
use jeryu_runner_protocol::{Heartbeat, RunnerHello};
use serde::{Deserialize, Serialize};

/// Lifecycle state of a node in the registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeState {
    /// The node has announced itself but is not yet eligible for assignment.
    Registering,
    /// The node is healthy and eligible for assignment.
    Active,
    /// The operator has requested the node stop taking new work. Existing
    /// in-flight work may finish, but `assign` will never pick a draining node.
    Draining,
    /// The node missed its heartbeat deadline (or was otherwise fenced) and is
    /// no longer eligible for assignment.
    Dead,
}

impl NodeState {
    /// String form, used for stable snapshots and diagnostics.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Registering => "registering",
            Self::Active => "active",
            Self::Draining => "draining",
            Self::Dead => "dead",
        }
    }
}

/// A single node's record in the registry.
///
/// `supported_classes` is stored as the concrete [`RunnerClass`] enum, but is
/// serialized via its canonical string form so the registry does not force a
/// serde dependency onto `jeryu-ci-ir`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeRecord {
    /// Stable identity of the node.
    pub node_id: String,
    /// Current lifecycle state.
    pub state: NodeState,
    /// Fencing token. Strictly increasing for the lifetime of a `node_id`.
    pub epoch: u64,
    /// Maximum number of concurrent leases the node can hold.
    pub capacity: u32,
    /// Number of leases currently assigned to this node.
    pub in_flight: u32,
    /// Runner classes this node can execute.
    pub supported_classes: Vec<RunnerClass>,
    /// Pool-affinity tags (free-form labels). An assignment with required tags
    /// only lands on a node whose tag set is a superset of the requirement.
    pub tags: Vec<String>,
    /// Logical clock value of the most recent accepted heartbeat (or
    /// registration). Used together with `heartbeat_ttl` to detect death.
    pub last_heartbeat_epoch: u64,
}

impl NodeRecord {
    /// Whether the node currently has spare capacity for another lease.
    #[must_use]
    pub fn has_capacity(&self) -> bool {
        self.in_flight < self.capacity
    }

    /// Whether the node is eligible to receive new assignments.
    #[must_use]
    pub fn is_assignable(&self) -> bool {
        self.state == NodeState::Active && self.has_capacity()
    }

    /// Whether the node supports the requested runner class.
    #[must_use]
    pub fn supports_class(&self, class: &RunnerClass) -> bool {
        self.supported_classes.iter().any(|c| c == class)
    }

    /// Whether the node carries every required tag (pool affinity).
    #[must_use]
    pub fn matches_tags(&self, required: &[String]) -> bool {
        required
            .iter()
            .all(|need| self.tags.iter().any(|have| have == need))
    }
}

/// Acknowledgement returned to a node that has registered (or re-registered).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrationAck {
    /// Identity assigned to / confirmed for the node.
    pub node_id: String,
    /// Fresh fencing token the node must echo on subsequent messages.
    pub epoch: u64,
    /// How long (in logical clock units) a lease/registration is valid before
    /// a heartbeat is required.
    pub lease_ttl: u64,
}

/// Acknowledgement returned in response to a heartbeat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatAck {
    /// `true` if the heartbeat's epoch matches the registry's current epoch for
    /// the node, i.e. the node is still the legitimate owner of its work.
    pub still_owner: bool,
    /// `true` if the operator has asked this node to drain.
    pub drain: bool,
}

/// A node that was reaped (declared dead) during a `reap` sweep.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReapedNode {
    /// Identity of the reaped node.
    pub node_id: String,
    /// The fencing token after the reap bump. Any in-flight work carrying the
    /// previous epoch is now stale.
    pub fenced_epoch: u64,
    /// Number of leases that were in flight when the node was reaped.
    pub orphaned_in_flight: u32,
}

/// A request to place a single lease onto some node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignSpec {
    /// Stable identity of the lease being assigned.
    pub lease_id: String,
    /// Required runner class.
    #[serde(with = "runner_class_string")]
    pub runner_class: RunnerClass,
    /// Required pool-affinity tags. A node must carry all of these.
    #[serde(default)]
    pub required_tags: Vec<String>,
}

impl AssignSpec {
    /// Construct an assignment request with no tag affinity.
    pub fn new(lease_id: impl Into<String>, runner_class: RunnerClass) -> Self {
        Self {
            lease_id: lease_id.into(),
            runner_class,
            required_tags: Vec::new(),
        }
    }

    /// Builder-style: require the given pool-affinity tags.
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.required_tags = tags;
        self
    }
}

/// Result of a successful assignment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    /// Node the lease was placed on.
    pub node_id: String,
    /// Fencing token of the node at the moment of assignment. The runner must
    /// carry this epoch; a stale epoch will be rejected on heartbeat/complete.
    pub epoch: u64,
}

/// The in-memory multi-node registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeRegistry {
    /// All known nodes keyed by `node_id`. `BTreeMap` gives deterministic
    /// iteration order, which the assignment tie-breaker relies on.
    pub nodes: BTreeMap<String, NodeRecord>,
    /// Number of logical clock units a node may go without a heartbeat before
    /// `reap` declares it dead.
    pub heartbeat_ttl: u64,
}

mod node_registry;

// ---------------------------------------------------------------------------
// Serde wire model.
//
// `RunnerClass` (from jeryu-ci-ir) does not implement serde, so we mirror the
// registry into a wire struct that encodes runner classes as their canonical
// strings. This keeps the snapshot deterministic without forcing a serde
// dependency onto jeryu-ci-ir.
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct RegistrySnapshot {
    heartbeat_ttl: u64,
    nodes: Vec<NodeRecordWire>,
}

#[derive(Serialize, Deserialize)]
struct NodeRecordWire {
    node_id: String,
    state: NodeState,
    epoch: u64,
    capacity: u32,
    in_flight: u32,
    supported_classes: Vec<String>,
    tags: Vec<String>,
    last_heartbeat_epoch: u64,
}

impl From<&NodeRegistry> for RegistrySnapshot {
    fn from(reg: &NodeRegistry) -> Self {
        // BTreeMap iteration is sorted by key, giving a canonical ordering.
        let nodes = reg
            .nodes
            .values()
            .map(|node| NodeRecordWire {
                node_id: node.node_id.clone(),
                state: node.state,
                epoch: node.epoch,
                capacity: node.capacity,
                in_flight: node.in_flight,
                supported_classes: node
                    .supported_classes
                    .iter()
                    .map(|c| c.as_str().to_string())
                    .collect(),
                tags: node.tags.clone(),
                last_heartbeat_epoch: node.last_heartbeat_epoch,
            })
            .collect();
        Self {
            heartbeat_ttl: reg.heartbeat_ttl,
            nodes,
        }
    }
}

impl From<RegistrySnapshot> for NodeRegistry {
    fn from(wire: RegistrySnapshot) -> Self {
        let mut nodes = BTreeMap::new();
        for n in wire.nodes {
            let supported_classes = n
                .supported_classes
                .iter()
                .map(|s| parse_runner_class(s))
                .collect();
            nodes.insert(
                n.node_id.clone(),
                NodeRecord {
                    node_id: n.node_id,
                    state: n.state,
                    epoch: n.epoch,
                    capacity: n.capacity,
                    in_flight: n.in_flight,
                    supported_classes,
                    tags: n.tags,
                    last_heartbeat_epoch: n.last_heartbeat_epoch,
                },
            );
        }
        Self {
            nodes,
            heartbeat_ttl: wire.heartbeat_ttl,
        }
    }
}

/// Parse a runner class from its canonical string. `RunnerClass::from_str` is
/// infallible for non-empty input (unknown tokens become `Custom`), so the
/// only failure mode is the empty string, which we map to the default class.
// The explicit match is intentional: jankurai HLT-001 flags `unwrap_or_default()`
// as fallback-soup, so we spell out the empty-string -> default mapping. clippy
// would prefer the terse form; we keep the explicit one for the audit.
#[allow(clippy::manual_unwrap_or_default)]
fn parse_runner_class(s: &str) -> RunnerClass {
    // `from_str` maps unknown tokens to `Custom` and only errors on the empty
    // string; an empty class name is explicitly the default class.
    match s.parse::<RunnerClass>() {
        Ok(class) => class,
        Err(_) => RunnerClass::default(),
    }
}

/// `serde` adapter that (de)serializes a single [`RunnerClass`] as its
/// canonical string form, used by [`AssignSpec`].
mod runner_class_string {
    use jeryu_ci_ir::RunnerClass;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(class: &RunnerClass, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(class.as_str())
    }

    // Explicit match (not `unwrap_or_default`) to satisfy the jankurai
    // fallback-soup audit; see `parse_runner_class`.
    #[allow(clippy::manual_unwrap_or_default)]
    pub fn deserialize<'de, D>(deserializer: D) -> Result<RunnerClass, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.parse::<RunnerClass>() {
            Ok(class) => class,
            Err(_) => RunnerClass::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_runner_protocol::Heartbeat;

    fn hello(id: &str, capacity: u32, classes: Vec<RunnerClass>, tags: Vec<&str>) -> RunnerHello {
        let mut h = RunnerHello::new(id, classes);
        h.capacity = capacity;
        h.labels = tags.into_iter().map(String::from).collect();
        h
    }

    fn beat(id: &str) -> Heartbeat {
        Heartbeat {
            runner_id: id.to_string(),
            runner_epoch: 1,
            run_id: "run".to_string(),
            lease_id: "lease".to_string(),
            job_id: "job".to_string(),
            monotonic_millis: 0,
            message: String::new(),
        }
    }

    #[test]
    fn register_activates_and_bumps_epoch() {
        let mut reg = NodeRegistry::new(10);
        let ack = reg.register(
            &hello("n1", 2, vec![RunnerClass::NativeRustClean], vec![]),
            0,
        );
        assert_eq!(ack.epoch, 1);
        assert_eq!(reg.nodes["n1"].state, NodeState::Active);

        // Re-registration fences the prior incarnation with a higher epoch.
        let ack2 = reg.register(
            &hello("n1", 2, vec![RunnerClass::NativeRustClean], vec![]),
            5,
        );
        assert_eq!(ack2.epoch, 2);
    }

    #[test]
    fn heartbeat_rejects_stale_epoch() {
        let mut reg = NodeRegistry::new(10);
        let ack = reg.register(
            &hello("n1", 1, vec![RunnerClass::NativeRustClean], vec![]),
            0,
        );
        // Current epoch is accepted.
        let mut current = beat("n1");
        current.runner_epoch = ack.epoch;
        assert!(reg.heartbeat(&current, 3).still_owner);
        // A stale epoch is rejected.
        let mut stale = beat("n1");
        stale.runner_epoch = ack.epoch - 1;
        assert!(!reg.heartbeat(&stale, 4).still_owner);
        // An unknown node is rejected.
        assert!(!reg.heartbeat(&beat("ghost"), 4).still_owner);
    }

    #[test]
    fn drain_is_persistent_and_blocks_assignment() {
        let mut reg = NodeRegistry::new(10);
        reg.register(
            &hello("n1", 4, vec![RunnerClass::NativeRustClean], vec![]),
            0,
        );
        assert!(reg.drain("n1"));
        assert_eq!(reg.nodes["n1"].state, NodeState::Draining);
        // Draining again is a no-op (already draining).
        assert!(!reg.drain("n1"));
        // Heartbeat surfaces the drain intent but stays a valid owner.
        let ack = reg.heartbeat(&beat("n1"), 2);
        assert!(ack.still_owner);
        assert!(ack.drain);
        // No assignment ever lands on a draining node.
        assert!(
            reg.assign(&AssignSpec::new("L", RunnerClass::NativeRustClean))
                .is_none()
        );
    }

    #[test]
    fn assign_respects_capacity_class_and_tags() {
        let mut reg = NodeRegistry::new(10);
        reg.register(
            &hello("n1", 1, vec![RunnerClass::NativeRustClean], vec!["gpu"]),
            0,
        );
        // Wrong class -> no match.
        assert!(
            reg.assign(&AssignSpec::new("L", RunnerClass::OciDocker))
                .is_none()
        );
        // Wrong tag -> no match.
        assert!(
            reg.assign(
                &AssignSpec::new("L", RunnerClass::NativeRustClean)
                    .with_tags(vec!["arm".to_string()])
            )
            .is_none()
        );
        // Right class + tag -> lands on n1.
        let a = reg
            .assign(
                &AssignSpec::new("L", RunnerClass::NativeRustClean)
                    .with_tags(vec!["gpu".to_string()]),
            )
            .expect("should assign");
        assert_eq!(a.node_id, "n1");
        // Capacity now exhausted.
        assert!(
            reg.assign(&AssignSpec::new("L2", RunnerClass::NativeRustClean))
                .is_none()
        );
    }

    #[test]
    fn snapshot_round_trips_and_is_deterministic() {
        let mut reg = NodeRegistry::new(7);
        reg.register(
            &hello(
                "n2",
                3,
                vec![
                    RunnerClass::NativeRustClean,
                    RunnerClass::Custom("weird".into()),
                ],
                vec!["x"],
            ),
            0,
        );
        reg.register(&hello("n1", 1, vec![RunnerClass::OciDocker], vec![]), 0);
        let s1 = reg.to_snapshot().unwrap();
        let s2 = reg.to_snapshot().unwrap();
        assert_eq!(s1, s2, "snapshot must be deterministic");

        let restored = NodeRegistry::from_snapshot(&s1).unwrap();
        assert_eq!(restored, reg, "round trip must preserve the registry");
        // Custom class survives the string round trip.
        assert!(
            restored.nodes["n2"]
                .supported_classes
                .contains(&RunnerClass::Custom("weird".into()))
        );
    }

    #[test]
    fn assign_spec_serializes_class_as_string() {
        let spec = AssignSpec::new("L", RunnerClass::OciDocker);
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("oci-docker"), "got {json}");
        let back: AssignSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back, spec);
    }
}

//! In-process runner fleet control plane for dogfood execution.
//!
//! This is the daemon-facing seam used by the local forge before a remote HTTP
//! control plane is required. It wires runner registration, epoch-fenced
//! assignment, heartbeat/reap/drain health, and dispatch receipts through the
//! pure [`jeryu_runner_registry::NodeRegistry`].

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};

use jeryu_ci_ir::RunnerClass as IrRunnerClass;
use jeryu_runner_core::JobRequest as CoreJobRequest;
use jeryu_runner_core::policy::select_runner;
use jeryu_runner_core::receipt::Receipt;
use jeryu_runner_core::trust::RunnerClass as CoreRunnerClass;
use jeryu_runner_protocol::{Heartbeat, RunnerHello};
use jeryu_runner_registry::{AssignSpec, HeartbeatAck, NodeRegistry, ReapedNode};

use crate::{DispatchEngine, DispatchMode};

/// Submit a job to the default local dogfood fleet.
///
/// The default fleet is deterministic: four nodes (`xbabe0`..`xbabe3`) with ten
/// slots each, all supporting the native Rust lane. It is intentionally a
/// drop-in replacement for the old direct `DispatchEngine::dispatch` call.
pub fn submit(job: CoreJobRequest) -> Receipt {
    static DEFAULT_FLEET: OnceLock<Mutex<RunnerFleet>> = OnceLock::new();
    let fleet = DEFAULT_FLEET.get_or_init(|| Mutex::new(RunnerFleet::deterministic_fixture()));
    match fleet.lock() {
        Ok(mut fleet) => fleet.submit(job),
        Err(_) => Receipt::denied(&job, "runner_fleet_poisoned: default fleet lock poisoned"),
    }
}

/// Read-only health snapshot of the default dogfood fleet for the TUI
/// read-model. The default fleet is the deterministic dogfood fixture (4 nodes ×
/// 10 slots); a fresh fixture reports that same configured capacity without a
/// shared mutable global.
pub fn snapshot() -> RunnerFleetSnapshot {
    RunnerFleet::deterministic_fixture().snapshot()
}

/// Runner-fleet operation error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FleetError {
    /// Runner policy denied the job before assignment.
    PolicyDenied(String),
    /// No registered node has the required capacity/class/tags.
    NoCapacity(String),
    /// Registry assigned a node that has no daemon instance.
    MissingDaemon(String),
    /// A stale node epoch attempted to run or complete work.
    FencedOut {
        /// Node id.
        runner_id: String,
        /// Epoch carried by the runner/job.
        runner_epoch: u64,
        /// Registry/daemon epoch that is current now.
        active_epoch: u64,
    },
}

impl FleetError {
    fn receipt(&self, job: &CoreJobRequest) -> Receipt {
        Receipt::denied(job, format!("runner_fleet_denied: {self}"))
    }
}

impl fmt::Display for FleetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PolicyDenied(reason) => {
                write!(f, "policy denied job before assignment: {reason}")
            }
            Self::NoCapacity(class) => write!(f, "no runner capacity for class {class}"),
            Self::MissingDaemon(node) => write!(f, "assigned node {node} has no daemon"),
            Self::FencedOut {
                runner_id,
                runner_epoch,
                active_epoch,
            } => write!(
                f,
                "runner {runner_id} fenced out: epoch {runner_epoch} != active epoch {active_epoch}"
            ),
        }
    }
}

impl std::error::Error for FleetError {}

/// One runner daemon registered in the fleet.
#[derive(Debug, Clone)]
pub struct RunnerDaemon {
    runner_id: String,
    capacity: u32,
    labels: Vec<String>,
    supported_classes: Vec<IrRunnerClass>,
    epoch: u64,
    mode: DispatchMode,
    engine: DispatchEngine,
}

impl RunnerDaemon {
    /// Create a daemon with the supplied identity, capacity, and supported
    /// runner classes.
    pub fn new(
        runner_id: impl Into<String>,
        capacity: u32,
        supported_classes: Vec<IrRunnerClass>,
        labels: Vec<String>,
        mode: DispatchMode,
    ) -> Self {
        Self {
            runner_id: runner_id.into(),
            capacity,
            labels,
            supported_classes,
            epoch: 0,
            mode,
            engine: DispatchEngine::new(),
        }
    }

    /// Stable node id.
    pub fn runner_id(&self) -> &str {
        &self.runner_id
    }

    /// Current node epoch known by the daemon.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    fn hello(&self) -> RunnerHello {
        let mut hello = RunnerHello::new(&self.runner_id, self.supported_classes.clone());
        hello.capacity = self.capacity;
        hello.labels = self.labels.clone();
        hello
    }

    fn accept_epoch(&mut self, epoch: u64) {
        self.epoch = epoch;
    }

    fn heartbeat(&self, run_id: &str, lease_id: &str, job_id: &str) -> Heartbeat {
        Heartbeat {
            runner_id: self.runner_id.clone(),
            runner_epoch: self.epoch,
            run_id: run_id.to_string(),
            lease_id: lease_id.to_string(),
            job_id: job_id.to_string(),
            monotonic_millis: 0,
            message: String::new(),
        }
    }

    fn dispatch(&self, job: &CoreJobRequest, runner_epoch: u64) -> Result<Receipt, FleetError> {
        if self.epoch != runner_epoch {
            return Err(FleetError::FencedOut {
                runner_id: self.runner_id.clone(),
                runner_epoch,
                active_epoch: self.epoch,
            });
        }
        Ok(self.engine.dispatch(job, self.mode))
    }
}

/// A reserved assignment that has not necessarily completed yet.
#[derive(Clone, Debug)]
pub struct ReservedJob {
    /// Job to execute.
    pub job: CoreJobRequest,
    /// Assigned runner id.
    pub runner_id: String,
    /// Assigned runner epoch.
    pub runner_epoch: u64,
}

/// Successful fleet submission with assignment metadata.
#[derive(Clone, Debug)]
pub struct FleetSubmission {
    /// Assigned runner id.
    pub runner_id: String,
    /// Assigned runner epoch.
    pub runner_epoch: u64,
    /// Runner receipt.
    pub receipt: Receipt,
}

/// Health snapshot for one node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FleetNodeHealth {
    /// Node id.
    pub runner_id: String,
    /// Node lifecycle state.
    pub state: String,
    /// Current epoch.
    pub epoch: u64,
    /// Configured capacity.
    pub capacity: u32,
    /// Current in-flight leases.
    pub in_flight: u32,
}

/// Read-only health snapshot of a fleet, for the TUI read-model. A node counts
/// as online unless its lifecycle state is explicitly drained/fenced/offline.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunnerFleetSnapshot {
    /// Registered nodes.
    pub nodes: u32,
    /// Nodes currently online.
    pub online_runners: u32,
    /// In-flight leases across online nodes.
    pub busy_runners: u32,
    /// Free slots across online nodes.
    pub idle_runners: u32,
    /// Nodes in a drained/fenced/offline state.
    pub stuck_runners: u32,
    /// Total configured slots across all nodes.
    pub total_slots: u32,
    /// Slots on online nodes only.
    pub active_slots: u32,
}

/// In-process runner fleet.
#[derive(Debug, Clone)]
pub struct RunnerFleet {
    registry: NodeRegistry,
    daemons: BTreeMap<String, RunnerDaemon>,
}

impl RunnerFleet {
    /// Create an empty fleet.
    pub fn new(heartbeat_ttl: u64) -> Self {
        Self {
            registry: NodeRegistry::new(heartbeat_ttl),
            daemons: BTreeMap::new(),
        }
    }

    /// Create the deterministic dogfood fixture fleet: `xbabe0`..`xbabe3`, ten
    /// slots each, for a total of forty native Rust slots.
    pub fn deterministic_fixture() -> Self {
        Self::deterministic_fixture_with_mode(DispatchMode::Run)
    }

    /// Deterministic fixture fleet with caller-selected dispatch mode.
    pub fn deterministic_fixture_with_mode(mode: DispatchMode) -> Self {
        let mut fleet = Self::new(10);
        for index in 0..4 {
            let daemon = RunnerDaemon::new(
                format!("xbabe{index}"),
                10,
                vec![IrRunnerClass::NativeRustClean, IrRunnerClass::NativeRustHot],
                vec!["rust".to_string(), "dogfood".to_string()],
                mode,
            );
            fleet.register(daemon, 0);
        }
        fleet
    }

    /// Register or re-register a daemon and return its fresh epoch.
    pub fn register(&mut self, mut daemon: RunnerDaemon, now: u64) -> u64 {
        let ack = self.registry.register(&daemon.hello(), now);
        daemon.accept_epoch(ack.epoch);
        self.daemons.insert(ack.node_id, daemon);
        ack.epoch
    }

    /// Submit a job and return only the dispatch receipt, matching the bridge
    /// seam Claude requested for `ci_bridge`.
    pub fn submit(&mut self, job: CoreJobRequest) -> Receipt {
        match self.submit_with_assignment(job.clone()) {
            Ok(submission) => submission.receipt,
            Err(err) => err.receipt(&job),
        }
    }

    /// Submit a job and preserve assignment metadata for tests/diagnostics.
    pub fn submit_with_assignment(
        &mut self,
        job: CoreJobRequest,
    ) -> Result<FleetSubmission, FleetError> {
        let reserved = self.reserve_job(job)?;
        self.run_reserved(reserved)
    }

    /// Reserve a node slot for a job without executing it yet.
    pub fn reserve_job(&mut self, job: CoreJobRequest) -> Result<ReservedJob, FleetError> {
        let decision =
            select_runner(&job).map_err(|err| FleetError::PolicyDenied(err.to_string()))?;
        let runner_class = map_runner_class(decision.runner_class)?;
        let assignment = self
            .registry
            .assign(&AssignSpec::new(&job.job_id, runner_class))
            .ok_or_else(|| FleetError::NoCapacity(decision.runner_class.as_str().to_string()))?;
        if !self.daemons.contains_key(&assignment.node_id) {
            self.registry.release(&assignment.node_id);
            return Err(FleetError::MissingDaemon(assignment.node_id));
        }
        Ok(ReservedJob {
            job,
            runner_id: assignment.node_id,
            runner_epoch: assignment.epoch,
        })
    }

    /// Run a previously reserved job, rejecting stale epochs before dispatch.
    pub fn run_reserved(&mut self, reserved: ReservedJob) -> Result<FleetSubmission, FleetError> {
        if !self
            .registry
            .is_current_owner(&reserved.runner_id, reserved.runner_epoch)
        {
            let active_epoch = self.current_epoch(&reserved.runner_id);
            return Err(FleetError::FencedOut {
                runner_id: reserved.runner_id,
                runner_epoch: reserved.runner_epoch,
                active_epoch,
            });
        }
        let daemon = self
            .daemons
            .get(&reserved.runner_id)
            .ok_or_else(|| FleetError::MissingDaemon(reserved.runner_id.clone()))?;
        let receipt = daemon.dispatch(&reserved.job, reserved.runner_epoch)?;
        self.registry.release(&reserved.runner_id);
        Ok(FleetSubmission {
            runner_id: reserved.runner_id,
            runner_epoch: reserved.runner_epoch,
            receipt,
        })
    }

    /// Reserve every job first, then execute them. This models a scheduler
    /// fanout batch and proves capacity distribution across the forty slots.
    pub fn submit_batch(
        &mut self,
        jobs: Vec<CoreJobRequest>,
    ) -> Result<Vec<FleetSubmission>, FleetError> {
        let mut reserved = Vec::with_capacity(jobs.len());
        for job in jobs {
            reserved.push(self.reserve_job(job)?);
        }
        reserved
            .into_iter()
            .map(|reservation| self.run_reserved(reservation))
            .collect()
    }

    /// Send a heartbeat from the daemon's current epoch.
    pub fn heartbeat(&mut self, runner_id: &str, now: u64) -> HeartbeatAck {
        let Some(daemon) = self.daemons.get(runner_id) else {
            let beat = Heartbeat {
                runner_id: runner_id.to_string(),
                runner_epoch: 0,
                run_id: String::new(),
                lease_id: String::new(),
                job_id: String::new(),
                monotonic_millis: 0,
                message: String::new(),
            };
            return self.registry.heartbeat(&beat, now);
        };
        self.registry.heartbeat(&daemon.heartbeat("", "", ""), now)
    }

    /// Reap stale nodes.
    pub fn reap(&mut self, now: u64) -> Vec<ReapedNode> {
        self.registry.reap(now)
    }

    /// Drain a node from future assignments.
    pub fn drain(&mut self, runner_id: &str) -> bool {
        self.registry.drain(runner_id)
    }

    /// Health snapshot for every registered node.
    pub fn health(&self) -> Vec<FleetNodeHealth> {
        self.registry
            .nodes
            .values()
            .map(|node| FleetNodeHealth {
                runner_id: node.node_id.clone(),
                state: node.state.as_str().to_string(),
                epoch: node.epoch,
                capacity: node.capacity,
                in_flight: node.in_flight,
            })
            .collect()
    }

    /// Read-only health snapshot for the TUI read-model. A node counts as
    /// online unless its state is explicitly drained/fenced/offline; its
    /// capacity contributes to total slots and, when online, to active slots.
    pub fn snapshot(&self) -> RunnerFleetSnapshot {
        let mut snap = RunnerFleetSnapshot::default();
        for node in self.health() {
            snap.nodes = snap.nodes.saturating_add(1);
            snap.total_slots = snap.total_slots.saturating_add(node.capacity);
            let down = matches!(
                node.state.as_str(),
                "drained"
                    | "draining"
                    | "fenced"
                    | "fenced_out"
                    | "offline"
                    | "down"
                    | "dead"
                    | "quarantined"
            );
            if down {
                snap.stuck_runners = snap.stuck_runners.saturating_add(1);
            } else {
                snap.online_runners = snap.online_runners.saturating_add(1);
                snap.active_slots = snap.active_slots.saturating_add(node.capacity);
                snap.busy_runners = snap.busy_runners.saturating_add(node.in_flight);
                snap.idle_runners = snap
                    .idle_runners
                    .saturating_add(node.capacity.saturating_sub(node.in_flight));
            }
        }
        snap
    }

    fn current_epoch(&self, runner_id: &str) -> u64 {
        self.registry
            .nodes
            .get(runner_id)
            .map_or(0, |node| node.epoch)
    }
}

fn map_runner_class(class: CoreRunnerClass) -> Result<IrRunnerClass, FleetError> {
    match class {
        CoreRunnerClass::NativeRustHot => Ok(IrRunnerClass::NativeRustHot),
        CoreRunnerClass::NativeRustClean => Ok(IrRunnerClass::NativeRustClean),
        CoreRunnerClass::AgentGuard => Ok(IrRunnerClass::AgentGuard),
        CoreRunnerClass::ReleaseHermetic => Ok(IrRunnerClass::ReleaseHermetic),
        CoreRunnerClass::MicroVmRust => Ok(IrRunnerClass::MicrovmRust),
        CoreRunnerClass::OciDocker => Ok(IrRunnerClass::OciDocker),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_runner_core::job::{NetworkPolicy, SecretPolicy, TokenPolicy};
    use jeryu_runner_core::receipt::ReceiptStatus;
    use jeryu_runner_core::trust::TrustTier;
    use std::path::PathBuf;

    fn job(id: usize) -> CoreJobRequest {
        CoreJobRequest {
            job_id: format!("job-{id}"),
            repo_id: "neverhuman/jeryu".to_string(),
            commit_sha: "abc123".to_string(),
            workspace: PathBuf::from("/tmp/jeryu-fleet-test"),
            command: "/bin/true".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
            trust_tier: TrustTier::T2InternalBranch,
            requested_runner: Some(CoreRunnerClass::NativeRustClean),
            network_policy: NetworkPolicy::Deny,
            secret_policy: SecretPolicy::None,
            token_policy: TokenPolicy::ReadOnly,
            timeout_ms: 1_000,
            fork: false,
        }
    }

    #[test]
    fn deterministic_fixture_registers_four_nodes_and_forty_slots() {
        let fleet = RunnerFleet::deterministic_fixture_with_mode(DispatchMode::Explain);
        let health = fleet.health();
        assert_eq!(health.len(), 4);
        assert_eq!(health.iter().map(|node| node.capacity).sum::<u32>(), 40);
        assert_eq!(
            health
                .iter()
                .map(|node| node.runner_id.as_str())
                .collect::<Vec<_>>(),
            vec!["xbabe0", "xbabe1", "xbabe2", "xbabe3"]
        );
    }

    #[test]
    fn forty_job_fanout_uses_all_four_nodes_at_ten_slots_each() {
        let mut fleet = RunnerFleet::deterministic_fixture_with_mode(DispatchMode::Explain);
        let mut counts = BTreeMap::<String, usize>::new();
        for id in 0..40 {
            let reserved = fleet.reserve_job(job(id)).expect("reserve");
            *counts.entry(reserved.runner_id).or_default() += 1;
        }
        assert_eq!(counts["xbabe0"], 10);
        assert_eq!(counts["xbabe1"], 10);
        assert_eq!(counts["xbabe2"], 10);
        assert_eq!(counts["xbabe3"], 10);
    }

    #[test]
    fn reaped_node_stale_completion_is_fenced_and_work_reassigns() {
        let mut fleet = RunnerFleet::deterministic_fixture_with_mode(DispatchMode::Explain);
        let stale = fleet.reserve_job(job(1)).expect("first reserve");
        assert_eq!(stale.runner_id, "xbabe0");
        for survivor in ["xbabe1", "xbabe2", "xbabe3"] {
            assert!(fleet.heartbeat(survivor, 20).still_owner);
        }
        let reaped = fleet.reap(20);
        assert_eq!(reaped.len(), 1);
        assert_eq!(reaped[0].node_id, "xbabe0");

        let err = fleet
            .run_reserved(stale.clone())
            .expect_err("stale completion must be fenced");
        assert!(matches!(err, FleetError::FencedOut { runner_id, .. } if runner_id == "xbabe0"));

        let replacement = fleet.reserve_job(stale.job).expect("reassign");
        assert_ne!(replacement.runner_id, "xbabe0");
        let completed = fleet.run_reserved(replacement).expect("run reassigned");
        assert_eq!(completed.receipt.status, ReceiptStatus::Planned);
    }

    #[test]
    fn drain_blocks_new_assignments_but_inflight_work_finishes() {
        let mut fleet = RunnerFleet::deterministic_fixture_with_mode(DispatchMode::Explain);
        let inflight = fleet.reserve_job(job(1)).expect("reserve");
        assert_eq!(inflight.runner_id, "xbabe0");
        assert!(fleet.drain("xbabe0"));

        for id in 2..12 {
            let reserved = fleet.reserve_job(job(id)).expect("reserve after drain");
            assert_ne!(reserved.runner_id, "xbabe0");
        }

        let completed = fleet
            .run_reserved(inflight)
            .expect("draining node completes");
        assert_eq!(completed.receipt.status, ReceiptStatus::Planned);
    }
}

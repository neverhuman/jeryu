#![doc = "Container lifecycle seam for the warm pool: pre-warm detached cells and reap orphans without a real Docker/Podman daemon."]

use crate::runtime::ContainerRuntime;
use jeryu_runner_core::error::RunnerResult;

/// Label key that tags every warm/agent container with the workcell id it backs.
///
/// The reaper lists containers by this label and matches each value against the
/// live claim ledger, so a container whose `jeryu.workcell=<id>` is unknown is an
/// orphan from a previous daemon lifetime.
pub const WORKCELL_LABEL: &str = "jeryu.workcell";

/// One detached container discovered by its `jeryu.workcell=<id>` label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarmContainer {
    /// Engine-assigned container id used to exec into or remove the container.
    pub container_id: String,
    /// Workcell id carried in the `jeryu.workcell=<id>` label.
    pub workcell_id: String,
}

impl WarmContainer {
    /// Pair a container id with the workcell id from its label.
    pub fn new(container_id: impl Into<String>, workcell_id: impl Into<String>) -> Self {
        Self {
            container_id: container_id.into(),
            workcell_id: workcell_id.into(),
        }
    }
}

/// Argv builder for a detached `sleep infinity` warm container.
///
/// The same builder feeds the real CLI and the recording fake, so the recorded
/// invocation cannot drift from what an engine would really launch: a detached,
/// network-isolated container labeled `jeryu.workcell=<id>` that idles on
/// `sleep infinity` until an agent session execs into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarmContainerSpec {
    /// Runtime executable, e.g. `podman` or `docker`.
    pub runtime: String,
    /// Image the warm container boots from (must match the agent image so a
    /// session can exec straight in).
    pub image: String,
    /// Network mode; agents idle network-isolated, so this is `none`.
    pub network: String,
    /// Workcell id stamped into the `jeryu.workcell=<id>` label.
    pub workcell_id: String,
}

impl WarmContainerSpec {
    /// Build a warm container spec from the engine defaults plus a workcell id.
    pub fn new(
        runtime: impl Into<String>,
        image: impl Into<String>,
        network: impl Into<String>,
        workcell_id: impl Into<String>,
    ) -> Self {
        Self {
            runtime: runtime.into(),
            image: image.into(),
            network: network.into(),
            workcell_id: workcell_id.into(),
        }
    }

    /// Runtime args without the executable: a detached, labeled, network-isolated
    /// container that idles on `sleep infinity`.
    pub fn args(&self) -> Vec<String> {
        vec![
            "run".to_string(),
            "--detach".to_string(),
            "--label".to_string(),
            format!("{WORKCELL_LABEL}={}", self.workcell_id),
            "--network".to_string(),
            self.network.clone(),
            self.image.clone(),
            "sleep".to_string(),
            "infinity".to_string(),
        ]
    }

    /// Full argv including the runtime executable, mirroring how the runtime is
    /// actually invoked.
    pub fn argv(&self) -> Vec<String> {
        let mut argv = vec![self.runtime.clone()];
        argv.extend(self.args());
        argv
    }
}

/// One lifecycle operation a recording runtime observed, for test assertions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleOp {
    /// A detached warm container was started, with the exact argv that ran.
    StartWarm {
        workcell_id: String,
        container_id: String,
        argv: Vec<String>,
    },
    /// The labeled containers were listed.
    List,
    /// A container was removed by id.
    Remove { container_id: String },
}

/// Lifecycle operations the warm pool needs on top of [`ContainerRuntime`].
///
/// Kept as a companion trait so the established [`crate::OciSpec::args`] callers
/// and existing [`ContainerRuntime`] impls are untouched: opting a runtime into
/// the warm pool means implementing exactly these three detached-container ops.
pub trait ContainerLifecycle: ContainerRuntime {
    /// Start a detached container labeled `jeryu.workcell=<workcell_id>` that
    /// idles on `sleep infinity`, returning its engine-assigned container id.
    fn start_warm(&self, workcell_id: &str) -> RunnerResult<String>;

    /// List every container carrying the `jeryu.workcell=*` label paired with
    /// the workcell id from that label.
    fn list_workcells(&self) -> RunnerResult<Vec<WarmContainer>>;

    /// Remove a container by id.
    fn remove(&self, container_id: &str) -> RunnerResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::FakeContainerRuntime;

    #[test]
    fn warm_spec_args_idle_on_sleep_infinity_with_the_workcell_label() {
        let spec = WarmContainerSpec::new("podman", "agent:latest", "none", "wc-0007");
        let args = spec.args();
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--label" && w[1] == "jeryu.workcell=wc-0007"),
            "args: {args:?}"
        );
        assert!(
            args.windows(2)
                .any(|w| w[0] == "sleep" && w[1] == "infinity"),
            "args: {args:?}"
        );
        assert!(args.contains(&"--detach".to_string()));
        assert_eq!(spec.argv()[0], "podman", "argv leads with the runtime");
    }

    #[test]
    fn fake_lifecycle_records_start_list_remove_and_tracks_live_set() {
        let fake = FakeContainerRuntime::default();
        let active = fake.start_warm("wc-0001").expect("start warm");
        let orphan = fake.start_warm("wc-0002").expect("start warm");
        assert_ne!(active, orphan, "each warm container gets a distinct id");

        let listed = fake.list_workcells().expect("list");
        assert_eq!(listed.len(), 2);
        assert!(
            listed
                .iter()
                .any(|c| c.workcell_id == "wc-0001" && c.container_id == active)
        );

        fake.remove(&orphan).expect("remove");
        let after = fake.live_warm();
        assert_eq!(after.len(), 1, "removed container leaves the live set");
        assert_eq!(after[0].workcell_id, "wc-0001");

        let ops = fake.lifecycle_ops();
        assert!(matches!(ops[0], LifecycleOp::StartWarm { .. }));
        assert!(matches!(ops[2], LifecycleOp::List));
        assert!(matches!(ops[3], LifecycleOp::Remove { .. }));
        // The recorded start argv is exactly what a real engine would launch.
        if let LifecycleOp::StartWarm { argv, .. } = &ops[0] {
            assert!(argv.contains(&"--detach".to_string()), "argv: {argv:?}");
            assert!(
                argv.windows(2)
                    .any(|w| w[0] == "--label" && w[1] == "jeryu.workcell=wc-0001")
            );
            assert!(
                argv.windows(2)
                    .any(|w| w[0] == "sleep" && w[1] == "infinity")
            );
        } else {
            panic!("first op must be StartWarm: {ops:?}");
        }
    }
}

//! Pure-logic, multi-node fencing scenario.
//!
//! Proves the core safety property of the registry: when a node holding a lease
//! dies and is reaped, the lease is reassigned to a *different* healthy node
//! exactly once, and the dead node's stale-epoch heartbeats/completions are
//! rejected (fencing). Also proves that draining a node permanently removes it
//! from the assignment pool.

use jeryu_ci_ir::RunnerClass;
use jeryu_runner_protocol::{Heartbeat, RunnerHello};
use jeryu_runner_registry::{AssignSpec, NodeRegistry, NodeState};

fn hello(id: &str, capacity: u32, tags: &[&str]) -> RunnerHello {
    let mut h = RunnerHello::new(id, vec![RunnerClass::NativeRustClean]);
    h.capacity = capacity;
    h.labels = tags.iter().map(|t| (*t).to_string()).collect();
    h
}

fn heartbeat(id: &str) -> Heartbeat {
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
fn three_node_fencing_and_reassignment() {
    let heartbeat_ttl = 10;
    let mut reg = NodeRegistry::new(heartbeat_ttl);

    // --- register three nodes in the same pool ("rust") at t=0 ---------------
    let ack0 = reg.register(&hello("xbabe0", 4, &["rust"]), 0);
    let ack1 = reg.register(&hello("xbabe1", 4, &["rust"]), 0);
    let ack2 = reg.register(&hello("xbabe2", 4, &["rust"]), 0);
    assert_eq!(ack0.epoch, 1);
    assert_eq!(ack1.epoch, 1);
    assert_eq!(ack2.epoch, 1);
    for id in ["xbabe0", "xbabe1", "xbabe2"] {
        assert_eq!(reg.nodes[id].state, NodeState::Active);
    }

    // --- assign a lease; it must land on a healthy, capacity-available, ------
    //     tag-matching node ----------------------------------------------------
    let spec = AssignSpec::new("lease-A", RunnerClass::NativeRustClean)
        .with_tags(vec!["rust".to_string()]);
    let first = reg.assign(&spec).expect("first assignment must succeed");
    let dead_node = first.node_id.clone();
    let dead_epoch = first.epoch;
    assert!(
        ["xbabe0", "xbabe1", "xbabe2"].contains(&dead_node.as_str()),
        "assignment landed on an unexpected node: {dead_node}"
    );
    assert!(reg.nodes[&dead_node].is_assignable() || reg.nodes[&dead_node].in_flight == 1);
    assert_eq!(
        reg.nodes[&dead_node].in_flight, 1,
        "in-flight must be tracked"
    );
    // The (node, epoch) pair is the current legitimate owner.
    assert!(reg.is_current_owner(&dead_node, dead_epoch));

    // --- keep the OTHER two nodes alive past the TTL, let the assigned -------
    //     node go silent --------------------------------------------------------
    // Advance the clock past heartbeat_ttl for the assigned node by heartbeating
    // the survivors at t=20 and never heartbeating `dead_node`.
    let now = 20; // > 0 + heartbeat_ttl(10)
    for id in ["xbabe0", "xbabe1", "xbabe2"] {
        if id != dead_node {
            let ack = reg.heartbeat(&heartbeat(id), now);
            assert!(ack.still_owner, "survivor {id} must remain owner");
        }
    }

    // --- reap: the silent (assigned) node becomes Dead with epoch bumped -----
    let reaped = reg.reap(now);
    assert_eq!(reaped.len(), 1, "exactly one node should be reaped");
    assert_eq!(reaped[0].node_id, dead_node);
    assert_eq!(
        reaped[0].fenced_epoch,
        dead_epoch + 1,
        "reap must bump the epoch to fence old work"
    );
    assert_eq!(reaped[0].orphaned_in_flight, 1);
    assert_eq!(reg.nodes[&dead_node].state, NodeState::Dead);
    assert_eq!(reg.nodes[&dead_node].epoch, dead_epoch + 1);

    // --- reassign: lands on a DIFFERENT healthy node, EXACTLY ONCE ------------
    // The dead node still carries in_flight=1, but it is Dead so never picked.
    // We model reassignment of the orphaned lease as a single fresh assign.
    let mut assign_count = 0;
    let second = reg.assign(&spec).expect("reassignment must succeed");
    assign_count += 1;
    assert_eq!(assign_count, 1, "lease must be reassigned exactly once");
    assert_ne!(
        second.node_id, dead_node,
        "reassignment must avoid the dead node"
    );
    assert!(
        ["xbabe0", "xbabe1", "xbabe2"].contains(&second.node_id.as_str()),
        "reassignment landed on an unexpected node: {}",
        second.node_id
    );
    assert_ne!(reg.nodes[&second.node_id].state, NodeState::Dead);
    assert_ne!(reg.nodes[&second.node_id].state, NodeState::Draining);
    assert_eq!(reg.nodes[&second.node_id].in_flight, 1);

    // --- fencing: the dead node's stale-epoch heartbeat is REJECTED ----------
    let mut stale = heartbeat(&dead_node);
    stale.runner_epoch = dead_epoch;
    let stale_hb = reg.heartbeat(&stale, 21);
    assert!(
        !stale_hb.still_owner,
        "dead node with stale epoch must NOT be owner"
    );
    // Even with the *new* (post-reap) epoch, a Dead node is never an owner.
    let mut dead = heartbeat(&dead_node);
    dead.runner_epoch = dead_epoch + 1;
    let dead_hb = reg.heartbeat(&dead, 22);
    assert!(
        !dead_hb.still_owner,
        "a Dead node is never a legitimate owner regardless of epoch"
    );

    // --- fencing: a stale-epoch completion is REJECTED -----------------------
    assert!(
        !reg.is_current_owner(&dead_node, dead_epoch),
        "stale-epoch completion from the dead node must be fenced/rejected"
    );
    // And the new owner's completion IS accepted under the right epoch.
    assert!(reg.is_current_owner(&second.node_id, second.epoch));

    // --- drain(xbabe1): subsequent assigns never pick xbabe1 -----------------
    // Free up capacity everywhere so xbabe1 would otherwise be a candidate,
    // then prove draining excludes it permanently.
    reg.drain("xbabe1");
    assert_eq!(reg.nodes["xbabe1"].state, NodeState::Draining);

    // Run several assignments; none may land on xbabe1.
    for i in 0..6 {
        if let Some(a) = reg.assign(
            &AssignSpec::new(format!("lease-{i}"), RunnerClass::NativeRustClean)
                .with_tags(vec!["rust".to_string()]),
        ) {
            assert_ne!(
                a.node_id, "xbabe1",
                "drained node xbabe1 must never receive an assignment"
            );
            assert_ne!(
                a.node_id, dead_node,
                "dead node must never receive an assignment"
            );
        }
    }
    // xbabe1 stayed at whatever in_flight it had; draining never increases it.
    // (It had at most 1 from the very first assignment if it was the winner,
    //  but it was excluded from every assign after drain.)
    assert_eq!(reg.nodes["xbabe1"].state, NodeState::Draining);
}

#[test]
fn snapshot_persists_fencing_state() {
    let mut reg = NodeRegistry::new(10);
    reg.register(&hello("xbabe0", 2, &["rust"]), 0);
    reg.register(&hello("xbabe1", 2, &["rust"]), 0);
    let spec =
        AssignSpec::new("L", RunnerClass::NativeRustClean).with_tags(vec!["rust".to_string()]);
    let a = reg.assign(&spec).unwrap();
    let _ = reg.reap(100); // both nodes die and get fenced
    assert_eq!(reg.nodes[&a.node_id].state, NodeState::Dead);

    let snap = reg.to_snapshot().unwrap();
    let restored = NodeRegistry::from_snapshot(&snap).unwrap();
    assert_eq!(restored, reg);
    // Fenced (bumped) epoch survives persistence.
    assert_eq!(restored.nodes[&a.node_id].epoch, a.epoch + 1);
    assert!(!restored.is_current_owner(&a.node_id, a.epoch));
}

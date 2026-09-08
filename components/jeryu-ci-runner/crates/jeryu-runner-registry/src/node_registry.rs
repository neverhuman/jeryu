use super::*;

impl NodeRegistry {
    /// Create an empty registry with the given heartbeat TTL.
    #[must_use]
    pub fn new(heartbeat_ttl: u64) -> Self {
        Self {
            nodes: BTreeMap::new(),
            heartbeat_ttl,
        }
    }

    /// Register (or re-register) a node from its `RunnerHello`.
    ///
    /// Transitions the node to `Active` and bumps its epoch. A brand-new node
    /// starts at epoch 1; a re-registering node gets a strictly higher epoch
    /// than it ever held before, fencing any prior incarnation.
    ///
    /// `now` is the current logical clock value and seeds `last_heartbeat_epoch`.
    pub fn register(&mut self, hello: &RunnerHello, now: u64) -> RegistrationAck {
        let node_id = hello.runner_id.clone();
        let next_epoch = self
            .nodes
            .get(&node_id)
            .map_or(1, |existing| existing.epoch.saturating_add(1));

        let record = NodeRecord {
            node_id: node_id.clone(),
            state: NodeState::Active,
            epoch: next_epoch,
            capacity: hello.capacity,
            // A re-registering node has, by definition, dropped its prior work.
            in_flight: 0,
            supported_classes: hello.supported_classes.clone(),
            tags: hello.labels.clone(),
            last_heartbeat_epoch: now,
        };
        self.nodes.insert(node_id.clone(), record);

        RegistrationAck {
            node_id,
            epoch: next_epoch,
            lease_ttl: self.heartbeat_ttl,
        }
    }

    /// Process a heartbeat, refreshing liveness if the epoch is current.
    ///
    /// A heartbeat carrying a stale epoch (or for an unknown node) is rejected
    /// with `still_owner = false` and does **not** refresh liveness — this is
    /// the fencing guarantee for a node that has been reaped or superseded.
    ///
    pub fn heartbeat(&mut self, beat: &Heartbeat, now: u64) -> HeartbeatAck {
        let Some(node) = self.nodes.get_mut(&beat.runner_id) else {
            return HeartbeatAck {
                still_owner: false,
                drain: false,
            };
        };

        // Fencing: a dead node, or a heartbeat with a stale/forged epoch, is
        // never the legitimate owner and never refreshes liveness.
        if node.state == NodeState::Dead || beat.runner_epoch != node.epoch {
            return HeartbeatAck {
                still_owner: false,
                drain: node.state == NodeState::Draining,
            };
        }

        node.last_heartbeat_epoch = now;
        HeartbeatAck {
            still_owner: true,
            drain: node.state == NodeState::Draining,
        }
    }

    /// Whether a (node, epoch) pair is the current legitimate owner.
    ///
    /// Useful for fencing a `complete`/result submission: a result carrying a
    /// stale epoch (e.g. from a node that has since been reaped) is rejected.
    #[must_use]
    pub fn is_current_owner(&self, node_id: &str, claimed_epoch: u64) -> bool {
        self.nodes
            .get(node_id)
            .is_some_and(|node| node.state != NodeState::Dead && node.epoch == claimed_epoch)
    }

    /// Reap nodes whose last heartbeat is older than `heartbeat_ttl`.
    ///
    /// Each reaped node transitions to `Dead` and has its epoch bumped to fence
    /// any in-flight work it may still be running. Returns one [`ReapedNode`]
    /// per node reaped on this sweep. Already-dead nodes are not reaped again.
    ///
    /// Iteration is over the `BTreeMap`, so the returned vector is ordered by
    /// `node_id` and is fully deterministic.
    pub fn reap(&mut self, now: u64) -> Vec<ReapedNode> {
        let mut reaped = Vec::new();
        for node in self.nodes.values_mut() {
            if node.state == NodeState::Dead {
                continue;
            }
            let deadline = node.last_heartbeat_epoch.saturating_add(self.heartbeat_ttl);
            if now > deadline {
                node.state = NodeState::Dead;
                node.epoch = node.epoch.saturating_add(1);
                reaped.push(ReapedNode {
                    node_id: node.node_id.clone(),
                    fenced_epoch: node.epoch,
                    orphaned_in_flight: node.in_flight,
                });
            }
        }
        reaped
    }

    /// Mark a node as draining. Persistent intent: the node stays `Draining`
    /// (it is never auto-promoted back to `Active`) until it re-registers.
    ///
    /// Returns `true` if the node existed and was eligible to drain (was
    /// `Active`); `false` for unknown, already-draining, or dead nodes.
    pub fn drain(&mut self, node_id: &str) -> bool {
        match self.nodes.get_mut(node_id) {
            Some(node) if node.state == NodeState::Active => {
                node.state = NodeState::Draining;
                true
            }
            _ => false,
        }
    }

    /// Attempt to place a lease.
    ///
    /// Picks the first node (in deterministic `node_id` order) that is `Active`,
    /// has spare capacity, supports the requested class, and carries every
    /// required tag. A `Draining` or `Dead` node is never selected.
    ///
    /// On success the chosen node's `in_flight` is incremented and an
    /// [`Assignment`] carrying the node's current epoch is returned.
    pub fn assign(&mut self, spec: &AssignSpec) -> Option<Assignment> {
        let chosen = self.nodes.values_mut().find(|node| {
            node.is_assignable()
                && node.supports_class(&spec.runner_class)
                && node.matches_tags(&spec.required_tags)
        })?;

        chosen.in_flight = chosen.in_flight.saturating_add(1);
        Some(Assignment {
            node_id: chosen.node_id.clone(),
            epoch: chosen.epoch,
        })
    }

    /// Release a lease previously assigned to `node_id`, decrementing its
    /// in-flight count. No-op for unknown nodes or nodes already at zero.
    pub fn release(&mut self, node_id: &str) {
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.in_flight = node.in_flight.saturating_sub(1);
        }
    }

    /// Produce a deterministic JSON snapshot for persistence.
    ///
    /// The snapshot is canonical: nodes are emitted in `node_id` order (a
    /// `BTreeMap` invariant) and `RunnerClass` values are encoded by their
    /// stable string form, so the same registry always serializes identically.
    ///
    /// # Errors
    ///
    /// Returns a [`serde_json::Error`] if serialization fails (effectively
    /// never, for this data shape).
    pub fn to_snapshot(&self) -> Result<String, serde_json::Error> {
        let wire = RegistrySnapshot::from(self);
        serde_json::to_string(&wire)
    }

    /// Reconstruct a registry from a snapshot produced by [`Self::to_snapshot`].
    ///
    /// # Errors
    ///
    /// Returns a [`serde_json::Error`] if the input is not a valid snapshot.
    pub fn from_snapshot(json: &str) -> Result<Self, serde_json::Error> {
        let wire: RegistrySnapshot = serde_json::from_str(json)?;
        Ok(Self::from(wire))
    }
}

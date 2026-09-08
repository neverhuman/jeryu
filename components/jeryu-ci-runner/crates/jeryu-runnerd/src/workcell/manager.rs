//! Workcell manager lifecycle implementation.

use super::*;

impl WorkcellManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_warm_pool(warm_cells: usize) -> Self {
        let mut manager = Self::new();
        for _ in 0..warm_cells {
            manager.spawn_ready_cell();
        }
        manager
    }

    pub fn ready_count(&self) -> usize {
        self.ready_queue.len()
    }

    pub fn workcell(&self, workcell_id: &str) -> Option<&WorkcellLease> {
        self.cells.get(workcell_id)
    }

    pub fn workcells(&self) -> Vec<WorkcellLease> {
        let mut leases: Vec<_> = self.cells.values().cloned().collect();
        leases.sort_by(|a, b| a.workcell_id.cmp(&b.workcell_id));
        leases
    }

    pub fn claim(&mut self, request: WorkcellClaimRequest) -> WorkcellResult<WorkcellLease> {
        let workcell_id = self
            .ready_queue
            .pop_front()
            .unwrap_or_else(|| self.spawn_ready_cell());
        let outcome = {
            let lease = self
                .cells
                .get_mut(&workcell_id)
                .expect("ready queue and lease table stay in sync");
            lease.apply_claim(&request);
            lease.apply_startup(&request.startup);
            match &request.startup {
                StartupSync::Rebased { .. } => Ok(()),
                StartupSync::Failed {
                    main_ref, reason, ..
                } => Err(WorkcellError::startup_rebase_failed(format!(
                    "workcell {} could not rebase onto {}: {}",
                    lease.workcell_id, main_ref, reason
                ))),
            }
        };

        // Keep the warm pool at a steady depth as soon as a warm cell is
        // consumed.
        self.spawn_ready_cell();

        let lease = self
            .cells
            .get(&workcell_id)
            .expect("ready queue and lease table stay in sync")
            .clone();
        match outcome {
            Ok(()) => Ok(lease),
            Err(err) => Err(err),
        }
    }

    /// Queue one warm cell and return its id so the warm pool can pair it with a
    /// detached `jeryu.workcell=<id>` container. The id matches the container
    /// label, which is what the reaper compares against the live ledger.
    pub fn warm_one(&mut self) -> String {
        self.spawn_ready_cell()
    }

    /// Claim the ready cell with this exact id — the warm pool has already paired
    /// it with a detached container labeled `jeryu.workcell=<id>`. Unlike
    /// [`WorkcellManager::claim`], the ready queue entry is consumed without
    /// minting a replacement, because the warm pool owns container refill.
    pub fn claim_ready(
        &mut self,
        workcell_id: &str,
        request: WorkcellClaimRequest,
    ) -> WorkcellResult<WorkcellLease> {
        if let Some(pos) = self.ready_queue.iter().position(|id| id == workcell_id) {
            self.ready_queue.remove(pos);
        }
        let cell = self.cells.get_mut(workcell_id).ok_or_else(|| {
            WorkcellError::claim_denied(format!("unknown warm cell {workcell_id}"))
        })?;
        cell.apply_claim(&request);
        cell.apply_startup(&request.startup);
        let outcome = match &request.startup {
            StartupSync::Rebased { .. } => Ok(()),
            StartupSync::Failed {
                main_ref, reason, ..
            } => Err(WorkcellError::startup_rebase_failed(format!(
                "workcell {workcell_id} could not rebase onto {main_ref}: {reason}"
            ))),
        };
        let lease = cell.clone();
        outcome.map(|()| lease)
    }

    pub fn repair_from_snapshot(
        &mut self,
        snapshot: &FrozenCiSnapshot,
        startup: StartupSync,
    ) -> WorkcellResult<WorkcellLease> {
        let request = WorkcellClaimRequest {
            agent_id: snapshot.branch_policy.agent_id.clone(),
            workspace_root: snapshot.workspace_root.clone(),
            repo_roots: snapshot.repo_roots.clone(),
            branch_budget: snapshot.branch_policy.max_branches,
            runner_id: snapshot.runner_id.clone(),
            runner_epoch: snapshot.runner_epoch.saturating_add(1),
            git_status_summary: snapshot.git_status_summary.clone(),
            ci_snapshot_age_ms: Some(snapshot.snapshot_age_ms),
            startup,
        };
        let mut lease = self.claim(request)?;
        let workcell_id = lease.workcell_id.clone();
        let cell = self
            .cells
            .get_mut(&workcell_id)
            .expect("claimed workcell must stay live");
        cell.state = WorkcellState::Held;
        cell.frozen_snapshot = Some(snapshot.clone());
        cell.branch_policy = snapshot.branch_policy.clone();
        cell.git_status_summary = snapshot.git_status_summary.clone();
        cell.ci_snapshot_age_ms = Some(snapshot.snapshot_age_ms);
        cell.heartbeat_healthy = snapshot.heartbeat_healthy;
        cell.failed_run_id = Some(snapshot.failed_run_id.clone());
        cell.failed_receipt_id = Some(snapshot.failed_receipt_id.clone());
        cell.allowed_paths = snapshot.allowed_paths.clone();
        cell.failure_log_digest = Some(snapshot.failure_log_digest.clone());
        lease = cell.clone();
        Ok(lease)
    }

    pub fn hold_failed_tree(
        &mut self,
        request: HoldFailedTreeRequest,
    ) -> WorkcellResult<WorkcellLease> {
        let HoldFailedTreeRequest {
            claim,
            ci_run_id,
            failed_run_id,
            failed_receipt_id,
            failure_log_digest,
        } = request;
        let mut lease = self.claim(claim)?;
        let workcell_id = lease.workcell_id.clone();
        let cell = self
            .cells
            .get_mut(&workcell_id)
            .expect("claimed workcell must stay live");
        cell.state = WorkcellState::Held;
        cell.failed_run_id = Some(failed_run_id.clone());
        cell.failed_receipt_id = Some(failed_receipt_id.clone());
        cell.failure_log_digest = Some(failure_log_digest.clone());
        cell.allowed_paths = cell
            .repo_roots
            .iter()
            .cloned()
            .chain(std::iter::once(cell.workspace_root.clone()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let snapshot_source = cell.clone();
        cell.frozen_snapshot = Some(FrozenCiSnapshot::from_workcell(
            ci_run_id,
            failed_run_id,
            failed_receipt_id,
            &snapshot_source,
            failure_log_digest,
            snapshot_source.ci_snapshot_age_ms.unwrap_or_default(),
        ));
        lease = cell.clone();
        Ok(lease)
    }

    pub fn begin_live_repair(
        &mut self,
        workcell_id: &str,
        runner_epoch: u64,
    ) -> WorkcellResult<WorkcellLease> {
        let cell = self.cells.get_mut(workcell_id).ok_or_else(|| {
            WorkcellError::epoch_fenced(format!("unknown workcell {workcell_id}"))
        })?;
        Self::require_epoch(cell, runner_epoch)?;
        if !matches!(cell.state, WorkcellState::Held) {
            return Err(WorkcellError::repair_state_denied(format!(
                "workcell {workcell_id} must be held before repair can start"
            )));
        }
        cell.state = WorkcellState::Repairing;
        Ok(cell.clone())
    }

    pub fn export_repair_branch(
        &mut self,
        workcell_id: &str,
        runner_epoch: u64,
        branch_suffix: impl Into<String>,
    ) -> WorkcellResult<String> {
        let cell = self.cells.get_mut(workcell_id).ok_or_else(|| {
            WorkcellError::epoch_fenced(format!("unknown workcell {workcell_id}"))
        })?;
        Self::require_epoch(cell, runner_epoch)?;
        if !matches!(cell.state, WorkcellState::Held | WorkcellState::Repairing) {
            return Err(WorkcellError::repair_state_denied(format!(
                "workcell {workcell_id} must be held or repairing before export"
            )));
        }
        cell.branch_policy.open_branch(branch_suffix)
    }

    pub fn heartbeat(
        &mut self,
        workcell_id: &str,
        runner_epoch: u64,
        heartbeat_healthy: bool,
    ) -> WorkcellResult<()> {
        let cell = self.cells.get_mut(workcell_id).ok_or_else(|| {
            WorkcellError::epoch_fenced(format!("unknown workcell {workcell_id}"))
        })?;
        Self::require_epoch(cell, runner_epoch)?;
        if !matches!(
            cell.state,
            WorkcellState::Claimed | WorkcellState::Held | WorkcellState::Repairing
        ) {
            return Err(WorkcellError::claim_denied(format!(
                "workcell {workcell_id} is not active"
            )));
        }
        cell.heartbeat_healthy = heartbeat_healthy;
        Ok(())
    }

    pub fn release(&mut self, workcell_id: &str, runner_epoch: u64) -> WorkcellResult<()> {
        let cell = self.cells.get_mut(workcell_id).ok_or_else(|| {
            WorkcellError::epoch_fenced(format!("unknown workcell {workcell_id}"))
        })?;
        Self::require_epoch(cell, runner_epoch)?;
        cell.state = WorkcellState::Released;
        cell.heartbeat_healthy = false;
        Ok(())
    }

    pub fn block(
        &mut self,
        workcell_id: &str,
        runner_epoch: u64,
        reason: impl Into<String>,
    ) -> WorkcellResult<()> {
        let cell = self.cells.get_mut(workcell_id).ok_or_else(|| {
            WorkcellError::epoch_fenced(format!("unknown workcell {workcell_id}"))
        })?;
        Self::require_epoch(cell, runner_epoch)?;
        cell.mark_blocked(reason);
        Ok(())
    }

    pub fn freeze_failed_ci_run(
        &self,
        workcell_id: &str,
        runner_epoch: u64,
        request: FreezeFailedCiRunRequest,
    ) -> WorkcellResult<FrozenCiSnapshot> {
        let cell = self.cells.get(workcell_id).ok_or_else(|| {
            WorkcellError::epoch_fenced(format!("unknown workcell {workcell_id}"))
        })?;
        Self::require_epoch(cell, runner_epoch)?;
        Ok(FrozenCiSnapshot::from_workcell(
            request.ci_run_id,
            request.failed_run_id,
            request.failed_receipt_id,
            cell,
            request.failure_log_digest,
            request.snapshot_age_ms,
        ))
    }

    fn spawn_ready_cell(&mut self) -> String {
        self.next_id = self.next_id.saturating_add(1);
        let workcell_id = format!("wc-{:04}", self.next_id);
        let lease = WorkcellLease::ready(workcell_id.clone());
        self.ready_queue.push_back(workcell_id.clone());
        self.cells.insert(workcell_id.clone(), lease);
        workcell_id
    }

    fn require_epoch(cell: &WorkcellLease, runner_epoch: u64) -> WorkcellResult<()> {
        if cell.runner_epoch != runner_epoch {
            return Err(WorkcellError::epoch_fenced(format!(
                "workcell {} fenced: epoch {} != active {}",
                cell.workcell_id, runner_epoch, cell.runner_epoch
            )));
        }
        Ok(())
    }
}

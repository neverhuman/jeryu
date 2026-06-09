#![doc = "Warm container pool: pre-warm detached agent cells so create-session reuses one with no cold start, and reap orphaned cells on daemon restart."]

use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

use jeryu_runner_core::error::{RunnerError, RunnerResult};
use jeryu_runner_core::job::JobRequest;
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_runner_oci::{AgentSessionPlan, ContainerLifecycle, plan_agent_session};

use crate::workcell::{WorkcellClaimRequest, WorkcellError, WorkcellLease, WorkcellManager};

/// One pre-warmed cell: a workcell id paired with the detached container that
/// backs it. The id is exactly the container's `jeryu.workcell=<id>` label, so
/// the reaper can match it against the live ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WarmEntry {
    workcell_id: String,
    container_id: String,
}

/// Everything create-session needs to materialize one agent session into a
/// claimed warm cell: the session identity, the latest-main checkout inputs, and
/// the workcell claim that drives the lease.
#[derive(Debug, Clone)]
pub struct SessionClaim {
    /// Repository owner / namespace.
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// Unique run identity for this session.
    pub run_id: String,
    /// Latest-main oid the session branch is pinned to.
    pub base_oid: String,
    /// Clone source for the isolated checkout.
    pub origin_url: String,
    /// Job the agent runs inside the confined container.
    pub job: JobRequest,
    /// Sandbox plan the hardened container is built from.
    pub plan: SandboxPlan,
    /// Workcell claim (agent id, runner epoch, startup rebase, branch budget).
    pub claim: WorkcellClaimRequest,
}

/// A warm cell handed to one agent session.
///
/// It carries the reused container id, the [`WorkcellLease`] (epoch-fenced, with
/// the agent/workcell branch policy), and the [`AgentSessionPlan`] whose branch
/// is the unique `agents/<agent_id>/sessions/<run_id>` namespace pinned to the
/// latest-main oid — never `main`.
#[derive(Debug, Clone)]
pub struct ClaimedCell {
    /// Reused (or cold-started) detached container id.
    pub container_id: String,
    /// Epoch-fenced workcell lease for the claimed cell.
    pub lease: WorkcellLease,
    /// Session-launch plan: isolated checkout on the unique session branch.
    pub session: AgentSessionPlan,
}

impl ClaimedCell {
    /// Branch the claimed session works on; namespaced and never `main`.
    pub fn branch(&self) -> &str {
        &self.session.branch
    }
}

/// Maintains a steady depth of detached `sleep infinity` agent containers so a
/// create-session claim reuses a warm container with no cold start, and reaps
/// orphaned containers left behind by a previous daemon lifetime.
#[derive(Debug)]
pub struct WarmPool {
    runtime: Arc<dyn ContainerLifecycle>,
    manager: WorkcellManager,
    target: usize,
    warm: VecDeque<WarmEntry>,
}

impl WarmPool {
    /// Build a warm pool and pre-warm it to `target` detached containers.
    pub fn new(runtime: Arc<dyn ContainerLifecycle>, target: usize) -> RunnerResult<Self> {
        let mut pool = Self {
            runtime,
            manager: WorkcellManager::new(),
            target,
            warm: VecDeque::new(),
        };
        pool.refill()?;
        Ok(pool)
    }

    /// Number of warm containers currently idling in the pool.
    pub fn warm_depth(&self) -> usize {
        self.warm.len()
    }

    /// Target steady-state depth the pool refills back to after each claim.
    pub fn target(&self) -> usize {
        self.target
    }

    /// Container ids of the warm (unclaimed) cells, sorted for determinism.
    pub fn warm_container_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .warm
            .iter()
            .map(|entry| entry.container_id.clone())
            .collect();
        ids.sort();
        ids
    }

    /// Claim a warm cell for one agent session, reusing a pre-warmed container
    /// when the pool is non-empty and cold-starting one only when it is drained.
    /// The agent's workspace is materialized as a latest-main checkout on the
    /// unique session branch, and the pool refills back to `target`.
    pub fn claim(&mut self, request: SessionClaim) -> RunnerResult<ClaimedCell> {
        let SessionClaim {
            owner,
            repo,
            run_id,
            base_oid,
            origin_url,
            job,
            plan,
            claim,
        } = request;

        let entry = match self.warm.pop_front() {
            Some(entry) => entry,
            None => self.warm_cell()?,
        };
        let agent_id = claim.agent_id.clone();
        let lease = self
            .manager
            .claim_ready(&entry.workcell_id, claim)
            .map_err(workcell_to_runner);
        // Restore steady-state depth even when the claimed cell blocks on a
        // failed startup rebase, so the next session still reuses a warm cell.
        self.refill()?;
        let lease = lease?;

        let session = plan_agent_session(
            &owner,
            &repo,
            &agent_id,
            &run_id,
            &base_oid,
            &origin_url,
            &job,
            &plan,
        )?;

        Ok(ClaimedCell {
            container_id: entry.container_id,
            lease,
            session,
        })
    }

    /// Remove every `jeryu.workcell=*` container whose label id is NOT in the
    /// active-lease ledger, returning the reaped container ids. Run this on
    /// daemon restart BEFORE refilling, so containers stranded by a previous
    /// lifetime are cleaned while live leases are kept.
    pub fn reap_orphans(&self, active_lease_ids: &BTreeSet<String>) -> RunnerResult<Vec<String>> {
        let mut reaped = Vec::new();
        for container in self.runtime.list_workcells()? {
            if !active_lease_ids.contains(&container.workcell_id) {
                self.runtime.remove(&container.container_id)?;
                reaped.push(container.container_id);
            }
        }
        reaped.sort();
        Ok(reaped)
    }

    fn refill(&mut self) -> RunnerResult<()> {
        while self.warm.len() < self.target {
            let entry = self.warm_cell()?;
            self.warm.push_back(entry);
        }
        Ok(())
    }

    fn warm_cell(&mut self) -> RunnerResult<WarmEntry> {
        let workcell_id = self.manager.warm_one();
        let container_id = self.runtime.start_warm(&workcell_id)?;
        Ok(WarmEntry {
            workcell_id,
            container_id,
        })
    }
}

fn workcell_to_runner(err: WorkcellError) -> RunnerError {
    RunnerError::new(err.reason, err.message().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workcell::{StartupSync, WorkcellState};
    use jeryu_runner_core::job::{NetworkPolicy, SecretPolicy, TokenPolicy};
    use jeryu_runner_core::policy::select_runner;
    use jeryu_runner_core::trust::{RunnerClass, TrustTier};
    use jeryu_runner_oci::{FakeContainerRuntime, LifecycleOp};
    use std::path::PathBuf;

    fn agent_job() -> JobRequest {
        JobRequest {
            job_id: "agent".to_string(),
            repo_id: "jeryu/jeryu".to_string(),
            commit_sha: "abc".to_string(),
            workspace: PathBuf::from("/tmp/agent-ws"),
            command: "/opt/jeryu/bin/codex".to_string(),
            args: vec!["exec".to_string()],
            env: Default::default(),
            trust_tier: TrustTier::T4ForkPr,
            requested_runner: Some(RunnerClass::OciDocker),
            network_policy: NetworkPolicy::Deny,
            secret_policy: SecretPolicy::None,
            token_policy: TokenPolicy::None,
            timeout_ms: 1000,
            fork: true,
        }
    }

    fn sandbox_plan(job: &JobRequest) -> SandboxPlan {
        let decision = select_runner(job).unwrap_or_else(|err| panic!("{err}"));
        SandboxPlan::from_decision(&job.workspace, &decision)
    }

    fn session_claim(agent_id: &str, run_id: &str, epoch: u64) -> SessionClaim {
        let job = agent_job();
        let plan = sandbox_plan(&job);
        SessionClaim {
            owner: "jeryu".to_string(),
            repo: "jeryu".to_string(),
            run_id: run_id.to_string(),
            base_oid: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string(),
            origin_url: "https://forge.invalid/jeryu/jeryu.git".to_string(),
            job: job.clone(),
            plan,
            claim: WorkcellClaimRequest {
                agent_id: agent_id.to_string(),
                workspace_root: job.workspace.clone(),
                repo_roots: vec![job.workspace.clone()],
                branch_budget: 1,
                runner_id: "xbabe0".to_string(),
                runner_epoch: epoch,
                git_status_summary: "clean".to_string(),
                ci_snapshot_age_ms: Some(0),
                startup: StartupSync::Rebased {
                    main_ref: "origin/main".to_string(),
                    base_sha: "abc123".to_string(),
                    head_sha: "def456".to_string(),
                },
            },
        }
    }

    // The pool pre-warms exactly N detached containers, and each one is created
    // idling on `sleep infinity` carrying the `jeryu.workcell=<id>` label — proven
    // by the fake's recorded StartWarm argv, not by inspecting live Docker.
    #[test]
    fn pool_pre_warms_exactly_n_sleep_infinity_labeled_containers() {
        let fake = Arc::new(FakeContainerRuntime::default());
        let pool = WarmPool::new(fake.clone(), 3).expect("warm the pool");
        assert_eq!(pool.warm_depth(), 3);
        assert_eq!(fake.live_warm().len(), 3, "exactly N live containers");

        let starts = fake
            .lifecycle_ops()
            .iter()
            .filter(|op| matches!(op, LifecycleOp::StartWarm { .. }))
            .count();
        assert_eq!(starts, 3, "exactly N warm starts");
        for op in fake.lifecycle_ops() {
            if let LifecycleOp::StartWarm {
                workcell_id, argv, ..
            } = op
            {
                assert!(
                    argv.windows(2)
                        .any(|w| w[0] == "--label"
                            && w[1] == format!("jeryu.workcell={workcell_id}")),
                    "argv must carry the workcell label: {argv:?}"
                );
                assert!(
                    argv.windows(2)
                        .any(|w| w[0] == "sleep" && w[1] == "infinity"),
                    "warm container must idle on sleep infinity: {argv:?}"
                );
            }
        }
    }

    // A claim reuses a warm container: no NEW start happens during the claim
    // beyond the single refill that restores depth, and the pool returns to N.
    #[test]
    fn claim_reuses_a_warm_container_and_refills_to_target() {
        let fake = Arc::new(FakeContainerRuntime::default());
        let mut pool = WarmPool::new(fake.clone(), 2).expect("warm the pool");
        let warmed = pool.warm_container_ids();
        let starts_before = fake
            .lifecycle_ops()
            .iter()
            .filter(|op| matches!(op, LifecycleOp::StartWarm { .. }))
            .count();
        assert_eq!(starts_before, 2);

        let claimed = pool
            .claim(session_claim("agent-7", "run-42", 9))
            .expect("claim reuses a warm cell");
        assert!(
            warmed.contains(&claimed.container_id),
            "claim handed out a PRE-WARMED container, not a cold one: {} not in {warmed:?}",
            claimed.container_id
        );
        assert_eq!(pool.warm_depth(), 2, "pool refills back to target");

        let starts_after = fake
            .lifecycle_ops()
            .iter()
            .filter(|op| matches!(op, LifecycleOp::StartWarm { .. }))
            .count();
        assert_eq!(
            starts_after - starts_before,
            1,
            "only the single refill start, never a cold start for the claim itself"
        );
    }

    // A zero-depth pool holds no warm container, so the claim itself must
    // cold-start exactly one container instead of reusing a pre-warmed one.
    #[test]
    fn claim_on_empty_pool_cold_starts() {
        let fake = Arc::new(FakeContainerRuntime::default());
        let mut empty = WarmPool::new(fake.clone(), 0).expect("zero-depth pool");
        assert_eq!(empty.warm_depth(), 0);
        let starts_before = fake
            .lifecycle_ops()
            .iter()
            .filter(|op| matches!(op, LifecycleOp::StartWarm { .. }))
            .count();

        let cold = empty
            .claim(session_claim("agent-8", "run-2", 11))
            .expect("claim cold-starts on an empty pool");

        let starts_after = fake
            .lifecycle_ops()
            .iter()
            .filter(|op| matches!(op, LifecycleOp::StartWarm { .. }))
            .count();
        assert_eq!(
            starts_after - starts_before,
            1,
            "an empty pool cold-starts exactly one container for the claim"
        );
        assert!(!cold.container_id.is_empty());
    }

    // The reaper removes containers whose label id is NOT in the active ledger
    // and KEEPS the active ones, returning exactly the reaped ids.
    #[test]
    fn reap_orphans_removes_unknown_labels_and_keeps_active_leases() {
        let fake = Arc::new(FakeContainerRuntime::default());
        // Two cells survive a restart (still in the ledger) and two are orphans.
        let active_a = fake.start_warm("wc-active-1").expect("start");
        let orphan_a = fake.start_warm("wc-orphan-1").expect("start");
        let active_b = fake.start_warm("wc-active-2").expect("start");
        let orphan_b = fake.start_warm("wc-orphan-2").expect("start");

        // A zero-depth pool does not warm anything of its own, so the only
        // labeled containers are the four seeded above.
        let pool = WarmPool::new(fake.clone(), 0).expect("zero-depth pool");

        let mut active = BTreeSet::new();
        active.insert("wc-active-1".to_string());
        active.insert("wc-active-2".to_string());

        let mut reaped = pool.reap_orphans(&active).expect("reap");
        reaped.sort();
        let mut expected = vec![orphan_a.clone(), orphan_b.clone()];
        expected.sort();
        assert_eq!(
            reaped, expected,
            "exactly the orphan container ids are reaped"
        );

        let live: Vec<String> = fake
            .live_warm()
            .into_iter()
            .map(|c| c.container_id)
            .collect();
        assert!(live.contains(&active_a), "active lease kept");
        assert!(live.contains(&active_b), "active lease kept");
        assert!(!live.contains(&orphan_a), "orphan removed");
        assert!(!live.contains(&orphan_b), "orphan removed");
    }

    // The claimed cell rides the unique, namespaced session branch — never main —
    // and the lease is in the Claimed state under the agent's id.
    #[test]
    fn claimed_cell_carries_the_session_branch_namespace_never_main() {
        let fake = Arc::new(FakeContainerRuntime::default());
        let mut pool = WarmPool::new(fake, 1).expect("warm the pool");
        let claimed = pool
            .claim(session_claim("agent-7", "run-42", 9))
            .expect("claim succeeds");
        assert_eq!(claimed.branch(), "agents/agent-7/sessions/run-42");
        assert_ne!(claimed.branch(), "main");
        assert_ne!(claimed.branch(), "refs/heads/main");
        assert!(
            claimed
                .session
                .branch
                .starts_with("agents/agent-7/sessions/")
        );
        assert_eq!(claimed.lease.state, WorkcellState::Claimed);
        assert_eq!(claimed.lease.agent_id, "agent-7");
        // The lease workcell id matches the reused container's label dimension.
        assert!(claimed.lease.workcell_id.starts_with("wc-"));
    }

    // A failed startup rebase blocks the claim but the pool still refills, so the
    // next session is not starved of a warm cell.
    #[test]
    fn failed_startup_blocks_claim_but_pool_still_refills() {
        let fake = Arc::new(FakeContainerRuntime::default());
        let mut pool = WarmPool::new(fake, 2).expect("warm the pool");
        let mut request = session_claim("agent-9", "run-9", 13);
        request.claim.startup = StartupSync::Failed {
            main_ref: "origin/main".to_string(),
            base_sha: "abc123".to_string(),
            head_sha: "def456".to_string(),
            reason: "rebase conflict".to_string(),
        };
        let err = pool
            .claim(request)
            .expect_err("failed rebase blocks the claim");
        assert_eq!(err.code(), "workcell_startup_rebase_failed");
        assert_eq!(pool.warm_depth(), 2, "the pool refills despite the block");
    }
}

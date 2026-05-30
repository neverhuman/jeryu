//! Queue simulation helpers for the Phase 7 validation gate.

use crate::{DeterministicValidator, MergeQueue, QueueSummary};
use jeryu_core::phase7::{ChangedPath, PullRequest, PullRequestId, RepoId};
use jeryu_proof::{ChangeSet, ProofEvidence, default_phase7_engine};
use std::sync::{Arc, Mutex};
use std::thread;

/// Result of the 100 concurrent PR simulation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimulationReceipt {
    /// Requested PR count.
    pub requested_prs: usize,
    /// Queue summary after processing.
    pub summary: QueueSummary,
}

/// Runs the Phase 7 concurrent PR simulation. Each PR touches a unique path,
/// so all should pass speculative validation after proof witnessing.
pub fn run_concurrent_pr_simulation(prs: usize) -> SimulationReceipt {
    let queue = Arc::new(Mutex::new(MergeQueue::new()));
    let mut handles = Vec::new();

    for i in 0..prs {
        let queue = Arc::clone(&queue);
        handles.push(thread::spawn(move || {
            let pr = PullRequest::new(
                RepoId::new("repo_phase7"),
                PullRequestId::new(format!("pr_{i}")),
                format!("agent/pr-{i}"),
                "main",
                "base001",
                format!("head{i:03}"),
                vec![ChangedPath::new(format!(
                    "crates/jeryu_ci_scheduler/src/generated_unique_{i}.rs"
                ))],
            );
            let engine = default_phase7_engine();
            let plan = engine
                .plan(&ChangeSet {
                    repo: pr.repo.clone(),
                    pr: pr.id.clone(),
                    head_sha: pr.head_sha.clone(),
                    paths: pr.changed_paths.clone(),
                    agent_authored: false,
                })
                .expect("simulation paths must have proof routes");
            let evidence = plan
                .lanes
                .iter()
                .map(|lane| ProofEvidence {
                    lane: lane.name.clone(),
                    commands: lane.commands.clone(),
                    success: true,
                    log_digest: format!("digest:{}:{i}", lane.name),
                })
                .collect::<Vec<_>>();
            let witness = engine
                .verify(&plan, &evidence)
                .expect("simulation witness must mint");
            let mut guard = queue.lock().expect("simulation mutex must not be poisoned");
            guard
                .enqueue(pr, Some(witness))
                .expect("enqueue must succeed");
        }));
    }

    for handle in handles {
        handle.join().expect("simulation worker must not panic");
    }

    let mut guard = queue.lock().expect("simulation mutex must not be poisoned");
    let summary = guard.process_all(&DeterministicValidator);
    SimulationReceipt {
        requested_prs: prs,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::run_concurrent_pr_simulation;

    #[test]
    fn concurrent_100_pr_queue_simulation() {
        let receipt = run_concurrent_pr_simulation(100);
        assert_eq!(receipt.requested_prs, 100);
        assert_eq!(receipt.summary.processed, 100);
        assert_eq!(receipt.summary.mergeable, 100);
        assert_eq!(receipt.summary.conflicts, 0);
        assert_eq!(receipt.summary.failed_validation, 0);
    }
}

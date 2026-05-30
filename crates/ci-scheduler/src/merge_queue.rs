//! Merge queue implementation.

use forge_core::phase7::{
    JitForgeError, JitForgeResult, ProofWitness, PullRequest, QueueEntry, QueueEntryId,
    QueueEntryState, Receipt, ReceiptKind,
};
use std::collections::BTreeSet;

/// Speculative merge validation request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpeculativeValidation {
    /// Queue entry id.
    pub entry_id: QueueEntryId,
    /// Pull request.
    pub pr: PullRequest,
    /// Candidate synthetic SHA.
    pub speculative_sha: String,
    /// Paths already accepted ahead in the queue.
    pub accepted_paths_ahead: BTreeSet<String>,
}

/// Speculative validation outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationOutcome {
    /// Validation passed.
    Passed,
    /// Validation failed.
    Failed(String),
}

/// Validator used by the merge queue.
pub trait SpeculativeValidator {
    /// Validates one speculative merge candidate.
    fn validate(&self, validation: &SpeculativeValidation) -> ValidationOutcome;
}

/// Validator that always passes unless overlapping paths are already accepted.
#[derive(Clone, Debug, Default)]
pub struct DeterministicValidator;

impl SpeculativeValidator for DeterministicValidator {
    fn validate(&self, validation: &SpeculativeValidation) -> ValidationOutcome {
        let changed = validation.pr.changed_path_set();
        if let Some(conflict_path) = changed
            .intersection(&validation.accepted_paths_ahead)
            .next()
        {
            return ValidationOutcome::Failed(format!("path conflict: {conflict_path}"));
        }
        ValidationOutcome::Passed
    }
}

/// Merge queue process summary.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QueueSummary {
    /// Entries validated as mergeable.
    pub mergeable: usize,
    /// Entries dequeued because of path conflicts.
    pub conflicts: usize,
    /// Entries dequeued because validation failed.
    pub failed_validation: usize,
    /// Total entries processed.
    pub processed: usize,
}

/// FIFO merge queue. Admission requires a proof witness.
#[derive(Clone, Debug, Default)]
pub struct MergeQueue {
    entries: Vec<QueueEntry>,
    accepted_paths: BTreeSet<String>,
}

impl MergeQueue {
    /// Creates an empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a snapshot of entries.
    pub fn entries(&self) -> &[QueueEntry] {
        &self.entries
    }

    /// Enqueues a PR. Missing proof witness is a hard block.
    pub fn enqueue(
        &mut self,
        pr: PullRequest,
        proof_witness: Option<ProofWitness>,
    ) -> JitForgeResult<QueueEntryId> {
        let Some(proof_witness) = proof_witness else {
            return Err(JitForgeError::MissingProofWitness(format!(
                "PR {} cannot enter merge queue without proof witness",
                pr.id
            )));
        };
        if proof_witness.repo != pr.repo
            || proof_witness.pr != pr.id
            || proof_witness.head_sha != pr.head_sha
        {
            return Err(JitForgeError::MissingProofWitness(format!(
                "proof witness {} does not cover PR {} at head {}",
                proof_witness.id, pr.id, pr.head_sha
            )));
        }
        let pr_paths = pr.changed_path_set();
        let witness_paths = proof_witness
            .changed_paths
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if pr_paths != witness_paths {
            return Err(JitForgeError::MissingProofWitness(format!(
                "proof witness {} path set does not match PR {}",
                proof_witness.id, pr.id
            )));
        }
        let entry_id = QueueEntryId::fresh();
        let receipt = Receipt::new(
            ReceiptKind::MergeQueue,
            pr.repo.clone(),
            None,
            entry_id.to_string(),
            pr.head_sha.clone(),
            format!("PR {} admitted to merge queue", pr.id),
            vec!["merge_queue.enqueue".to_string()],
            "none: proof witness accepted",
        );
        self.entries.push(QueueEntry {
            id: entry_id.clone(),
            pr,
            proof_witness,
            speculative_sha: None,
            state: QueueEntryState::Queued,
            receipts: vec![receipt],
        });
        Ok(entry_id)
    }

    /// Processes all queued entries in order.
    pub fn process_all<V: SpeculativeValidator>(&mut self, validator: &V) -> QueueSummary {
        let mut summary = QueueSummary::default();
        let mut accepted_paths = self.accepted_paths.clone();
        for entry in &self.entries {
            if entry.state == QueueEntryState::Mergeable {
                accepted_paths.extend(entry.pr.changed_path_set());
            }
        }
        for entry in &mut self.entries {
            if entry.state != QueueEntryState::Queued {
                continue;
            }
            summary.processed += 1;
            entry.state = QueueEntryState::SpeculativeMergeTesting;
            let speculative_sha = synthetic_sha(&entry.pr);
            entry.speculative_sha = Some(speculative_sha.clone());
            let validation = SpeculativeValidation {
                entry_id: entry.id.clone(),
                pr: entry.pr.clone(),
                speculative_sha: speculative_sha.clone(),
                accepted_paths_ahead: accepted_paths.clone(),
            };
            match validator.validate(&validation) {
                ValidationOutcome::Passed => {
                    entry.state = QueueEntryState::Mergeable;
                    accepted_paths.extend(entry.pr.changed_path_set());
                    summary.mergeable += 1;
                    entry.receipts.push(Receipt::new(
                        ReceiptKind::MergeQueue,
                        entry.pr.repo.clone(),
                        None,
                        entry.id.to_string(),
                        speculative_sha,
                        "speculative merge validation passed",
                        vec!["merge_queue.speculative_validate".to_string()],
                        "none",
                    ));
                }
                ValidationOutcome::Failed(reason) if reason.starts_with("path conflict:") => {
                    entry.state = QueueEntryState::DequeuedConflict;
                    summary.conflicts += 1;
                    entry.receipts.push(Receipt::new(
                        ReceiptKind::MergeQueue,
                        entry.pr.repo.clone(),
                        None,
                        entry.id.to_string(),
                        entry.pr.head_sha.clone(),
                        format!("dequeued due to conflict: {reason}"),
                        vec!["merge_queue.conflict_dequeue".to_string()],
                        "requires rebase or manual resolution",
                    ));
                }
                ValidationOutcome::Failed(reason) => {
                    entry.state = QueueEntryState::DequeuedFailedValidation;
                    summary.failed_validation += 1;
                    entry.receipts.push(Receipt::new(
                        ReceiptKind::MergeQueue,
                        entry.pr.repo.clone(),
                        None,
                        entry.id.to_string(),
                        entry.pr.head_sha.clone(),
                        format!("dequeued due to failed validation: {reason}"),
                        vec!["merge_queue.validation_dequeue".to_string()],
                        "validation failure must be repaired",
                    ));
                }
            }
        }
        self.accepted_paths = accepted_paths;
        summary
    }

    /// Returns accepted path set after processing.
    pub fn accepted_paths(&self) -> &BTreeSet<String> {
        &self.accepted_paths
    }
}

fn synthetic_sha(pr: &PullRequest) -> String {
    format!(
        "synthetic-{}-{}-{}",
        pr.target_branch.replace('/', "_"),
        &pr.base_sha,
        &pr.head_sha
    )
}

#[cfg(test)]
mod tests {
    use super::{DeterministicValidator, MergeQueue};
    use forge_core::phase7::{ChangedPath, PullRequest, PullRequestId, QueueEntryState, RepoId};
    use proofcore::{default_phase7_engine, ChangeSet, ProofEvidence};

    fn witness_for(pr: &PullRequest) -> forge_core::phase7::ProofWitness {
        let engine = default_phase7_engine();
        let plan = engine
            .plan(&ChangeSet {
                repo: pr.repo.clone(),
                pr: pr.id.clone(),
                head_sha: pr.head_sha.clone(),
                paths: pr.changed_paths.clone(),
                agent_authored: false,
            })
            .expect("test paths have proof routes");
        let evidence = plan
            .lanes
            .iter()
            .map(|lane| ProofEvidence {
                lane: lane.name.clone(),
                commands: lane.commands.clone(),
                success: true,
                log_digest: format!("digest:{}", lane.name),
            })
            .collect::<Vec<_>>();
        engine.verify(&plan, &evidence).expect("witness")
    }

    fn pr(id: &str, path: &str, head: &str) -> PullRequest {
        PullRequest::new(
            RepoId::new("repo_phase7"),
            PullRequestId::new(id),
            format!("branch/{id}"),
            "main",
            "base001",
            head,
            vec![ChangedPath::new(path)],
        )
    }

    #[test]
    fn missing_proof_witness_blocks_admission() {
        let mut queue = MergeQueue::new();
        let err = queue
            .enqueue(
                pr("pr_1", "crates/ci-scheduler/src/lib.rs", "head001"),
                None,
            )
            .expect_err("missing witness must block queue admission");
        assert!(err.to_string().contains("without proof witness"));
    }

    #[test]
    fn conflict_dequeue_tests() {
        let mut queue = MergeQueue::new();
        let first = pr("pr_1", "crates/ci-scheduler/src/lib.rs", "head001");
        let second = pr("pr_2", "crates/ci-scheduler/src/lib.rs", "head002");
        queue
            .enqueue(first.clone(), Some(witness_for(&first)))
            .expect("enqueue first");
        queue
            .enqueue(second.clone(), Some(witness_for(&second)))
            .expect("enqueue second");
        let summary = queue.process_all(&DeterministicValidator);
        assert_eq!(summary.mergeable, 1);
        assert_eq!(summary.conflicts, 1);
        assert_eq!(queue.entries()[1].state, QueueEntryState::DequeuedConflict);
    }

    #[test]
    fn witness_repo_mismatch_blocks_admission() {
        let mut queue = MergeQueue::new();
        let change = pr("pr_1", "crates/ci-scheduler/src/lib.rs", "head001");
        let mut witness = witness_for(&change);
        witness.repo = RepoId::new("different_repo");
        let err = queue
            .enqueue(change, Some(witness))
            .expect_err("witness for another repo must block admission");
        assert!(err.to_string().contains("does not cover PR"));
    }

    #[test]
    fn later_process_detects_conflict_with_existing_mergeable_entry() {
        let mut queue = MergeQueue::new();
        let first = pr("pr_1", "crates/ci-scheduler/src/lib.rs", "head001");
        queue
            .enqueue(first.clone(), Some(witness_for(&first)))
            .expect("enqueue first");
        let first_summary = queue.process_all(&DeterministicValidator);
        assert_eq!(first_summary.mergeable, 1);

        let second = pr("pr_2", "crates/ci-scheduler/src/lib.rs", "head002");
        queue
            .enqueue(second.clone(), Some(witness_for(&second)))
            .expect("enqueue second");
        let second_summary = queue.process_all(&DeterministicValidator);
        assert_eq!(second_summary.processed, 1);
        assert_eq!(second_summary.conflicts, 1);
        assert_eq!(queue.entries()[1].state, QueueEntryState::DequeuedConflict);
    }
}

//! Job lease and retry state for deterministic CI schedules.
//!
//! This module is a thin re-export hub over cohesive submodules grouped by
//! responsibility:
//! - [`types`]: lease value types, replay receipts, and the leased request.
//! - [`error`]: lease operation errors.
//! - [`book`]: the stateful [`LeaseBook`] ledger and lease minting.

mod book;
mod error;
mod types;

pub use book::LeaseBook;
pub use error::LeaseError;
pub use types::{JobLease, JobLeaseState, LeaseEventKind, LeaseReceipt, LeasedJobRequest};

#[cfg(test)]
mod tests {
    use super::{JobLeaseState, LeaseBook, LeaseError, LeaseEventKind};
    use crate::deterministic_schedule;
    use jeryu_ci_ir::{
        CacheMode, CacheMount, Job, Pipeline, PipelineSource, RetryPolicy, RunnerClass, Step,
        TrustTier,
    };
    use jeryu_runner_protocol::{JobOutcome, JobResult};

    fn pipeline(max_attempts: u32) -> Pipeline {
        let mut pipeline = Pipeline::new(
            PipelineSource::NativeToml,
            "acme/repo",
            "abc",
            TrustTier::InternalBranch,
        );
        let mut job = Job::new("test", "test", RunnerClass::NativeRustClean);
        job.steps.push(Step::run("test_0", "test", "cargo test"));
        job.cache_mounts.push(CacheMount {
            name: "target".to_string(),
            path: "target/".to_string(),
            mode: CacheMode::ReadOnly,
            fingerprint: "fnv64:target".to_string(),
        });
        job.retry_policy = RetryPolicy {
            max_attempts,
            backoff_seconds: 0,
        };
        pipeline.jobs.push(job);
        pipeline
    }

    fn lease_book(max_attempts: u32) -> LeaseBook {
        let pipeline = pipeline(max_attempts);
        let schedule = deterministic_schedule(&pipeline).expect("schedule");
        LeaseBook::new("run-1", &pipeline, &schedule).expect("lease book")
    }

    #[test]
    fn active_acquire_is_idempotent_for_same_worker() {
        let mut leases = lease_book(1);
        let first = leases
            .acquire("test", "worker-a", 100, 30)
            .expect("first lease");
        let second = leases
            .acquire("test", "worker-a", 110, 30)
            .expect("idempotent lease");
        assert_eq!(first, second);
    }

    #[test]
    fn active_lease_blocks_other_worker_until_expiry() {
        let mut leases = lease_book(1);
        leases
            .acquire("test", "worker-a", 100, 30)
            .expect("first lease");
        let err = leases
            .acquire("test", "worker-b", 110, 30)
            .expect_err("active lease must block another worker");
        assert!(matches!(err, LeaseError::ActiveLease { .. }));

        let takeover = leases
            .acquire("test", "worker-b", 131, 30)
            .expect("expired lease can be acquired");
        assert_eq!(takeover.worker_id, "worker-b");
        assert_eq!(takeover.attempt, 1);
    }

    #[test]
    fn stale_worker_result_after_takeover_is_rejected() {
        let pipeline = pipeline(1);
        let schedule = deterministic_schedule(&pipeline).expect("schedule");
        let mut leases = LeaseBook::new("run-1", &pipeline, &schedule).expect("lease book");
        let stale = leases
            .acquire("test", "worker-a", 100, 30)
            .expect("first lease");
        let takeover = leases
            .acquire("test", "worker-b", 131, 30)
            .expect("takeover lease");
        assert_ne!(stale.id, takeover.id);
        assert!(matches!(
            leases.complete(&stale),
            Err(LeaseError::LeaseMismatch(_))
        ));
    }

    #[test]
    fn failed_attempt_requeues_until_retry_budget_is_exhausted() {
        let mut leases = lease_book(2);
        let first = leases
            .acquire("test", "worker-a", 100, 30)
            .expect("first lease");
        leases.fail(&first, "flake").expect("requeue");
        assert!(matches!(leases.state("test"), Some(JobLeaseState::Pending)));
        assert_eq!(leases.attempt("test"), Some(2));

        let second = leases
            .acquire("test", "worker-a", 140, 30)
            .expect("retry lease");
        assert_eq!(second.attempt, 2);
        assert_ne!(first.id, second.id);
        leases
            .fail(&second, "still failing")
            .expect("permanent fail");
        assert!(matches!(
            leases.state("test"),
            Some(JobLeaseState::Failed { attempts: 2, .. })
        ));
    }

    #[test]
    fn completed_job_cannot_be_released() {
        let mut leases = lease_book(1);
        let lease = leases.acquire("test", "worker-a", 100, 30).expect("lease");
        leases.complete(&lease).expect("complete");
        assert!(matches!(
            leases.acquire("test", "worker-a", 110, 30),
            Err(LeaseError::AlreadySucceeded(_))
        ));
    }

    #[test]
    fn acquire_request_binds_lease_to_jeryu_runner_protocol_request() {
        let pipeline = pipeline(1);
        let schedule = deterministic_schedule(&pipeline).expect("schedule");
        let mut leases = LeaseBook::new("run-1", &pipeline, &schedule).expect("lease book");
        let leased = leases
            .acquire_request(&pipeline, "test", "worker-a", 100, 30)
            .expect("leased request");

        assert_eq!(leased.request.pipeline_id, pipeline.id);
        assert_eq!(leased.request.run_id, "run-1");
        assert_eq!(leased.request.lease_id, leased.lease.id);
        assert_eq!(leased.request.job_id, "test");
        assert_eq!(leased.request.steps[0].name, "test");
        assert_eq!(leased.request.cache_mounts[0].path, "target/");
        assert_eq!(leased.receipt.kind, LeaseEventKind::Acquired);
        assert_eq!(
            leased.receipt.request_hash.as_deref(),
            Some(leased.request.wire_hash().as_str())
        );
        assert_eq!(leased.receipt.digest(), leased.receipt.digest());
    }

    #[test]
    fn successful_runner_result_completes_lease_with_receipt() {
        let pipeline = pipeline(1);
        let schedule = deterministic_schedule(&pipeline).expect("schedule");
        let mut leases = LeaseBook::new("run-1", &pipeline, &schedule).expect("lease book");
        let leased = leases
            .acquire_request(&pipeline, "test", "worker-a", 100, 30)
            .expect("leased request");
        let result = result_for(&leased.lease, JobOutcome::Success);

        let receipt = leases.apply_result(&result, 120).expect("receipt");
        assert_eq!(receipt.kind, LeaseEventKind::Completed);
        assert_eq!(
            receipt.result_hash.as_deref(),
            Some(result.receipt_hash().as_str())
        );
        assert!(matches!(
            leases.state("test"),
            Some(JobLeaseState::Succeeded)
        ));
    }

    #[test]
    fn failed_runner_result_requeues_then_exhausts_retry_budget() {
        let pipeline = pipeline(2);
        let schedule = deterministic_schedule(&pipeline).expect("schedule");
        let mut leases = LeaseBook::new("run-1", &pipeline, &schedule).expect("lease book");
        let first = leases
            .acquire_request(&pipeline, "test", "worker-a", 100, 30)
            .expect("first lease");
        let first_result = result_for(&first.lease, JobOutcome::InfrastructureFailure);

        let requeue = leases.apply_result(&first_result, 120).expect("requeue");
        assert_eq!(requeue.kind, LeaseEventKind::Requeued);
        assert!(matches!(leases.state("test"), Some(JobLeaseState::Pending)));

        let second = leases
            .acquire_request(&pipeline, "test", "worker-a", 130, 30)
            .expect("second lease");
        let second_result = result_for(&second.lease, JobOutcome::Failed);
        let failed = leases.apply_result(&second_result, 150).expect("failed");
        assert_eq!(failed.kind, LeaseEventKind::Failed);
        assert!(matches!(
            leases.state("test"),
            Some(JobLeaseState::Failed { attempts: 2, .. })
        ));
    }

    #[test]
    fn runner_result_from_another_run_is_rejected() {
        let pipeline = pipeline(1);
        let schedule = deterministic_schedule(&pipeline).expect("schedule");
        let mut leases = LeaseBook::new("run-1", &pipeline, &schedule).expect("lease book");
        let leased = leases
            .acquire_request(&pipeline, "test", "worker-a", 100, 30)
            .expect("leased request");
        let mut result = result_for(&leased.lease, JobOutcome::Success);
        result.run_id = "run-2".to_string();

        assert!(matches!(
            leases.apply_result(&result, 120),
            Err(LeaseError::ResultMismatch(job)) if job == "test"
        ));
        assert!(matches!(
            leases.state("test"),
            Some(JobLeaseState::Leased(active)) if active.id == leased.lease.id
        ));
    }

    #[test]
    fn acquire_request_failure_does_not_leave_orphaned_lease() {
        let pipeline = pipeline(1);
        let schedule = deterministic_schedule(&pipeline).expect("schedule");
        let mut leases = LeaseBook::new("run-1", &pipeline, &schedule).expect("lease book");
        let mut missing_job_pipeline = pipeline.clone();
        missing_job_pipeline.jobs.clear();

        assert!(matches!(
            leases.acquire_request(&missing_job_pipeline, "test", "worker-a", 100, 30),
            Err(LeaseError::UnknownJob(job)) if job == "test"
        ));
        assert!(matches!(leases.state("test"), Some(JobLeaseState::Pending)));
    }

    fn result_for(lease: &super::JobLease, outcome: JobOutcome) -> JobResult {
        JobResult {
            run_id: "run-1".to_string(),
            lease_id: lease.id.clone(),
            job_id: lease.job_id.clone(),
            outcome,
            exit_code: Some(1),
            started_at_millis: 100,
            finished_at_millis: 110,
            artifact_digests: Vec::new(),
            cache_receipts: Vec::new(),
            log_digest: "sha256:log".to_string(),
        }
    }
}

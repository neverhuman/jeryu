//! Job lease and retry state for deterministic CI schedules.

use crate::Schedule;
use ci_ir::{deterministic_hash, Job, Pipeline};
use runner_protocol::{JobOutcome, JobRequest, JobResult};
use std::collections::BTreeMap;
use std::fmt;

/// A runnable job lease.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobLease {
    /// Stable lease identifier for this run, job, and attempt.
    pub id: String,
    /// Job id.
    pub job_id: String,
    /// Worker holding the lease.
    pub worker_id: String,
    /// One-based attempt number.
    pub attempt: u32,
    /// Lease acquisition time.
    pub acquired_at_epoch: u64,
    /// Lease expiry time.
    pub expires_at_epoch: u64,
}

/// Current scheduler state for one job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobLeaseState {
    /// Job has not completed and has no active lease.
    Pending,
    /// Job currently has an active or stale lease.
    Leased(JobLease),
    /// Job completed successfully.
    Succeeded,
    /// Job exhausted retry attempts.
    Failed { attempts: u32, reason: String },
}

/// Scheduler lease receipt event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LeaseEventKind {
    /// Lease was acquired and a runner request was produced.
    Acquired,
    /// Job completed successfully.
    Completed,
    /// Job failed but retry budget remains.
    Requeued,
    /// Job exhausted retry budget.
    Failed,
}

impl LeaseEventKind {
    /// Stable event label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Acquired => "acquired",
            Self::Completed => "completed",
            Self::Requeued => "requeued",
            Self::Failed => "failed",
        }
    }
}

/// Deterministic scheduler receipt for lease replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseReceipt {
    /// Receipt id.
    pub receipt_id: String,
    /// Event kind.
    pub kind: LeaseEventKind,
    /// Run id.
    pub run_id: String,
    /// Schedule hash.
    pub schedule_hash: String,
    /// Job id.
    pub job_id: String,
    /// Lease id.
    pub lease_id: String,
    /// Attempt number.
    pub attempt: u32,
    /// Worker id.
    pub worker_id: String,
    /// Event time.
    pub at_epoch: u64,
    /// Optional reason.
    pub reason: String,
    /// Runner request wire hash when available.
    pub request_hash: Option<String>,
    /// Runner result receipt hash when available.
    pub result_hash: Option<String>,
}

impl LeaseReceipt {
    /// Canonical receipt body.
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        push_field(&mut out, "kind", self.kind.as_str());
        push_field(&mut out, "run_id", &self.run_id);
        push_field(&mut out, "schedule_hash", &self.schedule_hash);
        push_field(&mut out, "job_id", &self.job_id);
        push_field(&mut out, "lease_id", &self.lease_id);
        push_field(&mut out, "attempt", self.attempt);
        push_field(&mut out, "worker_id", &self.worker_id);
        push_field(&mut out, "at_epoch", self.at_epoch);
        push_field(&mut out, "reason", &self.reason);
        push_field(
            &mut out,
            "request_hash",
            self.request_hash.as_deref().unwrap_or(""),
        );
        push_field(
            &mut out,
            "result_hash",
            self.result_hash.as_deref().unwrap_or(""),
        );
        out
    }

    /// Deterministic receipt digest.
    pub fn digest(&self) -> String {
        deterministic_hash(&self.canonical())
    }
}

/// Active lease with the protocol request and scheduler receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeasedJobRequest {
    /// Active lease.
    pub lease: JobLease,
    /// Runner protocol request.
    pub request: JobRequest,
    /// Scheduler receipt for replay.
    pub receipt: LeaseReceipt,
}

/// Lease operation error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LeaseError {
    /// Job id is not in the scheduled pipeline.
    UnknownJob(String),
    /// Job is already complete.
    AlreadySucceeded(String),
    /// Job has failed permanently.
    PermanentlyFailed(String),
    /// Another worker holds a non-expired lease.
    ActiveLease {
        /// Job id.
        job_id: String,
        /// Current lease holder.
        worker_id: String,
        /// Lease expiry time.
        expires_at_epoch: u64,
    },
    /// Lease id does not match the active lease.
    LeaseMismatch(String),
    /// Runner result does not match the active lease.
    ResultMismatch(String),
}

impl fmt::Display for LeaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownJob(job) => write!(f, "unknown job: {job}"),
            Self::AlreadySucceeded(job) => write!(f, "job already succeeded: {job}"),
            Self::PermanentlyFailed(job) => write!(f, "job permanently failed: {job}"),
            Self::ActiveLease {
                job_id,
                worker_id,
                expires_at_epoch,
            } => write!(
                f,
                "job {job_id} is leased by {worker_id} until {expires_at_epoch}"
            ),
            Self::LeaseMismatch(job) => write!(f, "lease does not match job: {job}"),
            Self::ResultMismatch(job) => write!(f, "runner result does not match job: {job}"),
        }
    }
}

impl std::error::Error for LeaseError {}

#[derive(Clone, Debug, Eq, PartialEq)]
struct JobLeaseRecord {
    attempt: u32,
    max_attempts: u32,
    state: JobLeaseState,
}

/// Lease book for one scheduled pipeline run.
#[derive(Clone, Debug)]
pub struct LeaseBook {
    run_id: String,
    schedule_hash: String,
    jobs: BTreeMap<String, JobLeaseRecord>,
}

impl LeaseBook {
    /// Creates a lease book for a validated schedule.
    pub fn new(
        run_id: impl Into<String>,
        pipeline: &Pipeline,
        schedule: &Schedule,
    ) -> Result<Self, LeaseError> {
        let mut jobs = BTreeMap::new();
        for job in &pipeline.jobs {
            jobs.insert(
                job.id.clone(),
                JobLeaseRecord {
                    attempt: 0,
                    max_attempts: job.retry_policy.max_attempts.max(1),
                    state: JobLeaseState::Pending,
                },
            );
        }
        for round in &schedule.rounds {
            for job_id in &round.jobs {
                if !jobs.contains_key(job_id) {
                    return Err(LeaseError::UnknownJob(job_id.clone()));
                }
            }
        }
        Ok(Self {
            run_id: run_id.into(),
            schedule_hash: schedule.schedule_hash.clone(),
            jobs,
        })
    }

    /// Acquires a job lease.
    ///
    /// Re-acquiring the same active job by the same worker is idempotent and
    /// returns the existing lease. A different worker can take over only after
    /// the previous lease has expired.
    pub fn acquire(
        &mut self,
        job_id: &str,
        worker_id: impl Into<String>,
        now_epoch: u64,
        ttl_seconds: u64,
    ) -> Result<JobLease, LeaseError> {
        let worker_id = worker_id.into();
        let run_id = self.run_id.clone();
        let schedule_hash = self.schedule_hash.clone();
        let record = self
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| LeaseError::UnknownJob(job_id.to_string()))?;
        match &record.state {
            JobLeaseState::Pending => {}
            JobLeaseState::Leased(lease) => {
                if lease.expires_at_epoch > now_epoch {
                    if lease.worker_id == worker_id {
                        return Ok(lease.clone());
                    }
                    return Err(LeaseError::ActiveLease {
                        job_id: job_id.to_string(),
                        worker_id: lease.worker_id.clone(),
                        expires_at_epoch: lease.expires_at_epoch,
                    });
                }
            }
            JobLeaseState::Succeeded => {
                return Err(LeaseError::AlreadySucceeded(job_id.to_string()));
            }
            JobLeaseState::Failed { .. } => {
                return Err(LeaseError::PermanentlyFailed(job_id.to_string()));
            }
        }

        if record.attempt == 0 {
            record.attempt = 1;
        }
        let lease = build_lease(
            &run_id,
            &schedule_hash,
            job_id,
            worker_id,
            record.attempt,
            now_epoch,
            ttl_seconds,
        );
        record.state = JobLeaseState::Leased(lease.clone());
        Ok(lease)
    }

    /// Acquires a lease and builds the runner protocol request in one
    /// idempotent scheduler action.
    pub fn acquire_request(
        &mut self,
        pipeline: &Pipeline,
        job_id: &str,
        worker_id: impl Into<String>,
        now_epoch: u64,
        ttl_seconds: u64,
    ) -> Result<LeasedJobRequest, LeaseError> {
        let lease = self.acquire(job_id, worker_id, now_epoch, ttl_seconds)?;
        let request = self.runner_request(pipeline, &lease)?;
        let receipt = self.lease_receipt(
            LeaseEventKind::Acquired,
            &lease,
            now_epoch,
            "runner request leased",
            Some(request.wire_hash()),
            None,
        );
        Ok(LeasedJobRequest {
            lease,
            request,
            receipt,
        })
    }

    /// Marks an active lease successful.
    pub fn complete(&mut self, lease: &JobLease) -> Result<(), LeaseError> {
        let record = self
            .jobs
            .get_mut(&lease.job_id)
            .ok_or_else(|| LeaseError::UnknownJob(lease.job_id.clone()))?;
        match &record.state {
            JobLeaseState::Leased(active) if active.id == lease.id => {
                record.state = JobLeaseState::Succeeded;
                Ok(())
            }
            _ => Err(LeaseError::LeaseMismatch(lease.job_id.clone())),
        }
    }

    /// Marks an active lease failed, requeueing when retry attempts remain.
    pub fn fail(&mut self, lease: &JobLease, reason: impl Into<String>) -> Result<(), LeaseError> {
        let reason = reason.into();
        let record = self
            .jobs
            .get_mut(&lease.job_id)
            .ok_or_else(|| LeaseError::UnknownJob(lease.job_id.clone()))?;
        match &record.state {
            JobLeaseState::Leased(active) if active.id == lease.id => {
                if record.attempt < record.max_attempts {
                    record.attempt += 1;
                    record.state = JobLeaseState::Pending;
                } else {
                    record.state = JobLeaseState::Failed {
                        attempts: record.attempt,
                        reason,
                    };
                }
                Ok(())
            }
            _ => Err(LeaseError::LeaseMismatch(lease.job_id.clone())),
        }
    }

    /// Applies a runner result to the active lease and emits a replay receipt.
    pub fn apply_result(
        &mut self,
        result: &JobResult,
        at_epoch: u64,
    ) -> Result<LeaseReceipt, LeaseError> {
        let lease = match self.state(&result.job_id) {
            Some(JobLeaseState::Leased(active))
                if active.id == result.lease_id && active.job_id == result.job_id =>
            {
                active.clone()
            }
            Some(_) => return Err(LeaseError::ResultMismatch(result.job_id.clone())),
            None => return Err(LeaseError::UnknownJob(result.job_id.clone())),
        };
        let result_hash = Some(result.receipt_hash());
        match result.outcome {
            JobOutcome::Success => {
                self.complete(&lease)?;
                Ok(self.lease_receipt(
                    LeaseEventKind::Completed,
                    &lease,
                    at_epoch,
                    "runner reported success",
                    None,
                    result_hash,
                ))
            }
            JobOutcome::Failed
            | JobOutcome::Cancelled
            | JobOutcome::TimedOut
            | JobOutcome::InfrastructureFailure => {
                let reason = format!("runner reported {}", result.outcome.as_str());
                self.fail(&lease, reason.clone())?;
                let kind = match self.state(&result.job_id) {
                    Some(JobLeaseState::Pending) => LeaseEventKind::Requeued,
                    Some(JobLeaseState::Failed { .. }) => LeaseEventKind::Failed,
                    _ => return Err(LeaseError::ResultMismatch(result.job_id.clone())),
                };
                Ok(self.lease_receipt(kind, &lease, at_epoch, reason, None, result_hash))
            }
        }
    }

    /// Returns the state for a job.
    pub fn state(&self, job_id: &str) -> Option<&JobLeaseState> {
        self.jobs.get(job_id).map(|record| &record.state)
    }

    /// Returns the one-based next/current attempt for a job.
    pub fn attempt(&self, job_id: &str) -> Option<u32> {
        self.jobs.get(job_id).map(|record| record.attempt.max(1))
    }

    fn runner_request(
        &self,
        pipeline: &Pipeline,
        lease: &JobLease,
    ) -> Result<JobRequest, LeaseError> {
        let job = pipeline
            .jobs
            .iter()
            .find(|job| job.id == lease.job_id)
            .ok_or_else(|| LeaseError::UnknownJob(lease.job_id.clone()))?;
        match self.state(&lease.job_id) {
            Some(JobLeaseState::Leased(active)) if active.id == lease.id => {}
            Some(_) => return Err(LeaseError::LeaseMismatch(lease.job_id.clone())),
            None => return Err(LeaseError::UnknownJob(lease.job_id.clone())),
        }
        Ok(runner_request_from_job(
            &pipeline.id,
            &self.run_id,
            lease,
            job,
        ))
    }

    fn lease_receipt(
        &self,
        kind: LeaseEventKind,
        lease: &JobLease,
        at_epoch: u64,
        reason: impl Into<String>,
        request_hash: Option<String>,
        result_hash: Option<String>,
    ) -> LeaseReceipt {
        let seed = format!(
            "lease-receipt|{}|{}|{}|{}|{}|{}|{}",
            self.run_id,
            self.schedule_hash,
            lease.job_id,
            lease.id,
            lease.attempt,
            kind.as_str(),
            at_epoch
        );
        LeaseReceipt {
            receipt_id: deterministic_hash(&seed),
            kind,
            run_id: self.run_id.clone(),
            schedule_hash: self.schedule_hash.clone(),
            job_id: lease.job_id.clone(),
            lease_id: lease.id.clone(),
            attempt: lease.attempt,
            worker_id: lease.worker_id.clone(),
            at_epoch,
            reason: reason.into(),
            request_hash,
            result_hash,
        }
    }
}

fn runner_request_from_job(
    pipeline_id: &str,
    run_id: &str,
    lease: &JobLease,
    job: &Job,
) -> JobRequest {
    let mut request = JobRequest::new(
        pipeline_id,
        run_id,
        &lease.id,
        &job.id,
        job.runner_class.clone(),
    );
    request.steps = job.steps.clone();
    request.cache_mounts = job.cache_mounts.clone();
    request.artifact_paths = job.artifact_paths.clone();
    request.timeout_seconds = job.timeout_seconds;
    request
}

fn build_lease(
    run_id: &str,
    schedule_hash: &str,
    job_id: &str,
    worker_id: String,
    attempt: u32,
    now_epoch: u64,
    ttl_seconds: u64,
) -> JobLease {
    let id = deterministic_hash(&format!(
        "lease|{run_id}|{schedule_hash}|{job_id}|{attempt}|{worker_id}|{now_epoch}"
    ));
    JobLease {
        id,
        job_id: job_id.to_string(),
        worker_id,
        attempt,
        acquired_at_epoch: now_epoch,
        expires_at_epoch: now_epoch.saturating_add(ttl_seconds),
    }
}

fn push_field<K: fmt::Display, V: fmt::Display>(out: &mut String, key: K, value: V) {
    out.push_str(&key.to_string());
    out.push('=');
    out.push_str(&value.to_string().replace('\n', "\\n"));
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::{JobLeaseState, LeaseBook, LeaseError, LeaseEventKind};
    use crate::deterministic_schedule;
    use ci_ir::{
        CacheMode, CacheMount, Job, Pipeline, PipelineSource, RetryPolicy, RunnerClass, Step,
        TrustTier,
    };
    use runner_protocol::{JobOutcome, JobResult};

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
    fn acquire_request_binds_lease_to_runner_protocol_request() {
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

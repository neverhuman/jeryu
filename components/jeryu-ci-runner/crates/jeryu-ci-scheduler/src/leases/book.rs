//! The [`LeaseBook`]: the stateful lease ledger for one scheduled pipeline run,
//! plus the helpers that mint leases and build runner requests.

use std::collections::BTreeMap;

use jeryu_ci_ir::{Job, Pipeline, deterministic_hash};
use jeryu_runner_protocol::{JobOutcome, JobRequest, JobResult};

use crate::Schedule;

use super::error::LeaseError;
use super::types::{JobLease, JobLeaseState, LeaseEventKind, LeaseReceipt, LeasedJobRequest};

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
    /// Re-acquiring the same active job by the same worker and epoch is
    /// idempotent. Expired leases consume an attempt before any takeover.
    pub fn acquire(
        &mut self,
        job_id: &str,
        worker_id: impl Into<String>,
        now_epoch: u64,
        ttl_seconds: u64,
    ) -> Result<JobLease, LeaseError> {
        self.acquire_with_epoch(job_id, worker_id, 0, now_epoch, ttl_seconds)
    }

    /// Acquires a job lease for a specific fenced runner epoch.
    pub fn acquire_with_epoch(
        &mut self,
        job_id: &str,
        worker_id: impl Into<String>,
        node_epoch: u64,
        now_epoch: u64,
        ttl_seconds: u64,
    ) -> Result<JobLease, LeaseError> {
        if ttl_seconds == 0 || now_epoch.checked_add(ttl_seconds).is_none() {
            return Err(LeaseError::InvalidLeaseTime(job_id.to_string()));
        }
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
                if now_epoch < lease.acquired_at_epoch {
                    return Err(LeaseError::InvalidLeaseTime(job_id.to_string()));
                }
                if lease.expires_at_epoch > now_epoch {
                    if lease.worker_id == worker_id {
                        if lease.node_epoch != node_epoch {
                            return Err(LeaseError::FencedOut {
                                job_id: job_id.to_string(),
                                worker_id,
                                node_epoch,
                                active_node_epoch: lease.node_epoch,
                            });
                        }
                        return Ok(lease.clone());
                    }
                    return Err(LeaseError::ActiveLease {
                        job_id: job_id.to_string(),
                        worker_id: lease.worker_id.clone(),
                        expires_at_epoch: lease.expires_at_epoch,
                    });
                }
                retry_or_fail(record, "lease expired".to_string());
                if matches!(record.state, JobLeaseState::Failed { .. }) {
                    return Err(LeaseError::PermanentlyFailed(job_id.to_string()));
                }
            }
            JobLeaseState::Succeeded => {
                return Err(LeaseError::AlreadySucceeded(job_id.to_string()));
            }
            JobLeaseState::Failed { .. } => {
                return Err(LeaseError::PermanentlyFailed(job_id.to_string()));
            }
            JobLeaseState::Cancelled { .. } => {
                return Err(LeaseError::AlreadyCancelled(job_id.to_string()));
            }
        }

        if record.attempt == 0 {
            record.attempt = 1;
        }
        let lease = build_lease(LeaseBuild {
            run_id: &run_id,
            schedule_hash: &schedule_hash,
            job_id,
            worker_id,
            node_epoch,
            attempt: record.attempt,
            now_epoch,
            ttl_seconds,
        });
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
        self.acquire_request_with_epoch(pipeline, job_id, worker_id, 0, now_epoch, ttl_seconds)
    }

    /// Acquires a lease for a fenced runner epoch and builds its protocol
    /// request in one idempotent scheduler action.
    pub fn acquire_request_with_epoch(
        &mut self,
        pipeline: &Pipeline,
        job_id: &str,
        worker_id: impl Into<String>,
        node_epoch: u64,
        now_epoch: u64,
        ttl_seconds: u64,
    ) -> Result<LeasedJobRequest, LeaseError> {
        if !pipeline.jobs.iter().any(|job| job.id == job_id) {
            return Err(LeaseError::UnknownJob(job_id.to_string()));
        }
        let lease =
            self.acquire_with_epoch(job_id, worker_id, node_epoch, now_epoch, ttl_seconds)?;
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

    /// Marks an active lease successful at the scheduler's current time.
    pub fn complete(&mut self, lease: &JobLease, at_epoch: u64) -> Result<(), LeaseError> {
        self.active_record(lease, at_epoch)?.state = JobLeaseState::Succeeded;
        Ok(())
    }

    /// Marks an active lease failed, requeueing when retry attempts remain.
    pub fn fail(
        &mut self,
        lease: &JobLease,
        reason: impl Into<String>,
        at_epoch: u64,
    ) -> Result<(), LeaseError> {
        retry_or_fail(self.active_record(lease, at_epoch)?, reason.into());
        Ok(())
    }

    /// Cancels an active lease without scheduling another attempt.
    pub fn cancel(
        &mut self,
        lease: &JobLease,
        reason: impl Into<String>,
        at_epoch: u64,
    ) -> Result<(), LeaseError> {
        let record = self.active_record(lease, at_epoch)?;
        record.state = JobLeaseState::Cancelled {
            attempts: record.attempt,
            reason: reason.into(),
        };
        Ok(())
    }

    /// Sweeps expired leases once, returning their retry or terminal receipts.
    /// The transport must persist these transitions together with its queue.
    pub fn expire(&mut self, at_epoch: u64) -> Vec<LeaseReceipt> {
        let expired: Vec<_> = self
            .jobs
            .values()
            .filter_map(|record| match &record.state {
                JobLeaseState::Leased(lease) if lease.expires_at_epoch <= at_epoch => {
                    Some(lease.clone())
                }
                _ => None,
            })
            .collect();
        expired
            .into_iter()
            .map(|lease| {
                let record = self.jobs.get_mut(&lease.job_id).expect("known expired job");
                let kind = retry_or_fail(record, "lease expired".to_string());
                self.lease_receipt(kind, &lease, at_epoch, "lease expired", None, None)
            })
            .collect()
    }

    fn active_record(
        &mut self,
        lease: &JobLease,
        at_epoch: u64,
    ) -> Result<&mut JobLeaseRecord, LeaseError> {
        let record = self
            .jobs
            .get_mut(&lease.job_id)
            .ok_or_else(|| LeaseError::UnknownJob(lease.job_id.clone()))?;
        if !matches!(&record.state, JobLeaseState::Leased(active) if active == lease) {
            return Err(LeaseError::LeaseMismatch(lease.job_id.clone()));
        }
        if at_epoch < lease.acquired_at_epoch {
            return Err(LeaseError::InvalidLeaseTime(lease.job_id.clone()));
        }
        if at_epoch >= lease.expires_at_epoch {
            return Err(LeaseError::LeaseExpired(lease.job_id.clone()));
        }
        Ok(record)
    }

    /// Applies a runner result to the active lease and emits a replay receipt.
    /// `at_epoch` must come from the scheduler clock, not the runner's payload.
    /// The lease id binds the attempt; an expired attempt cannot report a result.
    pub fn apply_result(
        &mut self,
        result: &JobResult,
        at_epoch: u64,
    ) -> Result<LeaseReceipt, LeaseError> {
        if result.run_id != self.run_id {
            return Err(LeaseError::ResultMismatch(result.job_id.clone()));
        }
        let lease = match self.state(&result.job_id) {
            Some(JobLeaseState::Leased(active))
                if active.id == result.lease_id && active.job_id == result.job_id =>
            {
                if active.worker_id != result.runner_id || active.node_epoch != result.runner_epoch
                {
                    return Err(LeaseError::FencedOut {
                        job_id: result.job_id.clone(),
                        worker_id: result.runner_id.clone(),
                        node_epoch: result.runner_epoch,
                        active_node_epoch: active.node_epoch,
                    });
                }
                active.clone()
            }
            Some(_) => return Err(LeaseError::ResultMismatch(result.job_id.clone())),
            None => return Err(LeaseError::UnknownJob(result.job_id.clone())),
        };
        let result_hash = Some(result.receipt_hash());
        match result.outcome {
            JobOutcome::Success => {
                self.complete(&lease, at_epoch)?;
                Ok(self.lease_receipt(
                    LeaseEventKind::Completed,
                    &lease,
                    at_epoch,
                    "runner reported success",
                    None,
                    result_hash,
                ))
            }
            JobOutcome::Cancelled => {
                self.cancel(&lease, "runner reported cancelled", at_epoch)?;
                Ok(self.lease_receipt(
                    LeaseEventKind::Cancelled,
                    &lease,
                    at_epoch,
                    "runner reported cancelled",
                    None,
                    result_hash,
                ))
            }
            JobOutcome::Failed | JobOutcome::TimedOut | JobOutcome::InfrastructureFailure => {
                let reason = format!("runner reported {}", result.outcome.as_str());
                self.fail(&lease, reason.clone(), at_epoch)?;
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
            "lease-receipt|{}|{}|{}|{}|{}|{}|{}|{}",
            self.run_id,
            self.schedule_hash,
            lease.job_id,
            lease.id,
            lease.attempt,
            lease.node_epoch,
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
            node_epoch: lease.node_epoch,
            at_epoch,
            reason: reason.into(),
            request_hash,
            result_hash,
        }
    }
}

fn retry_or_fail(record: &mut JobLeaseRecord, reason: String) -> LeaseEventKind {
    if record.attempt < record.max_attempts {
        record.attempt += 1;
        record.state = JobLeaseState::Pending;
        LeaseEventKind::Requeued
    } else {
        record.state = JobLeaseState::Failed {
            attempts: record.attempt,
            reason,
        };
        LeaseEventKind::Failed
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
    request.assign_runner(&lease.worker_id, lease.node_epoch);
    request.steps = job.steps.clone();
    request.cache_mounts = job.cache_mounts.clone();
    request.artifact_paths = job.artifact_paths.clone();
    request.timeout_seconds = job.timeout_seconds;
    request
}

struct LeaseBuild<'a> {
    run_id: &'a str,
    schedule_hash: &'a str,
    job_id: &'a str,
    worker_id: String,
    node_epoch: u64,
    attempt: u32,
    now_epoch: u64,
    ttl_seconds: u64,
}

fn build_lease(input: LeaseBuild<'_>) -> JobLease {
    let id = deterministic_hash(&format!(
        "lease|{}|{}|{}|{}|{}|{}|{}",
        input.run_id,
        input.schedule_hash,
        input.job_id,
        input.attempt,
        input.worker_id,
        input.node_epoch,
        input.now_epoch
    ));
    JobLease {
        id,
        job_id: input.job_id.to_string(),
        worker_id: input.worker_id,
        node_epoch: input.node_epoch,
        attempt: input.attempt,
        acquired_at_epoch: input.now_epoch,
        expires_at_epoch: input.now_epoch.saturating_add(input.ttl_seconds),
    }
}

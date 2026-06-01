//! Lease value types: leases, per-job state, replay receipts, and the
//! lease-bound runner request.

use std::fmt;

use jeryu_ci_ir::deterministic_hash;
use jeryu_runner_protocol::JobRequest;

/// A runnable job lease.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobLease {
    /// Stable lease identifier for this run, job, and attempt.
    pub id: String,
    /// Job id.
    pub job_id: String,
    /// Worker holding the lease.
    pub worker_id: String,
    /// Fencing token for the worker/node that holds the lease.
    pub node_epoch: u64,
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
    /// Job currently has an active or expired lease.
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
    /// Fencing token for the worker/node at lease acquisition.
    pub node_epoch: u64,
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
        push_field(&mut out, "node_epoch", self.node_epoch);
        push_field(&mut out, "at_epoch", self.at_epoch);
        push_field(&mut out, "reason", &self.reason);
        // `request_hash`/`result_hash` are absent for events that carry no wire
        // payload (e.g. a completion receipt has no request hash). The empty
        // string is the canonical encoding of "no payload", not a hidden error.
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

pub(super) fn push_field<K: fmt::Display, V: fmt::Display>(out: &mut String, key: K, value: V) {
    out.push_str(&key.to_string());
    out.push('=');
    out.push_str(&value.to_string().replace('\n', "\\n"));
    out.push('\n');
}

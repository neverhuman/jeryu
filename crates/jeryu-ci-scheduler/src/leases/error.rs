//! Lease operation errors.

use std::fmt;

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

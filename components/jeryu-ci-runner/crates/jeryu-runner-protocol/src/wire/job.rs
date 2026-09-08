use super::body::{WireArtifactPath, WireCacheMount, WireStep};
use super::context::WireRunnerClass;
use super::validation::{
    CanonicalSha256, MAX_ARTIFACTS, MAX_CACHE_MOUNTS, MAX_ENV_ENTRIES, MAX_JOB_TIMEOUT_SECONDS,
    ValidateWire, WireError, WireErrorCode, validate_digest, validate_env, validate_id,
    validate_sorted_unique, validate_timestamp, validate_unique,
};
use super::{MAX_RESULT_ITEMS, MAX_STEPS};
use crate::{JobOutcome, JobRequest, JobResult};
use jeryu_ci_ir::{ArtifactPath, CacheMount, Step};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Closed, bounded representation of the existing pure [`JobRequest`].
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireJobRequest {
    pub request_id: String,
    pub pipeline_id: String,
    pub run_id: String,
    pub lease_id: String,
    pub job_id: String,
    pub runner_id: String,
    pub runner_epoch: u64,
    pub runner_class: WireRunnerClass,
    pub steps: Vec<WireStep>,
    pub cache_mounts: Vec<WireCacheMount>,
    pub artifact_paths: Vec<WireArtifactPath>,
    pub env: BTreeMap<String, String>,
    pub timeout_seconds: u64,
}

impl WireJobRequest {
    pub fn validate(&self) -> Result<(), WireError> {
        self.validate_wire()
    }

    /// Canonical SHA-256 identity of every execution-affecting job field.
    #[must_use]
    pub fn execution_digest(&self) -> String {
        let mut digest = CanonicalSha256::new("jeryu.runner.job.v1");
        for (name, value) in [
            ("request_id", self.request_id.as_str()),
            ("pipeline_id", self.pipeline_id.as_str()),
            ("run_id", self.run_id.as_str()),
            ("lease_id", self.lease_id.as_str()),
            ("job_id", self.job_id.as_str()),
            ("runner_id", self.runner_id.as_str()),
            ("runner_class", self.runner_class.as_str()),
        ] {
            digest.field(name, value);
        }
        digest.field("runner_epoch", &self.runner_epoch.to_string());
        digest.field("steps.count", &self.steps.len().to_string());
        for step in &self.steps {
            digest.field("step.id", &step.id);
            digest.field("step.name", &step.name);
            digest_optional(&mut digest, "step.command", step.command.as_deref());
            digest_optional(&mut digest, "step.uses", step.uses.as_deref());
            digest.field("step.env.count", &step.env.len().to_string());
            for (name, value) in &step.env {
                digest.field("step.env.name", name);
                digest.field("step.env.value", value);
            }
            digest_optional(
                &mut digest,
                "step.working_directory",
                step.working_directory.as_deref(),
            );
        }
        digest.field("cache_mounts.count", &self.cache_mounts.len().to_string());
        for cache in &self.cache_mounts {
            digest.field("cache.name", &cache.name);
            digest.field("cache.path", &cache.path);
            digest.field("cache.mode", cache.mode.as_str());
            digest.field("cache.fingerprint", &cache.fingerprint);
        }
        digest.field(
            "artifact_paths.count",
            &self.artifact_paths.len().to_string(),
        );
        for artifact in &self.artifact_paths {
            digest.field("artifact.name", &artifact.name);
            digest.field("artifact.paths.count", &artifact.paths.len().to_string());
            for path in &artifact.paths {
                digest.field("artifact.path", path);
            }
            digest.field("artifact.when", artifact.when.as_str());
            digest.field(
                "artifact.retention_days",
                &artifact.retention_days.to_string(),
            );
        }
        digest.field("env.count", &self.env.len().to_string());
        for (name, value) in &self.env {
            digest.field("env.name", name);
            digest.field("env.value", value);
        }
        digest.field("timeout_seconds", &self.timeout_seconds.to_string());
        digest.finish()
    }
}

fn digest_optional(digest: &mut CanonicalSha256, name: &str, value: Option<&str>) {
    digest.field(
        &format!("{name}.present"),
        if value.is_some() { "true" } else { "false" },
    );
    if let Some(value) = value {
        digest.field(name, value);
    }
}

impl ValidateWire for WireJobRequest {
    fn validate_wire(&self) -> Result<(), WireError> {
        for (field, value) in [
            ("request_id", self.request_id.as_str()),
            ("pipeline_id", self.pipeline_id.as_str()),
            ("run_id", self.run_id.as_str()),
            ("lease_id", self.lease_id.as_str()),
            ("job_id", self.job_id.as_str()),
            ("runner_id", self.runner_id.as_str()),
        ] {
            validate_id(field, value)?;
        }
        if self.runner_epoch == 0 {
            return Err(WireError::new(WireErrorCode::InvalidField, "runner_epoch"));
        }
        if self.steps.is_empty() || self.steps.len() > MAX_STEPS {
            return Err(WireError::new(WireErrorCode::InvalidCollection, "steps"));
        }
        for step in &self.steps {
            step.validate_wire()?;
        }
        validate_unique("steps", self.steps.iter().map(|step| step.id.as_str()))?;
        if self.cache_mounts.len() > MAX_CACHE_MOUNTS {
            return Err(WireError::new(
                WireErrorCode::InvalidCollection,
                "cache_mounts",
            ));
        }
        for cache in &self.cache_mounts {
            cache.validate_wire()?;
        }
        validate_unique(
            "cache_mounts",
            self.cache_mounts.iter().map(|cache| cache.name.as_str()),
        )?;
        if self.artifact_paths.len() > MAX_ARTIFACTS {
            return Err(WireError::new(
                WireErrorCode::InvalidCollection,
                "artifact_paths",
            ));
        }
        for artifact in &self.artifact_paths {
            artifact.validate_wire()?;
        }
        validate_unique(
            "artifact_paths",
            self.artifact_paths
                .iter()
                .map(|artifact| artifact.name.as_str()),
        )?;
        validate_env("env", &self.env, MAX_ENV_ENTRIES)?;
        if !(1..=MAX_JOB_TIMEOUT_SECONDS).contains(&self.timeout_seconds) {
            return Err(WireError::new(
                WireErrorCode::InvalidField,
                "timeout_seconds",
            ));
        }
        Ok(())
    }
}

impl TryFrom<&JobRequest> for WireJobRequest {
    type Error = WireError;

    fn try_from(value: &JobRequest) -> Result<Self, Self::Error> {
        let wire = Self {
            request_id: value.request_id.clone(),
            pipeline_id: value.pipeline_id.clone(),
            run_id: value.run_id.clone(),
            lease_id: value.lease_id.clone(),
            job_id: value.job_id.clone(),
            runner_id: value.runner_id.clone(),
            runner_epoch: value.runner_epoch,
            runner_class: WireRunnerClass::from_runner_class(&value.runner_class)?,
            steps: value.steps.iter().map(WireStep::from).collect(),
            cache_mounts: value
                .cache_mounts
                .iter()
                .map(WireCacheMount::from)
                .collect(),
            artifact_paths: value
                .artifact_paths
                .iter()
                .map(WireArtifactPath::from)
                .collect(),
            env: value.env.clone(),
            timeout_seconds: value.timeout_seconds,
        };
        wire.validate_wire()?;
        Ok(wire)
    }
}

impl TryFrom<WireJobRequest> for JobRequest {
    type Error = WireError;

    fn try_from(value: WireJobRequest) -> Result<Self, Self::Error> {
        value.validate_wire()?;
        Ok(Self {
            request_id: value.request_id,
            pipeline_id: value.pipeline_id,
            run_id: value.run_id,
            lease_id: value.lease_id,
            job_id: value.job_id,
            runner_id: value.runner_id,
            runner_epoch: value.runner_epoch,
            runner_class: value.runner_class.to_runner_class(),
            steps: value.steps.into_iter().map(Step::from).collect(),
            cache_mounts: value
                .cache_mounts
                .into_iter()
                .map(CacheMount::from)
                .collect(),
            artifact_paths: value
                .artifact_paths
                .into_iter()
                .map(ArtifactPath::from)
                .collect(),
            env: value.env,
            timeout_seconds: value.timeout_seconds,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WireJobOutcome {
    Success,
    Failed,
    Cancelled,
    TimedOut,
    InfrastructureFailure,
}

impl From<&JobOutcome> for WireJobOutcome {
    fn from(value: &JobOutcome) -> Self {
        match value {
            JobOutcome::Success => Self::Success,
            JobOutcome::Failed => Self::Failed,
            JobOutcome::Cancelled => Self::Cancelled,
            JobOutcome::TimedOut => Self::TimedOut,
            JobOutcome::InfrastructureFailure => Self::InfrastructureFailure,
        }
    }
}

impl From<WireJobOutcome> for JobOutcome {
    fn from(value: WireJobOutcome) -> Self {
        match value {
            WireJobOutcome::Success => Self::Success,
            WireJobOutcome::Failed => Self::Failed,
            WireJobOutcome::Cancelled => Self::Cancelled,
            WireJobOutcome::TimedOut => Self::TimedOut,
            WireJobOutcome::InfrastructureFailure => Self::InfrastructureFailure,
        }
    }
}

impl WireJobOutcome {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed-out",
            Self::InfrastructureFailure => "infrastructure-failure",
        }
    }
}

/// Closed, bounded representation of the existing pure [`JobResult`].
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireJobResult {
    pub runner_id: String,
    pub runner_epoch: u64,
    pub run_id: String,
    pub lease_id: String,
    pub job_id: String,
    pub outcome: WireJobOutcome,
    pub exit_code: Option<i32>,
    pub started_at_unix_millis: u64,
    pub finished_at_unix_millis: u64,
    pub artifact_digests: Vec<String>,
    pub cache_receipts: Vec<String>,
    pub log_digest: String,
}

impl WireJobResult {
    pub fn validate(&self) -> Result<(), WireError> {
        self.validate_wire()
    }
}

impl ValidateWire for WireJobResult {
    fn validate_wire(&self) -> Result<(), WireError> {
        for (field, value) in [
            ("runner_id", self.runner_id.as_str()),
            ("run_id", self.run_id.as_str()),
            ("lease_id", self.lease_id.as_str()),
            ("job_id", self.job_id.as_str()),
        ] {
            validate_id(field, value)?;
        }
        if self.runner_epoch == 0 {
            return Err(WireError::new(WireErrorCode::InvalidField, "runner_epoch"));
        }
        validate_timestamp("started_at_unix_millis", self.started_at_unix_millis)?;
        validate_timestamp("finished_at_unix_millis", self.finished_at_unix_millis)?;
        if self.finished_at_unix_millis < self.started_at_unix_millis
            || self
                .finished_at_unix_millis
                .saturating_sub(self.started_at_unix_millis)
                > MAX_JOB_TIMEOUT_SECONDS * 1_000
        {
            return Err(WireError::new(
                WireErrorCode::InvalidField,
                "finished_at_unix_millis",
            ));
        }
        if self.artifact_digests.len() > MAX_RESULT_ITEMS
            || self.cache_receipts.len() > MAX_RESULT_ITEMS
        {
            return Err(WireError::new(
                WireErrorCode::InvalidCollection,
                "result_items",
            ));
        }
        for digest in &self.artifact_digests {
            validate_digest("artifact_digests", digest)?;
        }
        for receipt in &self.cache_receipts {
            validate_id("cache_receipts", receipt)?;
        }
        validate_sorted_unique("artifact_digests", &self.artifact_digests)?;
        validate_sorted_unique("cache_receipts", &self.cache_receipts)?;
        validate_digest("log_digest", &self.log_digest)?;
        if self.outcome == WireJobOutcome::Success && self.exit_code != Some(0) {
            return Err(WireError::new(WireErrorCode::InvalidField, "exit_code"));
        }
        Ok(())
    }
}

impl TryFrom<&JobResult> for WireJobResult {
    type Error = WireError;

    fn try_from(value: &JobResult) -> Result<Self, Self::Error> {
        let wire = Self {
            runner_id: value.runner_id.clone(),
            runner_epoch: value.runner_epoch,
            run_id: value.run_id.clone(),
            lease_id: value.lease_id.clone(),
            job_id: value.job_id.clone(),
            outcome: (&value.outcome).into(),
            exit_code: value.exit_code,
            started_at_unix_millis: value.started_at_millis,
            finished_at_unix_millis: value.finished_at_millis,
            artifact_digests: value.artifact_digests.clone(),
            cache_receipts: value.cache_receipts.clone(),
            log_digest: value.log_digest.clone(),
        };
        wire.validate_wire()?;
        Ok(wire)
    }
}

impl TryFrom<WireJobResult> for JobResult {
    type Error = WireError;

    fn try_from(value: WireJobResult) -> Result<Self, Self::Error> {
        value.validate_wire()?;
        Ok(Self {
            runner_id: value.runner_id,
            runner_epoch: value.runner_epoch,
            run_id: value.run_id,
            lease_id: value.lease_id,
            job_id: value.job_id,
            outcome: value.outcome.into(),
            exit_code: value.exit_code,
            started_at_millis: value.started_at_unix_millis,
            finished_at_millis: value.finished_at_unix_millis,
            artifact_digests: value.artifact_digests,
            cache_receipts: value.cache_receipts,
            log_digest: value.log_digest,
        })
    }
}

impl_redacted_debug!(WireJobRequest, WireJobResult);

use super::job::{WireJobRequest, WireJobResult};
use super::validation::{
    ValidateWire, WireError, WireErrorCode, validate_check, validate_digest, validate_git_sha,
    validate_head_sha, validate_id, validate_label, validate_repository,
};
use jeryu_ci_ir::RunnerClass;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Canonical JSON representation of a [`RunnerClass`].
///
/// Built-ins use exact kebab case. A custom class is encoded only as
/// `custom:<lowercase-token>`, preventing aliases from changing identity.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct WireRunnerClass(String);

impl WireRunnerClass {
    pub fn from_runner_class(class: &RunnerClass) -> Result<Self, WireError> {
        let canonical = match class {
            RunnerClass::Custom(value) => {
                validate_label("runner_class", value)?;
                format!("custom:{value}")
            }
            _ => class.as_str().to_string(),
        };
        Ok(Self(canonical))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn to_runner_class(&self) -> RunnerClass {
        match self.0.as_str() {
            "native-rust-hot" => RunnerClass::NativeRustHot,
            "native-rust-clean" => RunnerClass::NativeRustClean,
            "crategraph-delta" => RunnerClass::CrategraphDelta,
            "nextest-capsule" => RunnerClass::NextestCapsule,
            "agent-guard" => RunnerClass::AgentGuard,
            "merge-spec" => RunnerClass::MergeSpec,
            "release-hermetic" => RunnerClass::ReleaseHermetic,
            "microvm-rust" => RunnerClass::MicrovmRust,
            "oci-docker" => RunnerClass::OciDocker,
            "k8s-oci" => RunnerClass::K8sOci,
            custom => RunnerClass::Custom(
                custom
                    .strip_prefix("custom:")
                    .expect("wire runner class invariant")
                    .to_string(),
            ),
        }
    }

    fn parse(value: &str) -> Result<Self, WireError> {
        match value {
            "native-rust-hot" | "native-rust-clean" | "crategraph-delta" | "nextest-capsule"
            | "agent-guard" | "merge-spec" | "release-hermetic" | "microvm-rust" | "oci-docker"
            | "k8s-oci" => Ok(Self(value.to_string())),
            custom if custom.starts_with("custom:") => {
                validate_label("runner_class", &custom["custom:".len()..])?;
                Ok(Self(custom.to_string()))
            }
            _ => Err(WireError::new(WireErrorCode::InvalidField, "runner_class")),
        }
    }
}

impl fmt::Debug for WireRunnerClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WireRunnerClass(<redacted>)")
    }
}

impl Serialize for WireRunnerClass {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for WireRunnerClass {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(|_| D::Error::custom("invalid canonical runner class"))
    }
}

/// Runner identity and current fencing epoch.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerContext {
    pub runner_id: String,
    pub runner_epoch: u64,
}

impl ValidateWire for RunnerContext {
    fn validate_wire(&self) -> Result<(), WireError> {
        validate_id("runner_id", &self.runner_id)?;
        if self.runner_epoch == 0 {
            return Err(WireError::new(WireErrorCode::InvalidField, "runner_epoch"));
        }
        Ok(())
    }
}

/// Complete immutable context bound to a leased job and its result.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionContext {
    pub runner_id: String,
    pub runner_epoch: u64,
    pub run_id: String,
    pub lease_id: String,
    pub job_id: String,
    pub repository: String,
    pub head_sha: String,
    pub required_check: String,
    pub job_digest: String,
    pub protected_policy_sha: String,
    pub toolchain_digest: String,
    pub runner_class_policy_id: String,
    pub runner_class_policy_digest: String,
    pub image_digest: String,
    pub rootfs_digest: String,
}

impl ExecutionContext {
    #[must_use]
    pub fn runner(&self) -> RunnerContext {
        RunnerContext {
            runner_id: self.runner_id.clone(),
            runner_epoch: self.runner_epoch,
        }
    }

    pub(super) fn validate_job(&self, job: &WireJobRequest) -> Result<(), WireError> {
        if self.runner_id != job.runner_id
            || self.runner_epoch != job.runner_epoch
            || self.run_id != job.run_id
            || self.lease_id != job.lease_id
            || self.job_id != job.job_id
            || self.job_digest != job.execution_digest()
        {
            return Err(WireError::new(
                WireErrorCode::ContextMismatch,
                "execution_context",
            ));
        }
        Ok(())
    }

    pub(super) fn validate_result(&self, result: &WireJobResult) -> Result<(), WireError> {
        if self.runner_id != result.runner_id
            || self.runner_epoch != result.runner_epoch
            || self.run_id != result.run_id
            || self.lease_id != result.lease_id
            || self.job_id != result.job_id
        {
            return Err(WireError::new(
                WireErrorCode::ContextMismatch,
                "execution_context",
            ));
        }
        Ok(())
    }
}

impl ValidateWire for ExecutionContext {
    fn validate_wire(&self) -> Result<(), WireError> {
        self.runner().validate_wire()?;
        validate_id("run_id", &self.run_id)?;
        validate_id("lease_id", &self.lease_id)?;
        validate_id("job_id", &self.job_id)?;
        validate_repository(&self.repository)?;
        validate_head_sha(&self.head_sha)?;
        validate_check(&self.required_check)?;
        validate_digest("job_digest", &self.job_digest)?;
        validate_git_sha("protected_policy_sha", &self.protected_policy_sha)?;
        validate_digest("toolchain_digest", &self.toolchain_digest)?;
        validate_id("runner_class_policy_id", &self.runner_class_policy_id)?;
        validate_digest(
            "runner_class_policy_digest",
            &self.runner_class_policy_digest,
        )?;
        validate_digest("image_digest", &self.image_digest)?;
        validate_digest("rootfs_digest", &self.rootfs_digest)
    }
}

impl_redacted_debug!(RunnerContext, ExecutionContext);

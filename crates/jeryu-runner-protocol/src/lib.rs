use jeryu_ci_ir::{ArtifactPath, CacheMount, RunnerClass, Step, deterministic_hash};
use std::collections::BTreeMap;
use std::fmt;

pub const PROTOCOL_VERSION: &str = "jeryu.runner.v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerHello {
    pub runner_id: String,
    pub protocol_version: String,
    pub supported_classes: Vec<RunnerClass>,
    pub labels: Vec<String>,
    pub capacity: u32,
}

impl RunnerHello {
    pub fn new(runner_id: impl Into<String>, supported_classes: Vec<RunnerClass>) -> Self {
        Self {
            runner_id: runner_id.into(),
            protocol_version: PROTOCOL_VERSION.to_string(),
            supported_classes,
            labels: Vec::new(),
            capacity: 1,
        }
    }

    pub fn supports(&self, class: &RunnerClass) -> bool {
        self.supported_classes
            .iter()
            .any(|candidate| candidate == class)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobRequest {
    pub request_id: String,
    pub pipeline_id: String,
    pub run_id: String,
    pub lease_id: String,
    pub job_id: String,
    pub runner_id: String,
    pub runner_epoch: u64,
    pub runner_class: RunnerClass,
    pub steps: Vec<Step>,
    pub cache_mounts: Vec<CacheMount>,
    pub artifact_paths: Vec<ArtifactPath>,
    pub env: BTreeMap<String, String>,
    pub timeout_seconds: u64,
}

impl JobRequest {
    pub fn new(
        pipeline_id: impl Into<String>,
        run_id: impl Into<String>,
        lease_id: impl Into<String>,
        job_id: impl Into<String>,
        runner_class: RunnerClass,
    ) -> Self {
        let pipeline_id = pipeline_id.into();
        let run_id = run_id.into();
        let lease_id = lease_id.into();
        let job_id = job_id.into();
        let request_id = deterministic_hash(&format!(
            "job-request|{pipeline_id}|{run_id}|{lease_id}|{job_id}"
        ));
        Self {
            request_id,
            pipeline_id,
            run_id,
            lease_id,
            job_id,
            runner_id: String::new(),
            runner_epoch: 0,
            runner_class,
            steps: Vec::new(),
            cache_mounts: Vec::new(),
            artifact_paths: Vec::new(),
            env: BTreeMap::new(),
            timeout_seconds: 3600,
        }
    }

    pub fn assign_runner(&mut self, runner_id: impl Into<String>, runner_epoch: u64) {
        self.runner_id = runner_id.into();
        self.runner_epoch = runner_epoch;
    }

    pub fn canonical(&self) -> String {
        let mut out = String::new();
        field(&mut out, "protocol", PROTOCOL_VERSION);
        field(&mut out, "request_id", &self.request_id);
        field(&mut out, "pipeline_id", &self.pipeline_id);
        field(&mut out, "run_id", &self.run_id);
        field(&mut out, "lease_id", &self.lease_id);
        field(&mut out, "job_id", &self.job_id);
        field(&mut out, "runner_id", &self.runner_id);
        field(&mut out, "runner_epoch", self.runner_epoch);
        field(&mut out, "runner_class", self.runner_class.as_str());
        field(&mut out, "timeout_seconds", self.timeout_seconds);
        for (key, value) in &self.env {
            field(&mut out, format!("env.{key}"), value);
        }
        for step in &self.steps {
            field(&mut out, "step.id", &step.id);
            field(&mut out, "step.name", &step.name);
            if let Some(command) = &step.command {
                field(&mut out, "step.run", command);
            }
            if let Some(uses) = &step.uses {
                field(&mut out, "step.uses", uses);
            }
        }
        for mount in &self.cache_mounts {
            field(
                &mut out,
                "cache",
                format!("{}|{}|{}", mount.name, mount.path, mount.fingerprint),
            );
        }
        for artifact in &self.artifact_paths {
            field(
                &mut out,
                "artifact",
                format!("{}|{}", artifact.name, artifact.paths.join(",")),
            );
        }
        out
    }

    pub fn wire_hash(&self) -> String {
        deterministic_hash(&self.canonical())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heartbeat {
    pub runner_id: String,
    pub runner_epoch: u64,
    pub run_id: String,
    pub lease_id: String,
    pub job_id: String,
    pub monotonic_millis: u64,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobOutcome {
    Success,
    Failed,
    Cancelled,
    TimedOut,
    InfrastructureFailure,
}

impl JobOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed-out",
            Self::InfrastructureFailure => "infrastructure-failure",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobResult {
    pub runner_id: String,
    pub runner_epoch: u64,
    pub run_id: String,
    pub lease_id: String,
    pub job_id: String,
    pub outcome: JobOutcome,
    pub exit_code: Option<i32>,
    pub started_at_millis: u64,
    pub finished_at_millis: u64,
    pub artifact_digests: Vec<String>,
    pub cache_receipts: Vec<String>,
    pub log_digest: String,
}

impl JobResult {
    pub fn duration_millis(&self) -> u64 {
        self.finished_at_millis
            .saturating_sub(self.started_at_millis)
    }

    pub fn receipt_hash(&self) -> String {
        deterministic_hash(&format!(
            "result|{}|{}|{}|{}|{}|{}|{:?}|{}|{}|{}|{}|{}",
            self.runner_id,
            self.runner_epoch,
            self.run_id,
            self.lease_id,
            self.job_id,
            self.outcome.as_str(),
            self.exit_code,
            self.started_at_millis,
            self.finished_at_millis,
            self.artifact_digests.join(","),
            self.cache_receipts.join(","),
            self.log_digest
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    VersionMismatch { expected: String, actual: String },
    UnsupportedRunnerClass(String),
    MissingField(String),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VersionMismatch { expected, actual } => {
                write!(f, "protocol mismatch: expected {expected}, got {actual}")
            }
            Self::UnsupportedRunnerClass(class) => write!(f, "unsupported runner class: {class}"),
            Self::MissingField(field_name) => write!(f, "missing protocol field: {field_name}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

pub fn validate_hello(
    hello: &RunnerHello,
    requested_class: &RunnerClass,
) -> Result<(), ProtocolError> {
    if hello.protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolError::VersionMismatch {
            expected: PROTOCOL_VERSION.to_string(),
            actual: hello.protocol_version.clone(),
        });
    }
    if !hello.supports(requested_class) {
        return Err(ProtocolError::UnsupportedRunnerClass(
            requested_class.as_str().to_string(),
        ));
    }
    Ok(())
}

fn field<K: fmt::Display, V: fmt::Display>(out: &mut String, key: K, value: V) {
    out.push_str(&key.to_string());
    out.push('=');
    out.push_str(&value.to_string().replace('\n', "\\n"));
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_ci_ir::Step;

    #[test]
    fn hello_rejects_unsupported_runner() {
        let hello = RunnerHello::new("runner-a", vec![RunnerClass::NativeRustClean]);
        let result = validate_hello(&hello, &RunnerClass::OciDocker);
        assert!(matches!(
            result,
            Err(ProtocolError::UnsupportedRunnerClass(_))
        ));
    }

    #[test]
    fn job_request_wire_hash_is_stable() {
        let mut request = JobRequest::new("p", "r", "l", "j", RunnerClass::NativeRustClean);
        request.steps.push(Step::run("s", "test", "cargo test"));
        request.assign_runner("runner-a", 2);
        let a = request.wire_hash();
        let b = request.wire_hash();
        assert_eq!(a, b);

        request.assign_runner("runner-a", 3);
        assert_ne!(a, request.wire_hash());
    }

    #[test]
    fn result_receipt_hash_includes_artifacts() {
        let mut result = JobResult {
            runner_id: "runner-a".to_string(),
            runner_epoch: 1,
            run_id: "r".to_string(),
            lease_id: "l".to_string(),
            job_id: "j".to_string(),
            outcome: JobOutcome::Success,
            exit_code: Some(0),
            started_at_millis: 10,
            finished_at_millis: 20,
            artifact_digests: vec!["sha256:a".to_string()],
            cache_receipts: Vec::new(),
            log_digest: "sha256:log".to_string(),
        };
        let a = result.receipt_hash();
        result.artifact_digests.push("sha256:b".to_string());
        let b = result.receipt_hash();
        assert_ne!(a, b);
    }

    #[test]
    fn result_receipt_hash_includes_log_digest() {
        let mut result = JobResult {
            runner_id: "runner-a".to_string(),
            runner_epoch: 1,
            run_id: "r".to_string(),
            lease_id: "l".to_string(),
            job_id: "j".to_string(),
            outcome: JobOutcome::Success,
            exit_code: Some(0),
            started_at_millis: 10,
            finished_at_millis: 20,
            artifact_digests: Vec::new(),
            cache_receipts: Vec::new(),
            log_digest: "sha256:log-a".to_string(),
        };
        let a = result.receipt_hash();
        result.log_digest = "sha256:log-b".to_string();
        let b = result.receipt_hash();
        assert_ne!(a, b);
    }
}

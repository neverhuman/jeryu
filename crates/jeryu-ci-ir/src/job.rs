//! Job-level IR: steps, dependencies, retry/cache/artifact attributes, and the
//! canonical serialisation of a single job.

use std::collections::BTreeMap;

use crate::enums::{ArtifactWhen, CacheMode, NetworkPolicy, RunnerClass, TokenScope};
use crate::hashing::{line, network_to_string, token_to_string};

pub type EnvMap = BTreeMap<String, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff_seconds: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            backoff_seconds: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheMount {
    pub name: String,
    pub path: String,
    pub mode: CacheMode,
    pub fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactPath {
    pub name: String,
    pub paths: Vec<String>,
    pub when: ArtifactWhen,
    pub retention_days: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub name: String,
    pub command: Option<String>,
    pub uses: Option<String>,
    pub env: EnvMap,
    pub working_directory: Option<String>,
}

impl Step {
    pub fn run(id: impl Into<String>, name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            command: Some(command.into()),
            uses: None,
            env: EnvMap::new(),
            working_directory: None,
        }
    }

    pub fn uses(id: impl Into<String>, name: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            command: None,
            uses: Some(action.into()),
            env: EnvMap::new(),
            working_directory: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub name: String,
    pub runner_class: RunnerClass,
    pub steps: Vec<Step>,
    pub inputs: BTreeMap<String, String>,
    pub outputs: BTreeMap<String, String>,
    pub cache_mounts: Vec<CacheMount>,
    pub artifact_paths: Vec<ArtifactPath>,
    pub network_policy: NetworkPolicy,
    pub token_scope: TokenScope,
    pub timeout_seconds: u64,
    pub retry_policy: RetryPolicy,
}

impl Job {
    pub fn new(id: impl Into<String>, name: impl Into<String>, runner_class: RunnerClass) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            runner_class,
            steps: Vec::new(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            cache_mounts: Vec::new(),
            artifact_paths: Vec::new(),
            network_policy: NetworkPolicy::Deny,
            token_scope: TokenScope::ReadRepo,
            timeout_seconds: 3600,
            retry_policy: RetryPolicy::default(),
        }
    }

    pub(crate) fn canonical_into(&self, out: &mut String) {
        line(out, "job.id", &self.id);
        line(out, "job.name", &self.name);
        line(out, "job.runner", self.runner_class.as_str());
        line(out, "job.timeout_seconds", self.timeout_seconds);
        line(
            out,
            "job.retry.max_attempts",
            self.retry_policy.max_attempts,
        );
        line(
            out,
            "job.retry.backoff_seconds",
            self.retry_policy.backoff_seconds,
        );
        line(out, "job.network", network_to_string(&self.network_policy));
        line(out, "job.token", token_to_string(&self.token_scope));
        for (key, value) in &self.inputs {
            line(out, format!("job.input.{key}"), value);
        }
        for (key, value) in &self.outputs {
            line(out, format!("job.output.{key}"), value);
        }
        let mut cache_mounts = self.cache_mounts.clone();
        cache_mounts.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
        for mount in cache_mounts {
            line(
                out,
                "job.cache",
                format!(
                    "{}|{}|{}|{}",
                    mount.name,
                    mount.path,
                    mount.mode.as_str(),
                    mount.fingerprint
                ),
            );
        }
        let mut artifacts = self.artifact_paths.clone();
        artifacts.sort_by(|a, b| a.name.cmp(&b.name));
        for artifact in artifacts {
            line(
                out,
                "job.artifact",
                format!(
                    "{}|{}|{}|{}",
                    artifact.name,
                    artifact.when.as_str(),
                    artifact.retention_days,
                    artifact.paths.join(",")
                ),
            );
        }
        for step in &self.steps {
            line(out, "job.step.id", &step.id);
            line(out, "job.step.name", &step.name);
            if let Some(command) = &step.command {
                line(out, "job.step.run", command);
            }
            if let Some(uses) = &step.uses {
                line(out, "job.step.uses", uses);
            }
            if let Some(dir) = &step.working_directory {
                line(out, "job.step.cwd", dir);
            }
            for (key, value) in &step.env {
                line(out, format!("job.step.env.{key}"), value);
            }
        }
    }
}

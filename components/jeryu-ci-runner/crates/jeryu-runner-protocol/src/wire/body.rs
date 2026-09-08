use super::validation::{
    MAX_ARTIFACT_PATHS, MAX_LABEL_BYTES, MAX_STEP_ENV_ENTRIES, MAX_TEXT_BYTES, ValidateWire,
    WireError, WireErrorCode, validate_env, validate_id, validate_label, validate_text,
    validate_unique, validate_workspace_path,
};
use jeryu_ci_ir::{ArtifactPath, ArtifactWhen, CacheMode, CacheMount, Step};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WireCacheMode {
    ReadOnly,
    ReadWriteQuarantine,
    ReadWriteTrusted,
}

impl From<&CacheMode> for WireCacheMode {
    fn from(value: &CacheMode) -> Self {
        match value {
            CacheMode::ReadOnly => Self::ReadOnly,
            CacheMode::ReadWriteQuarantine => Self::ReadWriteQuarantine,
            CacheMode::ReadWriteTrusted => Self::ReadWriteTrusted,
        }
    }
}

impl From<WireCacheMode> for CacheMode {
    fn from(value: WireCacheMode) -> Self {
        match value {
            WireCacheMode::ReadOnly => Self::ReadOnly,
            WireCacheMode::ReadWriteQuarantine => Self::ReadWriteQuarantine,
            WireCacheMode::ReadWriteTrusted => Self::ReadWriteTrusted,
        }
    }
}

impl WireCacheMode {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::ReadWriteQuarantine => "read-write-quarantine",
            Self::ReadWriteTrusted => "read-write-trusted",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WireArtifactWhen {
    Always,
    OnSuccess,
    OnFailure,
}

impl From<&ArtifactWhen> for WireArtifactWhen {
    fn from(value: &ArtifactWhen) -> Self {
        match value {
            ArtifactWhen::Always => Self::Always,
            ArtifactWhen::OnSuccess => Self::OnSuccess,
            ArtifactWhen::OnFailure => Self::OnFailure,
        }
    }
}

impl From<WireArtifactWhen> for ArtifactWhen {
    fn from(value: WireArtifactWhen) -> Self {
        match value {
            WireArtifactWhen::Always => Self::Always,
            WireArtifactWhen::OnSuccess => Self::OnSuccess,
            WireArtifactWhen::OnFailure => Self::OnFailure,
        }
    }
}

impl WireArtifactWhen {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::OnSuccess => "on-success",
            Self::OnFailure => "on-failure",
        }
    }
}

/// One bounded executable or reusable-action step.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireStep {
    pub id: String,
    pub name: String,
    pub command: Option<String>,
    pub uses: Option<String>,
    pub env: BTreeMap<String, String>,
    pub working_directory: Option<String>,
}

impl ValidateWire for WireStep {
    fn validate_wire(&self) -> Result<(), WireError> {
        validate_id("step.id", &self.id)?;
        validate_text("step.name", &self.name, MAX_LABEL_BYTES)?;
        if self.name.is_empty() || self.command.is_some() == self.uses.is_some() {
            return Err(WireError::new(WireErrorCode::InvalidField, "step.body"));
        }
        if let Some(command) = &self.command {
            validate_text("step.command", command, MAX_TEXT_BYTES)?;
            if command.is_empty() {
                return Err(WireError::new(WireErrorCode::InvalidField, "step.command"));
            }
        }
        if let Some(uses) = &self.uses {
            validate_text("step.uses", uses, MAX_TEXT_BYTES)?;
            if uses.is_empty() {
                return Err(WireError::new(WireErrorCode::InvalidField, "step.uses"));
            }
        }
        if let Some(directory) = &self.working_directory {
            validate_workspace_path("step.working_directory", directory)?;
        }
        validate_env("step.env", &self.env, MAX_STEP_ENV_ENTRIES)
    }
}

impl From<&Step> for WireStep {
    fn from(value: &Step) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            command: value.command.clone(),
            uses: value.uses.clone(),
            env: value.env.clone(),
            working_directory: value.working_directory.clone(),
        }
    }
}

impl From<WireStep> for Step {
    fn from(value: WireStep) -> Self {
        Self {
            id: value.id,
            name: value.name,
            command: value.command,
            uses: value.uses,
            env: value.env,
            working_directory: value.working_directory,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireCacheMount {
    pub name: String,
    pub path: String,
    pub mode: WireCacheMode,
    pub fingerprint: String,
}

impl ValidateWire for WireCacheMount {
    fn validate_wire(&self) -> Result<(), WireError> {
        validate_label("cache.name", &self.name)?;
        validate_workspace_path("cache.path", &self.path)?;
        validate_id("cache.fingerprint", &self.fingerprint)?;
        Ok(())
    }
}

impl From<&CacheMount> for WireCacheMount {
    fn from(value: &CacheMount) -> Self {
        Self {
            name: value.name.clone(),
            path: value.path.clone(),
            mode: (&value.mode).into(),
            fingerprint: value.fingerprint.clone(),
        }
    }
}

impl From<WireCacheMount> for CacheMount {
    fn from(value: WireCacheMount) -> Self {
        Self {
            name: value.name,
            path: value.path,
            mode: value.mode.into(),
            fingerprint: value.fingerprint,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireArtifactPath {
    pub name: String,
    pub paths: Vec<String>,
    pub when: WireArtifactWhen,
    pub retention_days: u32,
}

impl ValidateWire for WireArtifactPath {
    fn validate_wire(&self) -> Result<(), WireError> {
        validate_label("artifact.name", &self.name)?;
        if self.paths.is_empty() || self.paths.len() > MAX_ARTIFACT_PATHS {
            return Err(WireError::new(
                WireErrorCode::InvalidCollection,
                "artifact.paths",
            ));
        }
        for path in &self.paths {
            validate_workspace_path("artifact.paths", path)?;
        }
        validate_unique("artifact.paths", self.paths.iter().map(String::as_str))?;
        if !(1..=365).contains(&self.retention_days) {
            return Err(WireError::new(
                WireErrorCode::InvalidField,
                "artifact.retention_days",
            ));
        }
        Ok(())
    }
}

impl From<&ArtifactPath> for WireArtifactPath {
    fn from(value: &ArtifactPath) -> Self {
        Self {
            name: value.name.clone(),
            paths: value.paths.clone(),
            when: (&value.when).into(),
            retention_days: value.retention_days,
        }
    }
}

impl From<WireArtifactPath> for ArtifactPath {
    fn from(value: WireArtifactPath) -> Self {
        Self {
            name: value.name,
            paths: value.paths,
            when: value.when.into(),
            retention_days: value.retention_days,
        }
    }
}

impl_redacted_debug!(WireStep, WireCacheMount, WireArtifactPath);

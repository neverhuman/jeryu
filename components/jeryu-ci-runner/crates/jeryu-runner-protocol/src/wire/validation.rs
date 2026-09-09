use super::MAX_MESSAGE_BYTES;
use crate::PROTOCOL_VERSION;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub(super) const MAX_ID_BYTES: usize = 128;
pub(super) const MAX_LABEL_BYTES: usize = 64;
pub(super) const MAX_CLASSES: usize = 32;
pub(super) const MAX_TEXT_BYTES: usize = 32_768;
pub(super) const MAX_MESSAGE_TEXT_BYTES: usize = 1_024;
pub(super) const MAX_ENV_ENTRIES: usize = 256;
pub(super) const MAX_STEP_ENV_ENTRIES: usize = 128;
pub(super) const MAX_CACHE_MOUNTS: usize = 64;
pub(super) const MAX_ARTIFACTS: usize = 64;
pub(super) const MAX_ARTIFACT_PATHS: usize = 64;
pub(super) const MAX_JOB_TIMEOUT_SECONDS: u64 = 7 * 24 * 60 * 60;
pub(super) const MAX_LEASE_MILLIS: u64 = 24 * 60 * 60 * 1_000;
pub(super) const MAX_HEARTBEAT_MILLIS: u64 = 5 * 60 * 1_000;
const MAX_UNIX_MILLIS: u64 = 253_402_300_799_999;

/// Stable, non-sensitive category for a wire decode or validation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireErrorCode {
    MessageTooLarge,
    InvalidJson,
    InvalidProtocol,
    InvalidField,
    InvalidCollection,
    ContextMismatch,
    ReceiptMismatch,
}

/// Secret-safe wire failure retaining no rejected input value.
#[derive(Clone, PartialEq, Eq)]
pub struct WireError {
    code: WireErrorCode,
    field: &'static str,
}

impl WireError {
    pub(super) fn new(code: WireErrorCode, field: &'static str) -> Self {
        Self { code, field }
    }

    #[must_use]
    pub fn code(&self) -> WireErrorCode {
        self.code
    }

    #[must_use]
    pub fn field(&self) -> &'static str {
        self.field
    }
}

impl fmt::Debug for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WireError")
            .field("code", &self.code)
            .field("field", &self.field)
            .finish()
    }
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "runner wire validation failed at {}", self.field)
    }
}

impl std::error::Error for WireError {}

pub(super) trait ValidateWire {
    fn validate_wire(&self) -> Result<(), WireError>;
}

pub(super) struct CanonicalSha256(Sha256);

impl CanonicalSha256 {
    pub(super) fn new(domain: &str) -> Self {
        let mut digest = Self(Sha256::new());
        digest.field("domain", domain);
        digest
    }

    pub(super) fn field(&mut self, name: &str, value: &str) {
        self.0.update(name.len().to_string().as_bytes());
        self.0.update(b":");
        self.0.update(name.as_bytes());
        self.0.update(b":");
        self.0.update(value.len().to_string().as_bytes());
        self.0.update(b":");
        self.0.update(value.as_bytes());
        self.0.update(b"\n");
    }

    pub(super) fn finish(self) -> String {
        format!("sha256:{}", hex::encode(self.0.finalize()))
    }
}

pub(super) fn decode_message<T: DeserializeOwned + ValidateWire>(
    bytes: &[u8],
) -> Result<T, WireError> {
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(WireError::new(WireErrorCode::MessageTooLarge, "body"));
    }
    let message: T = serde_json::from_slice(bytes)
        .map_err(|_| WireError::new(WireErrorCode::InvalidJson, "body"))?;
    message.validate_wire()?;
    Ok(message)
}

pub(super) fn encode_message<T: Serialize + ValidateWire>(
    message: &T,
) -> Result<Vec<u8>, WireError> {
    message.validate_wire()?;
    let bytes = serde_json::to_vec(message)
        .map_err(|_| WireError::new(WireErrorCode::InvalidJson, "body"))?;
    if bytes.len() > MAX_MESSAGE_BYTES {
        return Err(WireError::new(WireErrorCode::MessageTooLarge, "body"));
    }
    Ok(bytes)
}

pub(super) fn validate_header(
    protocol_version: &str,
    message_type: &str,
    expected_type: &'static str,
) -> Result<(), WireError> {
    if protocol_version != PROTOCOL_VERSION || message_type != expected_type {
        return Err(WireError::new(
            WireErrorCode::InvalidProtocol,
            "protocol_version",
        ));
    }
    Ok(())
}

pub(super) fn validate_id(field: &'static str, value: &str) -> Result<(), WireError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && !value.contains("..")
        && !value.contains("//");
    valid
        .then_some(())
        .ok_or_else(|| WireError::new(WireErrorCode::InvalidField, field))
}

pub(super) fn validate_label(field: &'static str, value: &str) -> Result<(), WireError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_LABEL_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && !value.contains("..");
    valid
        .then_some(())
        .ok_or_else(|| WireError::new(WireErrorCode::InvalidField, field))
}

pub(super) fn validate_repository(value: &str) -> Result<(), WireError> {
    let Some((owner, repository)) = value.split_once('/') else {
        return Err(WireError::new(WireErrorCode::InvalidField, "repository"));
    };
    if repository.contains('/') {
        return Err(WireError::new(WireErrorCode::InvalidField, "repository"));
    }
    validate_label("repository", owner)?;
    let valid_repository = !repository.is_empty()
        && repository.len() <= MAX_LABEL_BYTES
        && repository
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        && repository
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && repository
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && !repository.contains("..");
    valid_repository
        .then_some(())
        .ok_or_else(|| WireError::new(WireErrorCode::InvalidField, "repository"))
}

pub(super) fn validate_check(value: &str) -> Result<(), WireError> {
    validate_id("required_check", value)?;
    value
        .contains('/')
        .then_some(())
        .ok_or_else(|| WireError::new(WireErrorCode::InvalidField, "required_check"))
}

pub(super) fn validate_git_sha(field: &'static str, value: &str) -> Result<(), WireError> {
    let valid = matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    valid
        .then_some(())
        .ok_or_else(|| WireError::new(WireErrorCode::InvalidField, field))
}

pub(super) fn validate_head_sha(value: &str) -> Result<(), WireError> {
    validate_git_sha("head_sha", value)
}

pub(super) fn validate_timestamp(field: &'static str, value: u64) -> Result<(), WireError> {
    (1..=MAX_UNIX_MILLIS)
        .contains(&value)
        .then_some(())
        .ok_or_else(|| WireError::new(WireErrorCode::InvalidField, field))
}

pub(super) fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), WireError> {
    let valid = value.len() <= max
        && value
            .chars()
            .all(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'));
    valid
        .then_some(())
        .ok_or_else(|| WireError::new(WireErrorCode::InvalidField, field))
}

pub(super) fn validate_digest(field: &'static str, value: &str) -> Result<(), WireError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(WireError::new(WireErrorCode::InvalidField, field));
    };
    let valid = hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    valid
        .then_some(())
        .ok_or_else(|| WireError::new(WireErrorCode::InvalidField, field))
}

pub(super) fn validate_unique<'a>(
    field: &'static str,
    values: impl IntoIterator<Item = &'a str>,
) -> Result<(), WireError> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .all(|value| seen.insert(value))
        .then_some(())
        .ok_or_else(|| WireError::new(WireErrorCode::InvalidCollection, field))
}

pub(super) fn validate_sorted_unique(
    field: &'static str,
    values: &[String],
) -> Result<(), WireError> {
    values
        .windows(2)
        .all(|pair| pair[0] < pair[1])
        .then_some(())
        .ok_or_else(|| WireError::new(WireErrorCode::InvalidCollection, field))
}

fn reserved_auth_name(name: &str) -> bool {
    let canonical = name.to_ascii_uppercase().replace('-', "_");
    matches!(
        canonical.as_str(),
        "AUTHORIZATION"
            | "PROXY_AUTHORIZATION"
            | "AUTH_TOKEN"
            | "RUNNER_TOKEN"
            | "REGISTRATION_TOKEN"
            | "ACCESS_TOKEN"
            | "GIT_PAT"
            | "JERYU_PAT"
            | "GITHUB_TOKEN"
            | "JERYU_TOKEN"
            | "AWS_ACCESS_KEY_ID"
            | "AWS_SECRET_ACCESS_KEY"
            | "SSH_AUTH_SOCK"
            | "GIT_ASKPASS"
            | "SSH_ASKPASS"
            | "JERYU_ACTOR"
            | "JERYU_API_IDENTITY"
            | "REMOTE_USER"
            | "AUTHENTICATED_USER"
            | "ACTOR"
            | "ACTOR_ID"
            | "PASSWORD"
            | "PASSWD"
            | "SECRET"
            | "TOKEN"
            | "CREDENTIAL"
            | "CREDENTIALS"
    ) || [
        "AUTH_",
        "CREDENTIAL_",
        "PASSWORD_",
        "PASSWD_",
        "SECRET_",
        "TOKEN_",
        "SPOOF_",
        "JERYU_AUTH_",
        "JERYU_ACTOR_",
        "JERYU_RUNNER_",
        "GIT_ASKPASS",
        "GIT_CONFIG_",
        "GIT_SSH_",
        "SSH_AUTH_",
        "SSH_ASKPASS_",
    ]
    .iter()
    .any(|prefix| canonical.starts_with(prefix))
        || canonical.ends_with("_AUTH")
        || canonical.ends_with("_AUTHORIZATION")
        || canonical.ends_with("_AUTH_TOKEN")
        || canonical.ends_with("_ACCESS_TOKEN")
        || canonical.ends_with("_RUNNER_TOKEN")
        || canonical.ends_with("_REGISTRATION_TOKEN")
        || canonical.ends_with("_TOKEN")
        || canonical.ends_with("_PAT")
        || canonical.ends_with("_PASSWORD")
        || canonical.ends_with("_PASSWD")
        || canonical.ends_with("_SECRET")
        || canonical.ends_with("_SECRET_KEY")
        || canonical.ends_with("_SECRET_ACCESS_KEY")
        || canonical.ends_with("_ACCESS_KEY_ID")
        || canonical.ends_with("_CREDENTIAL")
        || canonical.ends_with("_CREDENTIALS")
        || canonical.ends_with("_ACTOR")
        || canonical.ends_with("_ACTOR_ID")
        || canonical.ends_with("_ASKPASS")
        || canonical.ends_with("_AUTH_SOCK")
}

pub(super) fn validate_workspace_path(field: &'static str, value: &str) -> Result<(), WireError> {
    validate_text(field, value, MAX_ID_BYTES)?;
    let first = value.split('/').next().unwrap_or_default();
    let unsafe_path = value.is_empty()
        || value.starts_with('/')
        || value.contains('\\')
        || value.contains('\0')
        || first.as_bytes().get(1) == Some(&b':')
        || value
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."));
    (!unsafe_path)
        .then_some(())
        .ok_or_else(|| WireError::new(WireErrorCode::InvalidField, field))
}

pub(super) fn validate_env(
    field: &'static str,
    env: &BTreeMap<String, String>,
    max_entries: usize,
) -> Result<(), WireError> {
    if env.len() > max_entries {
        return Err(WireError::new(WireErrorCode::InvalidCollection, field));
    }
    for (name, value) in env {
        let valid_name = !name.is_empty()
            && name.len() <= MAX_LABEL_BYTES
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            && name
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
            && !reserved_auth_name(name);
        if !valid_name {
            return Err(WireError::new(WireErrorCode::InvalidField, field));
        }
        validate_text(field, value, MAX_TEXT_BYTES)?;
    }
    Ok(())
}

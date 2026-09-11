use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use chrono::{DateTime, Utc};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

use super::{ACTIVATION_ERROR, PAT_DEFAULT_TTL_DAYS, PAT_MAX_TTL_DAYS, require_login};
use crate::errors::{ForgeError, Result};

pub(super) fn normalize_team_bindings(bindings: &[String]) -> Result<Vec<String>> {
    let mut normalized = BTreeSet::new();
    for binding in bindings {
        let binding = binding.trim();
        let (organization, team) = split_team_binding(binding)?;
        require_login(organization)?;
        require_login(team)?;
        normalized.insert(format!("{organization}/{team}"));
    }
    Ok(normalized.into_iter().collect())
}

pub(super) fn split_team_binding(binding: &str) -> Result<(&str, &str)> {
    let (organization, team) = binding.split_once('/').ok_or_else(|| {
        ForgeError::Validation("team binding must be canonical organization/team".to_string())
    })?;
    if organization.is_empty() || team.is_empty() || team.contains('/') {
        return Err(ForgeError::Validation(
            "team binding must be canonical organization/team".to_string(),
        ));
    }
    Ok((organization, team))
}

pub(super) fn activation_error() -> ForgeError {
    ForgeError::Validation(ACTIVATION_ERROR.to_string())
}

pub(super) fn next_auth_epoch(current: u64) -> Result<u64> {
    current
        .checked_add(1)
        .ok_or_else(|| ForgeError::Storage("account auth epoch overflow".to_string()))
}

pub(super) fn require_password(password: &str) -> Result<()> {
    if password.len() < 12 {
        return Err(ForgeError::Validation(
            "password must be at least 12 bytes".to_string(),
        ));
    }
    Ok(())
}

fn argon2() -> Result<Argon2<'static>> {
    let params = Params::new(19_456, 2, 1, None)
        .map_err(|err| ForgeError::Storage(format!("argon2 params: {err}")))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

pub(super) fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2()?
        .hash_password(password.as_bytes(), &salt)
        .map_err(|err| ForgeError::Storage(format!("hash password: {err}")))?
        .to_string();
    if !hash.starts_with("$argon2id$") {
        return Err(ForgeError::Storage(
            "password hash was not encoded as argon2id PHC".to_string(),
        ));
    }
    Ok(hash)
}

pub(super) fn verify_password(password: &str, password_hash: &str) -> Result<()> {
    let parsed = PasswordHash::new(password_hash)
        .map_err(|_| ForgeError::Validation("invalid login or password".to_string()))?;
    argon2()?
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| ForgeError::Validation("invalid login or password".to_string()))
}

pub(super) fn validate_pat_expiry(
    expires_at: Option<DateTime<Utc>>,
) -> Result<Option<DateTime<Utc>>> {
    let now = Utc::now();
    let expires_at =
        expires_at.unwrap_or_else(|| now + chrono::Duration::days(PAT_DEFAULT_TTL_DAYS));
    if expires_at <= now {
        return Err(ForgeError::Validation(
            "token expiry must be in the future".to_string(),
        ));
    }
    if expires_at > now + chrono::Duration::days(PAT_MAX_TTL_DAYS) {
        return Err(ForgeError::Validation(format!(
            "token expiry may not exceed {PAT_MAX_TTL_DAYS} days"
        )));
    }
    Ok(Some(expires_at))
}

pub(in crate::core) fn token_hash(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

pub(in crate::core) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

pub(crate) fn random_secret() -> Result<String> {
    let mut bytes = [0u8; 32];
    OsRng
        .try_fill_bytes(&mut bytes)
        .map_err(|err| ForgeError::Storage(format!("read randomness: {err}")))?;
    Ok(hex::encode(bytes))
}

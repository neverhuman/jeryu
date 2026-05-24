//! Owner: Autonomy signing-secret resolution — pure helpers
//! Proof: `cargo nextest run -p jeryu -- autonomy::signing_secrets`
//! Invariants: helpers manage env-file IO and seed parsing for ed25519 signing
//! keys. They never touch the network and never embed cleartext seeds in error
//! messages.

use std::path::{Path, PathBuf};

use crate::autonomy::signing::EdSigningKey;
use crate::env_file;
use anyhow::{Context, Result, anyhow};
use base64::Engine;

pub(super) fn is_ci() -> bool {
    std::env::var("CI")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

pub(super) fn slot_env_key(slot: &str) -> String {
    format!(
        "JERYU_{}_ED25519_SEED_HEX",
        slot.chars()
            .map(|c| if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            })
            .collect::<String>()
    )
}

pub(super) fn signing_env_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("HOME is not available"))?;
    Ok(home.join(".jeryu/secrets/signing.env"))
}

pub(super) fn read_named_secret_from_file(slot: &str, slot_env: &str) -> Result<Option<String>> {
    let path = signing_env_path()?;
    let contents = match env_file::read_text_file_optional(&path, "read")? {
        Some(contents) => contents,
        None => return Ok(None),
    };
    let aliases = [
        slot_env.to_string(),
        format!("{}_ED25519_SEED_HEX", slot.to_ascii_uppercase()),
        "JERYU_ED25519_SEED_HEX".to_string(),
    ];
    let values = env_file::parse_env_text(&contents);
    for alias in aliases {
        if let Some(value) = values.get(&alias).filter(|value| !value.is_empty()) {
            return Ok(Some(value.clone()));
        }
    }
    Ok(None)
}

pub(super) fn ensure_local_seed_for_slot(slot_env: &str) -> Result<String> {
    let path = signing_env_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let existing = match env_file::read_text_file_optional(&path, "read")? {
        Some(contents) => contents,
        None => String::new(),
    };
    if let Some(seed) = find_env_value(&existing, slot_env)
        && !seed.trim().is_empty()
    {
        env_file::write_text_file_secure(&path, &existing, "open", "write")?;
        return Ok(seed);
    }

    let seed: [u8; 32] = rand::random();
    let seed_hex = hex::encode(seed);
    let updated = env_file::upsert_env_text(&existing, slot_env, &seed_hex);
    env_file::write_text_file_secure(&path, &updated, "open", "write")?;
    Ok(seed_hex)
}

pub(super) fn export_public_key_if_requested(
    key: &EdSigningKey,
    public_keys_dir: Option<&Path>,
) -> Result<()> {
    let Some(public_keys_dir) = public_keys_dir else {
        return Ok(());
    };
    std::fs::create_dir_all(public_keys_dir)
        .with_context(|| format!("create {}", public_keys_dir.display()))?;
    let file_name = format!("{}.ed25519.pub", safe_key_file_stem(&key.key_id));
    let path = public_keys_dir.join(file_name);
    std::fs::write(&path, format!("{}\n", key.public_key_hex()))
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

pub(super) fn safe_key_file_stem(key_id: &str) -> String {
    let mut stem: String = key_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if stem.is_empty() {
        stem = "signing-key".to_string();
    }
    stem
}

pub(super) fn ed25519_from_seed_string(key_id: &str, value: &str) -> Result<EdSigningKey> {
    let trimmed = value.trim();
    let bytes = match hex::decode(trimmed) {
        Ok(bytes) => bytes,
        Err(_) => base64::engine::general_purpose::STANDARD
            .decode(trimmed)
            .context("seed must be hex or base64")?,
    };
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow!("ed25519 seed must decode to exactly 32 bytes"))?;
    Ok(EdSigningKey::from_seed(key_id, seed))
}

pub(super) fn find_env_value(text: &str, key: &str) -> Option<String> {
    env_file::parse_env_text(text).get(key).cloned()
}

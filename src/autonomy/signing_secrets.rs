//! Local Ed25519 signing-secret resolution.
//!
//! CI and production lanes remain fail-closed: callers must inject a seed via
//! environment or a pre-provisioned signing env file. Non-CI local ownership
//! proofs may create a persistent slot-specific seed under
//! `~/.jeryu/secrets/signing.env`.

use anyhow::{Context, Result, anyhow};
use std::path::Path;

use super::signing::EdSigningKey;

#[path = "signing_secrets_helpers.rs"]
mod signing_secrets_helpers;
use signing_secrets_helpers::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigningKeyMode {
    /// Missing seeds are an error in every environment.
    Strict,
    /// Non-CI local runs may create and persist a slot-specific seed.
    LocalPersistentAllowed,
    /// Non-CI local runs may generate an in-memory key if no seed exists.
    LocalEphemeralAllowed,
}

pub fn resolve_ed25519_signing_key(
    slot: &str,
    key_id: &str,
    mode: SigningKeyMode,
    public_keys_dir: Option<&Path>,
) -> Result<EdSigningKey> {
    let slot_env = slot_env_key(slot);
    for env_name in [
        slot_env.as_str(),
        "JERYU_ED25519_SEED_HEX",
        "JERYU_SIGNING_KEY_SEED_HEX",
    ] {
        if let Ok(value) = std::env::var(env_name)
            && !value.trim().is_empty()
        {
            let key = ed25519_from_seed_string(key_id, &value)
                .with_context(|| format!("decode signing seed from secret slot {env_name}"))?;
            export_public_key_if_requested(&key, public_keys_dir)?;
            return Ok(key);
        }
    }

    if let Some(seed) = read_named_secret_from_file(slot, &slot_env)? {
        let key = ed25519_from_seed_string(key_id, &seed)
            .with_context(|| format!("decode signing seed from secret slot {slot_env}"))?;
        export_public_key_if_requested(&key, public_keys_dir)?;
        return Ok(key);
    }

    if is_ci() {
        return Err(missing_seed_error(&slot_env));
    }

    let key = match mode {
        SigningKeyMode::Strict => return Err(missing_seed_error(&slot_env)),
        SigningKeyMode::LocalPersistentAllowed => {
            let seed = ensure_local_seed_for_slot(&slot_env)?;
            ed25519_from_seed_string(key_id, &seed)
                .with_context(|| format!("decode generated signing seed for {slot_env}"))?
        }
        SigningKeyMode::LocalEphemeralAllowed => {
            EdSigningKey::generate(format!("{key_id}.local-ephemeral"))
        }
    };
    export_public_key_if_requested(&key, public_keys_dir)?;
    Ok(key)
}

fn missing_seed_error(slot_env: &str) -> anyhow::Error {
    anyhow!(
        "missing signing secret slot {slot_env}; set {slot_env}, JERYU_ED25519_SEED_HEX, or ~/.jeryu/secrets/signing.env"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    struct EnvGuard {
        saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl EnvGuard {
        fn new(home: &Path) -> Self {
            let keys = [
                "HOME",
                "CI",
                "JERYU_MR_ED25519_SEED_HEX",
                "JERYU_ED25519_SEED_HEX",
                "JERYU_SIGNING_KEY_SEED_HEX",
            ];
            let saved = keys
                .iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect::<Vec<_>>();
            // SAFETY: these tests hold a process-wide mutex before mutating
            // environment variables and restore the saved values in Drop.
            unsafe {
                std::env::set_var("HOME", home);
                std::env::remove_var("CI");
                std::env::remove_var("JERYU_MR_ED25519_SEED_HEX");
                std::env::remove_var("JERYU_ED25519_SEED_HEX");
                std::env::remove_var("JERYU_SIGNING_KEY_SEED_HEX");
            }
            Self { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: EnvGuard is only constructed while the same global
            // mutex is held, so restoration is serialized with the mutation.
            unsafe {
                for (key, value) in self.saved.drain(..) {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }
    }

    #[test]
    fn missing_local_seed_is_generated_upserted_and_public_key_exported() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::new(temp.path());
        let keys_dir = temp.path().join("repo/.jeryu/autonomy/keys");

        let key = resolve_ed25519_signing_key(
            "mr",
            "mr.validate.v1",
            SigningKeyMode::LocalPersistentAllowed,
            Some(&keys_dir),
        )
        .unwrap();

        let signing_env = temp.path().join(".jeryu/secrets/signing.env");
        let text = std::fs::read_to_string(&signing_env).unwrap();
        assert_eq!(text.matches("JERYU_MR_ED25519_SEED_HEX=").count(), 1);
        let seed = find_env_value(&text, "JERYU_MR_ED25519_SEED_HEX").unwrap();
        assert_eq!(hex::decode(seed).unwrap().len(), 32);
        assert_eq!(
            std::fs::read_to_string(keys_dir.join("mr.validate.v1.ed25519.pub"))
                .unwrap()
                .trim(),
            key.public_key_hex()
        );
        assert_secret_file_mode(&signing_env);
    }

    #[test]
    fn existing_local_seed_is_preserved_and_chmodded() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::new(temp.path());
        let signing_env = temp.path().join(".jeryu/secrets/signing.env");
        std::fs::create_dir_all(signing_env.parent().unwrap()).unwrap();
        let seed_hex = hex::encode([1u8; 32]);
        std::fs::write(
            &signing_env,
            format!("OTHER_KEY=kept\nJERYU_MR_ED25519_SEED_HEX={seed_hex}\n"),
        )
        .unwrap();
        set_secret_file_mode(&signing_env, 0o664);

        let key = resolve_ed25519_signing_key(
            "mr",
            "mr.validate.v1",
            SigningKeyMode::LocalPersistentAllowed,
            None,
        )
        .unwrap();

        let text = std::fs::read_to_string(&signing_env).unwrap();
        assert!(text.contains("OTHER_KEY=kept"));
        assert_eq!(text.matches("JERYU_MR_ED25519_SEED_HEX=").count(), 1);
        assert_eq!(
            key.public_key_hex(),
            EdSigningKey::from_seed("mr.validate.v1", [1u8; 32]).public_key_hex()
        );
        assert_secret_file_mode(&signing_env);
    }

    #[test]
    fn ci_missing_seed_fails_closed_without_creating_local_material() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::new(temp.path());
        // SAFETY: the test holds the process-wide env mutex through
        // EnvGuard's lifetime, so this mutation cannot race another test.
        unsafe {
            std::env::set_var("CI", "true");
        }

        let err = match resolve_ed25519_signing_key(
            "mr",
            "mr.validate.v1",
            SigningKeyMode::LocalPersistentAllowed,
            None,
        ) {
            Ok(_) => panic!("missing CI seed should fail closed"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("missing signing secret slot"));
        assert!(!temp.path().join(".jeryu/secrets/signing.env").exists());
    }

    #[cfg(unix)]
    fn set_secret_file_mode(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(mode);
        std::fs::set_permissions(path, perms).unwrap();
    }

    #[cfg(not(unix))]
    fn set_secret_file_mode(_path: &Path, _mode: u32) {}

    #[cfg(unix)]
    fn assert_secret_file_mode(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(not(unix))]
    fn assert_secret_file_mode(_path: &Path) {}
}

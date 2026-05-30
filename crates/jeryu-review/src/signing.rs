//! Receipt / verdict signing.
//!
//! Algorithms distinguished by the `algo` field of `Signature`:
//! - `stub` — placeholder; rejected by enforcement-mode verifiers
//!   (see `conditions::evidence_signature_invalid`).
//! - `sha256-hmac-stub` — symmetric HMAC placeholder; rejected in enforcement.
//! - `ed25519` — real per-agent ed25519 signing via [`EdSigningKey`]; accepted
//!   by enforcement-mode verifiers.
//!
//! The wire field names (`key_id`, `algo`, `value`) are frozen here: receipts
//! are signed over their own canonical JSON with the signature stubbed, so any
//! rename would recompute every historical signature and break replay.

use ed25519_dalek::{Signer, SigningKey as DalekSigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Signature {
    pub key_id: String,
    pub algo: String,
    pub value: String,
}

impl Signature {
    /// Unsigned placeholder. The wire-format `algo: "stub"` is preserved because
    /// enforcement-mode verifiers already reject it, so an unsigned object in
    /// flight is always caught at the gate boundary.
    pub fn default_unsigned() -> Self {
        Self {
            key_id: "unsigned".into(),
            algo: "stub".into(),
            value: "0".repeat(64),
        }
    }

    /// Test/placeholder alias for [`Signature::default_unsigned`].
    pub fn stub() -> Self {
        Self::default_unsigned()
    }
}

/// SHA-256 hex digest helper, returned as `sha256:<hex>`. The `sha256:` prefix
/// is part of the prompt_sha / raw_response_sha wire format and is load-bearing
/// for replay.
pub fn sha256_digest(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{}", hex::encode(h.finalize()))
}

/// Per-agent ed25519 signing key. Public keys serialize as 32-byte hex over the
/// verifying-key bytes.
pub struct EdSigningKey {
    pub key_id: String,
    inner: DalekSigningKey,
}

impl EdSigningKey {
    /// Build a key from a 32-byte seed. Deterministic — same seed → same key.
    pub fn from_seed(key_id: impl Into<String>, seed: [u8; 32]) -> Self {
        Self {
            key_id: key_id.into(),
            inner: DalekSigningKey::from_bytes(&seed),
        }
    }

    /// Generate a fresh random key (test/dev convenience).
    pub fn generate(key_id: impl Into<String>) -> Self {
        let seed: [u8; 32] = rand::random();
        Self::from_seed(key_id, seed)
    }

    /// Sign raw bytes; returns a `Signature` with `algo: "ed25519"`.
    pub fn sign_raw(&self, body: &[u8]) -> Signature {
        let sig = self.inner.sign(body);
        Signature {
            key_id: self.key_id.clone(),
            algo: "ed25519".into(),
            value: hex::encode(sig.to_bytes()),
        }
    }

    /// Export the public-key bytes as 32-byte hex.
    pub fn public_key_hex(&self) -> String {
        hex::encode(self.inner.verifying_key().to_bytes())
    }

    /// Return the corresponding verifier (cheap; derived from the secret).
    pub fn verifier(&self) -> EdVerifier {
        EdVerifier {
            key_id: self.key_id.clone(),
            inner: self.inner.verifying_key(),
        }
    }
}

/// Public-key verifier for `algo: "ed25519"` signatures.
pub struct EdVerifier {
    pub key_id: String,
    inner: VerifyingKey,
}

impl EdVerifier {
    /// Reconstruct from the 32-byte hex public key.
    pub fn from_public_key_hex(key_id: impl Into<String>, hex_str: &str) -> Result<Self, String> {
        let bytes = hex::decode(hex_str.trim()).map_err(|e| format!("hex decode: {e}"))?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "public key must be 32 bytes".to_string())?;
        let vk = VerifyingKey::from_bytes(&arr).map_err(|e| format!("vk decode: {e}"))?;
        Ok(Self {
            key_id: key_id.into(),
            inner: vk,
        })
    }

    /// Verify `body` against `sig`. Rejects on algo mismatch, key-id mismatch,
    /// malformed signature bytes, or signature/body mismatch.
    pub fn verify(&self, body: &[u8], sig: &Signature) -> bool {
        if sig.algo != "ed25519" {
            return false;
        }
        if sig.key_id != self.key_id {
            return false;
        }
        let bytes = match hex::decode(&sig.value) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let arr: [u8; 64] = match bytes.try_into() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let dalek_sig = ed25519_dalek::Signature::from_bytes(&arr);
        self.inner.verify(body, &dalek_sig).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_signature_round_trips() {
        let s = Signature::stub();
        let j = serde_json::to_string(&s).unwrap();
        let back: Signature = serde_json::from_str(&j).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn sha256_digest_is_stable() {
        let d = sha256_digest(b"abc");
        assert_eq!(
            d,
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn ed25519_sign_and_verify_round_trip() {
        let k = EdSigningKey::from_seed("agent.judge.v1", [7u8; 32]);
        let v = k.verifier();
        let sig = k.sign_raw(b"hello world");
        assert_eq!(sig.algo, "ed25519");
        assert_eq!(sig.key_id, "agent.judge.v1");
        assert!(v.verify(b"hello world", &sig));
        assert!(!v.verify(b"tampered", &sig));
    }

    #[test]
    fn ed25519_from_seed_is_deterministic() {
        let k1 = EdSigningKey::from_seed("k", [42u8; 32]);
        let k2 = EdSigningKey::from_seed("k", [42u8; 32]);
        assert_eq!(k1.public_key_hex(), k2.public_key_hex());
        assert_eq!(k1.sign_raw(b"x").value, k2.sign_raw(b"x").value);
    }

    #[test]
    fn ed25519_wrong_algo_rejects() {
        let k = EdSigningKey::from_seed("a", [1u8; 32]);
        let v = k.verifier();
        assert!(!v.verify(b"x", &Signature::stub()));
    }

    #[test]
    fn ed25519_pubkey_hex_round_trips() {
        let k = EdSigningKey::from_seed("a", [9u8; 32]);
        let v = EdVerifier::from_public_key_hex("a", &k.public_key_hex()).unwrap();
        assert!(v.verify(b"payload", &k.sign_raw(b"payload")));
    }
}

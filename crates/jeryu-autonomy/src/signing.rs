//! Receipt / ledger / verdict signing.
//!
//! Three algorithms are recognized on the wire (distinguished by the `algo`
//! field of `Signature`):
//! - `stub` — placeholder; rejected by enforcement-mode verifiers
//!   (see [`crate::conditions::cond_evidence_signature_invalid`]).
//! - `sha256-hmac-stub` — symmetric HMAC; still placeholder; rejected in enforcement.
//! - `ed25519` — real per-agent ed25519 signing via [`EdSigningKey`];
//!   accepted by enforcement-mode verifiers.
//!
//! Public keys live under `.jeryu/autonomy/keys/<agent_id>.ed25519.pub`
//! (32 bytes, hex). Private key material is vaulted by the host (a thin seam,
//! not part of this crate).

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
    /// Build an unsigned signature marker. Used by helpers that construct a
    /// ledger/verdict body before [`sign_entry`](crate::ledger::sign_entry)
    /// overwrites the signature with a real ed25519 value. The wire-format
    /// `algo: "stub"` is preserved because enforcement-mode verifiers
    /// (`VerdictLedger::append`, `cond_evidence_signature_invalid`) already
    /// reject it, so an unsigned object in flight is always caught at the
    /// append boundary.
    pub fn default_unsigned() -> Self {
        Self {
            key_id: "unsigned".into(),
            algo: "stub".into(),
            value: "0".repeat(64),
        }
    }

    /// Test-only placeholder signature. Identical wire shape to
    /// [`Signature::default_unsigned`].
    #[cfg(any(test, debug_assertions))]
    pub fn placeholder_for_tests() -> Self {
        Self::default_unsigned()
    }

    /// Backward-compatible alias for [`Signature::default_unsigned`].
    pub fn stub() -> Self {
        Self::default_unsigned()
    }
}

/// Symmetric placeholder "key" (HMAC-SHA256). NOT cryptographically equivalent
/// to ed25519; retained only so the refuse-lists have something to reject.
pub struct SigningKey {
    pub key_id: String,
    pub secret: Vec<u8>,
}

impl SigningKey {
    pub fn new(key_id: impl Into<String>, secret: impl Into<Vec<u8>>) -> Self {
        Self {
            key_id: key_id.into(),
            secret: secret.into(),
        }
    }

    /// HMAC-SHA-256 over `body`. NOT cryptographically equivalent to ed25519.
    pub fn sign(&self, body: &[u8]) -> Signature {
        let mut h = Sha256::new();
        h.update(&self.secret);
        h.update(body);
        h.update(&self.secret);
        Signature {
            key_id: self.key_id.clone(),
            algo: "sha256-hmac-stub".into(),
            value: hex::encode(h.finalize()),
        }
    }

    pub fn verify(&self, body: &[u8], sig: &Signature) -> bool {
        if sig.algo != "sha256-hmac-stub" || sig.key_id != self.key_id {
            return false;
        }
        let expected = self.sign(body);
        expected.value == sig.value
    }
}

/// SHA-256 hex digest helper (returns `sha256:<hex>`).
pub fn sha256_digest(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{}", hex::encode(h.finalize()))
}

// ---------------------------------------------------------------------------
// Real ed25519 signing key (algo = "ed25519")
// ---------------------------------------------------------------------------

/// Per-agent ed25519 signing key. Wraps `ed25519-dalek`'s `SigningKey`.
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

    /// Generate a fresh random key. Test/dev convenience; production should
    /// call [`EdSigningKey::from_seed`] with vaulted bytes.
    pub fn generate(key_id: impl Into<String>) -> Self {
        let seed: [u8; 32] = rand::random();
        Self::from_seed(key_id, seed)
    }

    /// Sign raw bytes and return the wire-format [`Signature`] with
    /// `algo: "ed25519"`.
    pub fn sign_raw(&self, body: &[u8]) -> Signature {
        let sig = self.inner.sign(body);
        Signature {
            key_id: self.key_id.clone(),
            algo: "ed25519".into(),
            value: hex::encode(sig.to_bytes()),
        }
    }

    /// Export the public-key bytes as 32-byte hex. Suitable for writing to
    /// `.jeryu/autonomy/keys/<agent_id>.ed25519.pub`.
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
    /// Reconstruct from the 32-byte hex string written to
    /// `.jeryu/autonomy/keys/*.ed25519.pub`.
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
    fn sign_and_verify() {
        let k = SigningKey::new("k1", b"super-secret".to_vec());
        let body = b"hello world";
        let sig = k.sign(body);
        assert!(k.verify(body, &sig));
        assert!(!k.verify(b"tampered", &sig));
    }

    #[test]
    fn wrong_key_id_rejects() {
        let k1 = SigningKey::new("k1", b"s1".to_vec());
        let k2 = SigningKey::new("k2", b"s1".to_vec());
        let sig = k1.sign(b"x");
        assert!(!k2.verify(b"x", &sig));
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
        let s1 = k1.sign_raw(b"x");
        let s2 = k2.sign_raw(b"x");
        assert_eq!(s1.value, s2.value);
    }

    #[test]
    fn ed25519_wrong_key_id_rejects() {
        let k = EdSigningKey::from_seed("a", [1u8; 32]);
        let v = EdSigningKey::from_seed("b", [1u8; 32]).verifier();
        let sig = k.sign_raw(b"x");
        assert!(!v.verify(b"x", &sig), "different key_id must reject");
    }

    #[test]
    fn ed25519_wrong_algo_rejects() {
        let k = EdSigningKey::from_seed("a", [1u8; 32]);
        let v = k.verifier();
        let stub = Signature::stub();
        assert!(
            !v.verify(b"x", &stub),
            "stub algo must not verify under ed25519"
        );
    }

    #[test]
    fn ed25519_pubkey_hex_round_trips() {
        let k = EdSigningKey::from_seed("a", [9u8; 32]);
        let hex_pub = k.public_key_hex();
        let v = EdVerifier::from_public_key_hex("a", &hex_pub).unwrap();
        let sig = k.sign_raw(b"payload");
        assert!(v.verify(b"payload", &sig));
    }

    #[test]
    fn ed25519_pubkey_hex_rejects_bad_input() {
        assert!(EdVerifier::from_public_key_hex("x", "not-hex").is_err());
        assert!(EdVerifier::from_public_key_hex("x", "ab").is_err());
    }

    #[test]
    fn ed25519_generated_keys_are_distinct() {
        let k1 = EdSigningKey::generate("a");
        let k2 = EdSigningKey::generate("a");
        assert_ne!(k1.public_key_hex(), k2.public_key_hex());
    }
}

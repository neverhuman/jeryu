//! Shared receipt / ledger / verdict signing core for the jeryu autonomy gate.
//!
//! This is the single owner of the cryptographic signing primitives that both
//! the control plane (`jeryu-autonomy`) and the reviewer orchestrator
//! (`jeryu-review`) bind their receipts and verdicts to. Hosting it in one leaf
//! crate keeps the `Signature` wire object, the ed25519 signing/verification
//! path, the low-trust HMAC path, and the digest helper byte-for-byte identical
//! across both consumers — historical signatures stay verifiable on either side.
//!
//! Three algorithms are recognized on the wire (distinguished by the `algo`
//! field of [`Signature`]):
//! - `unsigned` — no cryptographic signature has been applied; rejected by
//!   enforcement-mode verifiers in the consuming crates.
//! - `hmac-sha256-insecure` — symmetric HMAC; not enforcement-grade (any holder
//!   of the shared secret can forge it); rejected in enforcement.
//! - `ed25519` — real per-agent ed25519 signing via [`EdSigningKey`]; accepted
//!   by enforcement-mode verifiers.
//!
//! Public keys live under `.jeryu/autonomy/keys/<agent_id>.ed25519.pub`
//! (32 bytes, hex). Private key material is vaulted by the host (a thin seam,
//! not part of this crate).

#![forbid(unsafe_code)]

use ed25519_dalek::{Signer, SigningKey as DalekSigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Wire-format signature object carried by every receipt, ledger entry, and
/// verdict. The field names (`key_id`, `algo`, `value`) are frozen: objects are
/// signed over their own canonical JSON with the signature zeroed, so any rename
/// would recompute every historical signature and break replay.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Signature {
    pub key_id: String,
    pub algo: String,
    pub value: String,
}

impl Signature {
    /// Build an unsigned signature marker. Used by helpers that construct a
    /// ledger/verdict body before a real ed25519 value is signed in. The
    /// wire-format `algo: "unsigned"` is load-bearing: enforcement-mode
    /// verifiers reject any algo other than `ed25519`, so an unsigned object in
    /// flight is always caught at the append/gate boundary.
    pub fn default_unsigned() -> Self {
        Self {
            key_id: "unsigned".into(),
            algo: "unsigned".into(),
            value: "0".repeat(64),
        }
    }

    /// Construct the unsigned marker. Alias for [`Signature::default_unsigned`];
    /// reads better at call sites that just want "a not-yet-signed signature".
    pub fn unsigned() -> Self {
        Self::default_unsigned()
    }
}

/// Symmetric HMAC-SHA256 key. NOT enforcement-grade: any holder of the shared
/// secret can forge a signature, so enforcement-mode verifiers reject its
/// `hmac-sha256-insecure` algo. Retained for the refuse-lists and low-trust paths.
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
            algo: "hmac-sha256-insecure".into(),
            value: hex::encode(h.finalize()),
        }
    }

    pub fn verify(&self, body: &[u8], sig: &Signature) -> bool {
        if sig.algo != "hmac-sha256-insecure" || sig.key_id != self.key_id {
            return false;
        }
        let expected = self.sign(body);
        expected.value == sig.value
    }
}

/// SHA-256 hex digest helper (returns `sha256:<hex>`). The `sha256:` prefix is
/// part of the prompt_sha / raw_response_sha / evidence-digest wire format and
/// is load-bearing for replay.
pub fn sha256_digest(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{}", hex::encode(h.finalize()))
}

// ---------------------------------------------------------------------------
// Real ed25519 signing key (algo = "ed25519")
// ---------------------------------------------------------------------------

/// Per-agent ed25519 signing key. Wraps `ed25519-dalek`'s `SigningKey`. Public
/// keys serialize as 32-byte hex over the verifying-key bytes.
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
    fn unsigned_signature_round_trips() {
        let s = Signature::unsigned();
        let j = serde_json::to_string(&s).unwrap();
        let back: Signature = serde_json::from_str(&j).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn unsigned_marker_has_frozen_wire_shape() {
        // The unsigned marker is a load-bearing wire constant: enforcement-mode
        // verifiers in the consumer crates key off `algo == "unsigned"`.
        let s = Signature::unsigned();
        assert_eq!(s.key_id, "unsigned");
        assert_eq!(s.algo, "unsigned");
        assert_eq!(s.value, "0".repeat(64));
        assert_eq!(Signature::default_unsigned(), s);
    }

    #[test]
    fn hmac_sign_and_verify() {
        let k = SigningKey::new("k1", b"super-secret".to_vec());
        let body = b"hello world";
        let sig = k.sign(body);
        assert_eq!(sig.algo, "hmac-sha256-insecure");
        assert!(k.verify(body, &sig));
        assert!(!k.verify(b"tampered", &sig));
    }

    #[test]
    fn hmac_wrong_key_id_rejects() {
        let k1 = SigningKey::new("k1", b"s1".to_vec());
        let k2 = SigningKey::new("k2", b"s1".to_vec());
        let sig = k1.sign(b"x");
        assert!(!k2.verify(b"x", &sig));
    }

    #[test]
    fn hmac_does_not_verify_under_ed25519() {
        // Cross-algo confusion must fail: an HMAC value must never satisfy the
        // ed25519 verifier even with a matching key_id.
        let hmac = SigningKey::new("shared", b"secret".to_vec());
        let sig = hmac.sign(b"x");
        let ed = EdSigningKey::from_seed("shared", [3u8; 32]).verifier();
        assert!(!ed.verify(b"x", &sig));
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
        assert!(
            !v.verify(b"x", &Signature::unsigned()),
            "unsigned algo must not verify under ed25519"
        );
    }

    #[test]
    fn ed25519_pubkey_hex_round_trips() {
        let k = EdSigningKey::from_seed("a", [9u8; 32]);
        let v = EdVerifier::from_public_key_hex("a", &k.public_key_hex()).unwrap();
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

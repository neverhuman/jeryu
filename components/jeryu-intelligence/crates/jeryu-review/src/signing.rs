//! Receipt / verdict signing.
//!
//! The signing primitives are owned by the shared [`jeryu_signing`] leaf crate
//! so the `Signature` wire object, the ed25519 path, and the digest helper stay
//! byte-for-byte identical with `jeryu-autonomy`. This module re-exports them
//! under the historical `crate::signing::*` path used throughout the reviewer
//! orchestrator.
//!
//! Algorithms distinguished by the `algo` field of [`Signature`]:
//! - `unsigned` — no cryptographic signature was applied; rejected by
//!   enforcement-mode verifiers (see `conditions::cond_evidence_signature_invalid`).
//! - `hmac-sha256-insecure` — symmetric HMAC; not key-bound to an agent, so
//!   rejected in enforcement.
//! - `ed25519` — real per-agent ed25519 signing via [`EdSigningKey`]; accepted
//!   by enforcement-mode verifiers.
//!
//! The wire field names (`key_id`, `algo`, `value`) are frozen: receipts are
//! signed over their own canonical JSON with the signature zeroed, so any rename
//! would recompute every historical signature and break replay.

pub use jeryu_signing::{EdSigningKey, EdVerifier, Signature, sha256_digest};

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
        assert!(!v.verify(b"x", &Signature::unsigned()));
    }

    #[test]
    fn ed25519_pubkey_hex_round_trips() {
        let k = EdSigningKey::from_seed("a", [9u8; 32]);
        let v = EdVerifier::from_public_key_hex("a", &k.public_key_hex()).unwrap();
        assert!(v.verify(b"payload", &k.sign_raw(b"payload")));
    }
}

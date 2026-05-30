//! Receipt / ledger / verdict signing.
//!
//! The signing primitives are owned by the shared [`jeryu_signing`] leaf crate
//! so the `Signature` wire object, the ed25519 path, the insecure-HMAC path, and
//! the digest helper stay byte-for-byte identical with `jeryu-review`. This
//! module re-exports them under the historical `crate::signing::*` path used
//! throughout the control plane.
//!
//! Three algorithms are recognized on the wire (the `algo` field of
//! [`Signature`]):
//! - `unsigned` — no cryptographic signature; rejected by enforcement-mode
//!   verifiers (see [`crate::conditions::cond_evidence_signature_invalid`]).
//! - `hmac-sha256-insecure` — symmetric HMAC; not enforcement-grade (any holder
//!   of the shared secret can forge it); rejected in enforcement.
//! - `ed25519` — real per-agent ed25519 signing via [`EdSigningKey`]; accepted
//!   by enforcement-mode verifiers.
//!
//! Public keys live under `.jeryu/autonomy/keys/<agent_id>.ed25519.pub`
//! (32 bytes, hex). Private key material is vaulted by the host.

pub use jeryu_signing::{EdSigningKey, EdVerifier, Signature, SigningKey, sha256_digest};

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
        let unsigned = Signature::unsigned();
        assert!(
            !v.verify(b"x", &unsigned),
            "unsigned algo must not verify under ed25519"
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

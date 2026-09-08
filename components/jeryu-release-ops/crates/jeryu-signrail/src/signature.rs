//! Signature service abstraction for SignRail.

use crate::checksum::{hex_encode, sha256_digest, sha256_hex};
use crate::error::{Result, SignRailError};
use crate::json;
use jeryu_signing::{EdSigningKey, Signature as WireSignature};

/// Signature attached to provenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    /// Signature algorithm identifier.
    pub algorithm: String,
    /// Key identifier.
    pub key_id: String,
    /// Signature bytes encoded as lowercase hex.
    pub value_hex: String,
}

impl Signature {
    /// Render signature as JSON.
    pub fn to_json(&self) -> String {
        format!(
            "{{{},{},{}}}",
            json::field("algorithm", &self.algorithm),
            json::field("key_id", &self.key_id),
            json::field("value_hex", &self.value_hex)
        )
    }
}

/// Signing backend contract.
pub trait Signer {
    /// Stable signer identity.
    fn signer_id(&self) -> &str;

    /// Sign bytes or fail closed.
    fn sign(&self, message: &[u8]) -> Result<Signature>;

    /// Verify bytes against a signature.
    fn verify(&self, message: &[u8], signature: &Signature) -> Result<()>;
}

/// Deterministic local signer used for tests and development.
#[derive(Clone, Debug)]
pub struct HmacSha256Signer {
    key_id: String,
    secret: Vec<u8>,
}

/// Enforcement-grade Ed25519 signer for release provenance.
pub struct Ed25519Signer {
    key_id: String,
    inner: EdSigningKey,
}

impl Ed25519Signer {
    /// Create an Ed25519 signer from a 32-byte hex seed.
    pub fn from_seed_hex(key_id: Option<String>, seed_hex: &str) -> Result<Self> {
        let seed = decode_hex_exact::<32>(seed_hex)?;
        let provisional = EdSigningKey::from_seed("signrail-ed25519", seed);
        let key_id = key_id.unwrap_or_else(|| {
            let public_key = provisional.public_key_hex();
            format!(
                "signrail-ed25519:{}",
                &sha256_hex(public_key.as_bytes())[..16]
            )
        });
        Ok(Self {
            inner: EdSigningKey::from_seed(key_id.clone(), seed),
            key_id,
        })
    }

    /// Return the verifier public key as lowercase hex.
    pub fn public_key_hex(&self) -> String {
        self.inner.public_key_hex()
    }
}

impl Signer for Ed25519Signer {
    fn signer_id(&self) -> &str {
        &self.key_id
    }

    fn sign(&self, message: &[u8]) -> Result<Signature> {
        let signature = self.inner.sign_raw(message);
        Ok(Signature {
            algorithm: "JFSIG-ED25519".to_string(),
            key_id: self.key_id.clone(),
            value_hex: signature.value,
        })
    }

    fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        if signature.algorithm != "JFSIG-ED25519" {
            return Err(SignRailError::Verification(format!(
                "unsupported signature algorithm {}",
                signature.algorithm
            )));
        }
        if signature.key_id != self.key_id {
            return Err(SignRailError::Verification(format!(
                "signature key mismatch: expected {}, got {}",
                self.key_id, signature.key_id
            )));
        }
        let wire = WireSignature {
            key_id: self.key_id.clone(),
            algo: "ed25519".to_string(),
            value: signature.value_hex.clone(),
        };
        if self.inner.verifier().verify(message, &wire) {
            Ok(())
        } else {
            Err(SignRailError::Verification(
                "signature mismatch".to_string(),
            ))
        }
    }
}

impl HmacSha256Signer {
    /// Create a local HMAC-SHA256 signer.
    pub fn new(key_id: impl Into<String>, secret: impl AsRef<[u8]>) -> Self {
        Self {
            key_id: key_id.into(),
            secret: secret.as_ref().to_vec(),
        }
    }
}

impl Signer for HmacSha256Signer {
    fn signer_id(&self) -> &str {
        &self.key_id
    }

    fn sign(&self, message: &[u8]) -> Result<Signature> {
        Ok(Signature {
            algorithm: "JFSIG-HMAC-SHA256".to_string(),
            key_id: self.key_id.clone(),
            value_hex: hmac_sha256_hex(&self.secret, message),
        })
    }

    fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        if signature.algorithm != "JFSIG-HMAC-SHA256" {
            return Err(SignRailError::Verification(format!(
                "unsupported signature algorithm {}",
                signature.algorithm
            )));
        }
        if signature.key_id != self.key_id {
            return Err(SignRailError::Verification(format!(
                "signature key mismatch: expected {}, got {}",
                self.key_id, signature.key_id
            )));
        }
        let expected = hmac_sha256_hex(&self.secret, message);
        if !constant_time_eq(expected.as_bytes(), signature.value_hex.as_bytes()) {
            return Err(SignRailError::Verification(
                "signature mismatch".to_string(),
            ));
        }
        Ok(())
    }
}

/// Signer that always fails. Used to prove signing outages fail closed.
#[derive(Clone, Debug)]
pub struct UnavailableSigner {
    key_id: String,
}

impl UnavailableSigner {
    /// Create an unavailable signer.
    pub fn new(key_id: impl Into<String>) -> Self {
        Self {
            key_id: key_id.into(),
        }
    }
}

impl Signer for UnavailableSigner {
    fn signer_id(&self) -> &str {
        &self.key_id
    }

    fn sign(&self, _message: &[u8]) -> Result<Signature> {
        Err(SignRailError::SigningUnavailable(format!(
            "signer {} is unavailable",
            self.key_id
        )))
    }

    fn verify(&self, _message: &[u8], _signature: &Signature) -> Result<()> {
        Err(SignRailError::SigningUnavailable(format!(
            "signer {} is unavailable",
            self.key_id
        )))
    }
}

fn hmac_sha256_hex(key: &[u8], message: &[u8]) -> String {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        key_block[..32].copy_from_slice(&sha256_digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= key_block[i];
        opad[i] ^= key_block[i];
    }

    let mut inner = Vec::with_capacity(BLOCK + message.len());
    inner.extend_from_slice(&ipad);
    inner.extend_from_slice(message);
    let inner_hash = sha256_digest(&inner);

    let mut outer = Vec::with_capacity(BLOCK + inner_hash.len());
    outer.extend_from_slice(&opad);
    outer.extend_from_slice(&inner_hash);
    hex_encode(&sha256_digest(&outer))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

fn decode_hex_exact<const N: usize>(value: &str) -> Result<[u8; N]> {
    let trimmed = value.trim();
    if trimmed.len() != N * 2 {
        return Err(SignRailError::InvalidInput(format!(
            "Ed25519 seed must be {} hex chars",
            N * 2
        )));
    }
    let mut out = [0u8; N];
    let bytes = trimmed.as_bytes();
    for index in 0..N {
        let hi = hex_nibble(bytes[index * 2])?;
        let lo = hex_nibble(bytes[index * 2 + 1])?;
        out[index] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_nibble(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(SignRailError::InvalidInput(
            "Ed25519 seed must be lowercase or uppercase hex".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_signer_round_trips() {
        let signer = HmacSha256Signer::new("k1", b"secret");
        let sig = signer
            .sign(b"message")
            .unwrap_or_else(|err| panic!("sign failed: {err}"));
        signer
            .verify(b"message", &sig)
            .unwrap_or_else(|err| panic!("verify failed: {err}"));
        assert!(signer.verify(b"tampered", &sig).is_err());
    }

    #[test]
    fn ed25519_signer_round_trips_and_rejects_tampering() {
        let signer = Ed25519Signer::from_seed_hex(None, &"11".repeat(32))
            .unwrap_or_else(|err| panic!("ed25519 signer failed: {err}"));
        assert!(signer.signer_id().starts_with("signrail-ed25519:"));
        assert_eq!(signer.public_key_hex().len(), 64);

        let sig = signer
            .sign(b"message")
            .unwrap_or_else(|err| panic!("sign failed: {err}"));
        assert_eq!(sig.algorithm, "JFSIG-ED25519");
        signer
            .verify(b"message", &sig)
            .unwrap_or_else(|err| panic!("verify failed: {err}"));
        assert!(signer.verify(b"tampered", &sig).is_err());
    }

    #[test]
    fn ed25519_seed_must_be_32_bytes_hex() {
        let err = match Ed25519Signer::from_seed_hex(None, "abc") {
            Ok(_) => panic!("short seed unexpectedly succeeded"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("64 hex chars"));
    }
}

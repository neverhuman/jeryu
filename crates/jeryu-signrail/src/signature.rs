//! Signature service abstraction for SignRail.

use crate::checksum::{hex_encode, sha256_digest};
use crate::error::{Result, SignRailError};
use crate::json;

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
}

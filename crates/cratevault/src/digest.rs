//! Stable content digest utilities.
//!
//! Production JitForge should use BLAKE3 exactly as specified by the design.
//! This dependency-free phase slice uses a stable 256-bit internal digest label
//! (`cv1-...`) so tests and receipts remain deterministic without external crates.

use crate::error::{Result, VaultError};
use std::fmt::{Display, Formatter};
use std::io::Read;

/// A content digest string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Digest(String);

impl Digest {
    /// Parses a digest string.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if !value.starts_with("cv1-") || value.len() != 68 {
            return Err(VaultError::InvalidInput(format!(
                "digest must be cv1- plus 64 hex characters: {value}"
            )));
        }
        if !value[4..].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(VaultError::InvalidInput(format!(
                "digest contains non-hex characters: {value}"
            )));
        }
        Ok(Self(value))
    }

    /// Returns the digest string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for Digest {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Computes a deterministic 256-bit content digest.
pub fn digest_bytes(bytes: &[u8]) -> Digest {
    let mut a: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    let mut b: u128 = 0x9e37_79b9_7f4a_7c15_f39c_c060_5ced_c835;
    for (idx, byte) in bytes.iter().copied().enumerate() {
        let x = (byte as u128) + ((idx as u128) << 8);
        a ^= x;
        a = a.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
        a = a.rotate_left(17) ^ b.rotate_right(11);
        b ^= a.wrapping_add(x << 1);
        b = b.wrapping_mul(0x1000_0000_01b3);
        b = b.rotate_left(29) ^ 0xa076_1d64_78bd_642f_a076_1d64_78bd_642f;
    }
    Digest(format!("cv1-{a:032x}{b:032x}"))
}

/// Computes a digest from a reader.
pub fn digest_reader(mut reader: impl Read) -> Result<Digest> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(digest_bytes(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_deterministic() {
        assert_eq!(digest_bytes(b"abc"), digest_bytes(b"abc"));
        assert_ne!(digest_bytes(b"abc"), digest_bytes(b"abcd"));
    }
}

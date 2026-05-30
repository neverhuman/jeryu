use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{CrateVaultError, Result};

/// A lowercase BLAKE3 digest in hex form.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Digest(String);

impl Digest {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).to_hex().to_string())
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref();
        if value.len() != 64 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(CrateVaultError::InvalidDigest(value.to_owned()));
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn shard_path(&self) -> (&str, &str, &str) {
        (&self.0[0..2], &self.0[2..4], &self.0[4..])
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Digest {
    type Err = CrateVaultError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::Digest;

    #[test]
    fn digest_is_blake3_hex() {
        let digest = Digest::from_bytes(b"jitforge");
        assert_eq!(digest.as_str().len(), 64);
        assert!(Digest::parse(digest.as_str()).is_ok());
    }

    #[test]
    fn rejects_short_digest() {
        assert!(Digest::parse("abc").is_err());
    }
}

use std::fmt;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::{JeryuCacheError, Result};

/// A lowercase BLAKE3 digest in hex form.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Digest(String);

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<__D>(deserializer: __D) -> std::result::Result<Self, __D::Error>
    where
        __D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(__D::Error::custom)
    }
}

impl Digest {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes).to_hex().to_string())
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref();
        if value.len() != 64 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(JeryuCacheError::InvalidDigest(value.to_owned()));
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
    type Err = JeryuCacheError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::Digest;

    #[test]
    fn digest_is_blake3_hex() {
        let digest = Digest::from_bytes(b"jeryu");
        assert_eq!(digest.as_str().len(), 64);
        assert!(Digest::parse(digest.as_str()).is_ok());
    }

    #[test]
    fn rejects_short_digest() {
        assert!(Digest::parse("abc").is_err());
    }

    #[test]
    fn deserialization_enforces_and_normalizes_the_digest_contract() {
        for malformed in [
            "abc".to_owned(),
            "g".repeat(64),
            "0".repeat(63),
            "0".repeat(65),
        ] {
            let encoded = serde_json::to_string(&malformed).unwrap();
            assert!(serde_json::from_str::<Digest>(&encoded).is_err());
        }

        let expected = Digest::from_bytes(b"normalized");
        let uppercase = serde_json::to_string(&expected.as_str().to_ascii_uppercase()).unwrap();
        let decoded: Digest = serde_json::from_str(&uppercase).unwrap();
        assert_eq!(decoded, expected);
    }
}

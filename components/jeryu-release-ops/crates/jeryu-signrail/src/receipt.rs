//! Generic receipt helpers.

use crate::checksum::sha256_hex;
use crate::json;

/// Append-only receipt envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    /// Receipt kind.
    pub kind: String,
    /// Subject identifier.
    pub subject: String,
    /// Canonical payload JSON.
    pub payload_json: String,
    /// Payload digest.
    pub digest: String,
}

impl Receipt {
    /// Create a receipt from payload JSON.
    pub fn new(
        kind: impl Into<String>,
        subject: impl Into<String>,
        payload_json: impl Into<String>,
    ) -> Self {
        let payload_json = payload_json.into();
        let digest = sha256_hex(payload_json.as_bytes());
        Self {
            kind: kind.into(),
            subject: subject.into(),
            payload_json,
            digest,
        }
    }

    /// Render receipt JSON.
    pub fn to_json(&self) -> String {
        format!(
            "{{{},{},{},\"payload\":{}}}",
            json::field("kind", &self.kind),
            json::field("subject", &self.subject),
            json::field("digest", &self.digest),
            self.payload_json
        )
    }
}

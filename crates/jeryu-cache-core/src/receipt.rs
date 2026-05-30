use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::digest::Digest;
use crate::error::Result;
use crate::policy::{AccessDecision, CacheAction, CacheLayer};
use crate::tier::TrustTier;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CacheEvent {
    Restore,
    Read,
    Write,
    QuarantineWrite,
    Promote,
    Verify,
    Deny,
    SafeMiss,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheReceipt {
    pub receipt_id: Digest,
    pub event: CacheEvent,
    pub key_digest: Option<Digest>,
    pub object_digest: Option<Digest>,
    pub action: CacheAction,
    pub layer: CacheLayer,
    pub actor_tier: TrustTier,
    pub decision: AccessDecision,
    pub timestamp_ms: u128,
    pub parent_receipt: Option<Digest>,
}

impl CacheReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event: CacheEvent,
        key_digest: Option<Digest>,
        object_digest: Option<Digest>,
        action: CacheAction,
        layer: CacheLayer,
        actor_tier: TrustTier,
        decision: AccessDecision,
        parent_receipt: Option<Digest>,
    ) -> Result<Self> {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let material = serde_json::json!({
            "event": &event,
            "key_digest": &key_digest,
            "object_digest": &object_digest,
            "action": &action,
            "layer": &layer,
            "actor_tier": &actor_tier,
            "decision": &decision,
            "timestamp_ms": timestamp_ms,
            "parent_receipt": &parent_receipt,
        });
        let receipt_id = Digest::from_bytes(&serde_json::to_vec(&material)?);
        Ok(Self {
            receipt_id,
            event,
            key_digest,
            object_digest,
            action,
            layer,
            actor_tier,
            decision,
            timestamp_ms,
            parent_receipt,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ReceiptSink {
    root: PathBuf,
}

impl ReceiptSink {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn write(&self, receipt: &CacheReceipt) -> Result<PathBuf> {
        let path = self
            .root
            .join(format!("{}.receipt.json", receipt.receipt_id));
        fs::write(&path, serde_json::to_vec_pretty(receipt)?)?;
        Ok(path)
    }

    pub fn read(&self, receipt_id: &Digest) -> Result<CacheReceipt> {
        let path = self.root.join(format!("{}.receipt.json", receipt_id));
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::policy::AccessDecision;

    #[test]
    fn receipt_round_trips() {
        let receipt = CacheReceipt::new(
            CacheEvent::Write,
            None,
            None,
            CacheAction::Write,
            CacheLayer::L2RunnerLocalSourceBlob,
            TrustTier::T2InternalBranch,
            AccessDecision::Allow {
                reasons: vec!["ok".into()],
            },
            None,
        )
        .unwrap();
        let tmp = tempdir().unwrap();
        let sink = ReceiptSink::open(tmp.path()).unwrap();
        sink.write(&receipt).unwrap();
        let read = sink.read(&receipt.receipt_id).unwrap();
        assert_eq!(read.receipt_id, receipt.receipt_id);
    }
}

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cas::{CasObject, ContentAddressedStore};
use crate::digest::Digest;
use crate::error::{JeryuCacheError, Result};
use crate::receipt::CacheReceipt;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuarantineRecord {
    pub object: CasObject,
    pub reason: String,
    pub writer_receipt_id: Digest,
}

#[derive(Clone, Debug)]
pub struct QuarantineStore {
    root: PathBuf,
    cas: ContentAddressedStore,
}

impl QuarantineStore {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("records"))?;
        let cas = ContentAddressedStore::open(root.join("cas"))?;
        Ok(Self { root, cas })
    }

    pub fn write(
        &self,
        bytes: &[u8],
        reason: impl Into<String>,
        receipt: &CacheReceipt,
    ) -> Result<QuarantineRecord> {
        let object = self.cas.put_bytes(bytes)?;
        let record = QuarantineRecord {
            object,
            reason: reason.into(),
            writer_receipt_id: receipt.receipt_id.clone(),
        };
        fs::write(
            self.record_path(&record.object.digest),
            serde_json::to_vec_pretty(&record)?,
        )?;
        Ok(record)
    }

    pub fn promote_to(
        &self,
        record_digest: &Digest,
        target: &ContentAddressedStore,
        promotion_receipt: &CacheReceipt,
    ) -> Result<CasObject> {
        if !promotion_receipt.decision.allowed() {
            return Err(JeryuCacheError::PolicyDenied(
                "promotion receipt was not allowed".into(),
            ));
        }
        let bytes = self.cas.get_bytes(record_digest)?;
        target.put_bytes(&bytes)
    }

    pub fn record_path(&self, digest: &Digest) -> PathBuf {
        self.root.join("records").join(format!("{digest}.json"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

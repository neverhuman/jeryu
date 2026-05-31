use serde::{Deserialize, Serialize};

use crate::digest::Digest;
use crate::key::CacheKey;
use crate::policy::CacheLayer;
use crate::receipt::CacheReceipt;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheEntry {
    pub key: CacheKey,
    pub object_digest: Digest,
    pub layer: CacheLayer,
    pub size_bytes: u64,
    pub writer_receipt: CacheReceipt,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CacheManifest {
    pub entries: Vec<CacheEntry>,
}

impl CacheManifest {
    pub fn add(&mut self, entry: CacheEntry) {
        self.entries.retain(|existing| {
            existing.key.digest != entry.key.digest || existing.layer != entry.layer
        });
        self.entries.push(entry);
    }

    pub fn find(&self, key_digest: &Digest, layer: CacheLayer) -> Option<&CacheEntry> {
        self.entries
            .iter()
            .find(|entry| &entry.key.digest == key_digest && entry.layer == layer)
    }
}

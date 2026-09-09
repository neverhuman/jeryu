//! Public input/output shapes for the JeryuCache service surface.

use std::path::PathBuf;

use jeryu_cache_core::{CacheReceipt, Digest};

#[derive(Clone, Debug)]
pub struct JeryuCachePaths {
    pub cas_root: PathBuf,
    pub receipt_root: PathBuf,
    pub quarantine_root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct RestoreOutcome {
    pub hit: bool,
    pub object_digest: Option<Digest>,
    pub receipt: CacheReceipt,
}

#[derive(Clone, Debug)]
pub struct WriteOutcome {
    pub object_digest: Digest,
    pub receipt: CacheReceipt,
}

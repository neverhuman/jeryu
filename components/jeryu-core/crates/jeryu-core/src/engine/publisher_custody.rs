//! Physical identity of the Core instance admitting publisher operations.
//! These measurements neither authenticate a commissioning packet nor establish
//! external writer exclusion. Descriptor numbers and PID are incarnation-local.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::ForgeCore;
use crate::{ForgeError, Result};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublisherResourceIdentity {
    pub path: PathBuf,
    pub device: u64,
    pub inode: u64,
    pub owner: u32,
    pub group: u32,
    pub mode: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublisherHeldFileIdentity {
    pub resource: PublisherResourceIdentity,
    pub descriptor: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredPublisherCustody {
    pub database: PublisherHeldFileIdentity,
    pub storage_root: PublisherResourceIdentity,
    pub writer_leases: Vec<PublisherHeldFileIdentity>,
    pub process_id: u32,
}

impl ForgeCore {
    /// Installation-only direct-library readback. No HTTP/MCP route accepts a
    /// caller-provided measurement as actual custody. Call outside an existing
    /// coordinator operation; internal callers measure through private storage
    /// while retaining their already held authority/repository guards.
    pub fn required_publisher_custody(&self) -> Result<RequiredPublisherCustody> {
        self.with_global_mutation(|| {
            self.runtime
                .storage
                .as_ref()
                .ok_or_else(|| {
                    ForgeError::WriterUnavailable(
                        "managed durable publisher custody is absent".into(),
                    )
                })?
                .required_publisher_custody()
        })
    }
}

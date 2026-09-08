use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::bundle::{read_bundle, verify_bundle};
use crate::errors::{JeryuMirrorError, Result};
use crate::model::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleCatalog {
    pub entries: Vec<BundleCatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleCatalogEntry {
    pub bundle_id: Uuid,
    pub path: PathBuf,
    pub source: SourceDescriptor,
    pub counts: ArchiveCounts,
    pub archive_digest: String,
    pub registered_at: DateTime<Utc>,
}

impl BundleCatalog {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn register_bundle(&mut self, path: impl AsRef<Path>) -> Result<BundleCatalogEntry> {
        let path = path.as_ref();
        let verification = verify_bundle(path)?;
        if !verification.ok {
            return Err(JeryuMirrorError::invalid_bundle(
                path,
                verification.errors.join("; "),
            ));
        }
        let archive = read_bundle(path)?;
        let entry = BundleCatalogEntry {
            bundle_id: archive.archive_id,
            path: path.to_path_buf(),
            source: archive.source.clone(),
            counts: archive.counts(),
            archive_digest: archive.canonical_digest(),
            registered_at: Utc::now(),
        };
        self.entries
            .retain(|existing| existing.bundle_id != entry.bundle_id);
        self.entries.push(entry.clone());
        self.entries.sort_by_key(|entry| entry.registered_at);
        Ok(entry)
    }

    pub fn get(&self, id: Uuid) -> Option<&BundleCatalogEntry> {
        self.entries.iter().find(|entry| entry.bundle_id == id)
    }
}

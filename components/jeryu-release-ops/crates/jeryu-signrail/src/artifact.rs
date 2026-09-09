//! Release artifact metadata.

use crate::checksum::sha256_file;
use crate::error::{Result, SignRailError};
use crate::json;
use std::fs;
use std::path::{Path, PathBuf};

/// A release artifact tracked by digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    /// Stable artifact name.
    pub name: String,
    /// Source path where the artifact was observed.
    pub path: PathBuf,
    /// MIME/media type.
    pub media_type: String,
    /// SHA-256 digest in lowercase hex.
    pub digest: String,
    /// Size in bytes.
    pub size: u64,
    /// Optional mutable alias. `latest` aliases are blocked for release witness.
    pub mutable_alias: Option<String>,
}

impl Artifact {
    /// Construct artifact metadata from a local file.
    pub fn from_file(
        name: impl Into<String>,
        path: impl AsRef<Path>,
        media_type: impl Into<String>,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file() {
            return Err(SignRailError::InvalidInput(format!(
                "artifact path is not a file: {}",
                path.display()
            )));
        }
        Ok(Self {
            name: name.into(),
            digest: sha256_file(&path)?,
            size: metadata.len(),
            path,
            media_type: media_type.into(),
            mutable_alias: None,
        })
    }

    /// Add a mutable alias marker. Release policy blocks `latest` aliases.
    pub fn with_mutable_alias(mut self, alias: impl Into<String>) -> Self {
        self.mutable_alias = Some(alias.into());
        self
    }

    /// True if the artifact is a mutable latest-only asset.
    pub fn is_mutable_latest(&self) -> bool {
        self.mutable_alias.as_deref() == Some("latest")
            || self.name == "latest"
            || self.name == "latest.tar.gz"
    }

    /// Canonical JSON representation for receipts.
    pub fn to_json(&self) -> String {
        let alias = self.mutable_alias.clone().unwrap_or_default();
        format!(
            "{{{},{},{},{},{},{}}}",
            json::field("name", &self.name),
            json::field("path", &self.path.display().to_string()),
            json::field("media_type", &self.media_type),
            json::field("digest", &self.digest),
            json::number_field("size", self.size),
            json::field("mutable_alias", &alias)
        )
    }
}

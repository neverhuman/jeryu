use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{ArchiveCounts, SourceDescriptor};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedComment {
    pub id: String,
    pub author: Option<String>,
    pub body: String,
    pub created_at: Option<DateTime<Utc>>,
    pub raw_source: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewArchive {
    pub id: String,
    pub author: Option<String>,
    pub state: String,
    pub body: Option<String>,
    pub submitted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseArchive {
    pub tag_name: String,
    pub name: Option<String>,
    pub body: Option<String>,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<ReleaseAssetArchive>,
    pub created_at: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub raw_source: Option<Value>,
}

impl ReleaseArchive {
    pub fn new(tag_name: impl Into<String>) -> Self {
        Self {
            tag_name: tag_name.into(),
            name: None,
            body: None,
            draft: false,
            prerelease: false,
            assets: Vec::new(),
            created_at: None,
            published_at: None,
            raw_source: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseAssetArchive {
    pub name: String,
    pub size: Option<u64>,
    pub digest: Option<String>,
    pub content_type: Option<String>,
    pub download_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactMetadata {
    pub name: String,
    pub kind: String,
    pub size: Option<u64>,
    pub digest: Option<String>,
    pub source_url: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub retained: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebhookMigration {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
    pub active: bool,
    pub content_type: Option<String>,
    pub secret_name: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppInstallationMigration {
    pub id: String,
    pub slug: String,
    pub permissions: Vec<String>,
    pub events: Vec<String>,
    pub token_secret_name: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtectedBranchArchive {
    pub pattern: String,
    pub required_status_checks: Vec<String>,
    pub required_reviews: u32,
    pub linear_history: bool,
    pub allow_force_pushes: bool,
    pub allow_deletions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleManifest {
    pub format: String,
    pub bundle_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub source: SourceDescriptor,
    pub counts: ArchiveCounts,
    pub archive_digest: String,
    pub files: Vec<String>,
    pub restore_instructions: String,
}

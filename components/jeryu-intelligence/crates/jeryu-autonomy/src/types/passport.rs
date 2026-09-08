//! Canonical objects 6 & 7: the Merge Passport (the signed authorization to
//! perform a Git merge) and the Release Passport (the signed authorization to
//! deploy a built, provenance-attested artifact).
//!
//! `MergePassport` retains the word "merge" because it is the passport to
//! perform a Git merge.

use super::schema_tag::{MergePassportTag, ReleasePassportTag, SchemaTag};
use super::true_default;
use crate::signing::Signature;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 6. Merge Passport
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergePassport {
    #[serde(rename = "schema")]
    pub schema: SchemaTag<MergePassportTag>,
    pub id: String,
    pub verdict_id: String,
    pub repo: String,
    #[serde(rename = "pull_request")]
    pub pull_request: String,
    pub head_sha: String,
    pub target_branch: String,
    #[serde(default)]
    pub conditions: Vec<String>,
    #[serde(default = "true_default")]
    pub rebind_on_train: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_sha: Option<String>,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumed_at: Option<DateTime<Utc>>,
    pub signature: Signature,
}

// ---------------------------------------------------------------------------
// 7. Release Passport
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Container,
    RustBinary,
    WasmModule,
    Deb,
    Rpm,
    Tarball,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeployEnvironment {
    Dev,
    Staging,
    Canary,
    Prod,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleaseRollbackPlan {
    pub strategy: String,
    #[serde(default)]
    pub tested: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReleasePassport {
    #[serde(rename = "schema")]
    pub schema: SchemaTag<ReleasePassportTag>,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_id: Option<String>,
    pub artifact_digest: String,
    pub artifact_kind: ArtifactKind,
    pub sbom_digest: String,
    pub provenance_digest: String,
    pub source_sha: String,
    pub build_logs_digest: String,
    #[serde(default)]
    pub allowed_environments: Vec<DeployEnvironment>,
    pub rollback_plan: ReleaseRollbackPlan,
    pub issued_at: DateTime<Utc>,
    pub signature: Signature,
}

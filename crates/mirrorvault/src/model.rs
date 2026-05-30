use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const BUNDLE_FORMAT: &str = "jitforge.mirrorvault.bundle.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    GitHub,
    OfflineBundle,
    LocalGit,
    Mixed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    #[default]
    Private,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObjectState {
    Open,
    Closed,
    Merged,
    Draft,
    Archived,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Archive {
    pub format: String,
    pub archive_id: Uuid,
    pub generated_at: DateTime<Utc>,
    pub source: SourceDescriptor,
    pub repositories: Vec<RepositoryArchive>,
    pub warnings: Vec<String>,
}

impl Archive {
    pub fn new(source_kind: SourceKind, source_id: impl Into<String>) -> Self {
        Self {
            format: BUNDLE_FORMAT.to_string(),
            archive_id: Uuid::new_v4(),
            generated_at: Utc::now(),
            source: SourceDescriptor {
                kind: source_kind,
                id: source_id.into(),
                captured_at: Utc::now(),
            },
            repositories: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn counts(&self) -> ArchiveCounts {
        let mut counts = ArchiveCounts {
            repositories: self.repositories.len(),
            ..ArchiveCounts::default()
        };
        for repo in &self.repositories {
            counts.issues += repo.issues.len();
            counts.pull_requests += repo.pull_requests.len();
            counts.merge_requests += repo.merge_requests.len();
            counts.labels += repo.labels.len();
            counts.milestones += repo.milestones.len();
            counts.releases += repo.releases.len();
            counts.release_assets += repo
                .releases
                .iter()
                .map(|release| release.assets.len())
                .sum::<usize>();
            counts.artifacts += repo.artifacts.len();
            counts.webhooks += repo.webhooks.len();
            counts.app_installations += repo.app_installations.len();
            counts.protected_branches += repo.protected_branches.len();
        }
        counts
    }

    pub fn canonical_digest(&self) -> String {
        let bytes = serde_json::to_vec(self)
            .expect("archive serialization is infallible for derived Serialize");
        let digest = Sha256::digest(bytes);
        format!("sha256:{}", hex::encode(digest))
    }

    pub fn sort_for_determinism(&mut self) {
        self.repositories.sort_by_key(RepositoryArchive::full_name);
        for repo in &mut self.repositories {
            repo.labels.sort_by(|a, b| a.name.cmp(&b.name));
            repo.milestones.sort_by(|a, b| a.title.cmp(&b.title));
            repo.issues.sort_by_key(|issue| issue.number);
            repo.pull_requests.sort_by_key(|pr| pr.number);
            repo.merge_requests.sort_by_key(|mr| mr.iid);
            repo.releases.sort_by(|a, b| a.tag_name.cmp(&b.tag_name));
            repo.artifacts.sort_by(|a, b| a.name.cmp(&b.name));
            repo.webhooks.sort_by(|a, b| a.url.cmp(&b.url));
            repo.app_installations.sort_by(|a, b| a.slug.cmp(&b.slug));
            repo.protected_branches
                .sort_by(|a, b| a.pattern.cmp(&b.pattern));
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchiveCounts {
    pub repositories: usize,
    pub issues: usize,
    pub pull_requests: usize,
    pub merge_requests: usize,
    pub labels: usize,
    pub milestones: usize,
    pub releases: usize,
    pub release_assets: usize,
    pub artifacts: usize,
    pub webhooks: usize,
    pub app_installations: usize,
    pub protected_branches: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceDescriptor {
    pub kind: SourceKind,
    pub id: String,
    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryArchive {
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub visibility: Visibility,
    pub default_branch: String,
    pub archived: bool,
    pub disabled: bool,
    pub git: Option<GitRemoteSnapshot>,
    pub labels: Vec<LabelArchive>,
    pub milestones: Vec<MilestoneArchive>,
    pub issues: Vec<NormalizedIssue>,
    pub pull_requests: Vec<NormalizedPullRequest>,
    pub merge_requests: Vec<NormalizedMergeRequest>,
    pub releases: Vec<ReleaseArchive>,
    pub artifacts: Vec<ArtifactMetadata>,
    pub webhooks: Vec<WebhookMigration>,
    pub app_installations: Vec<AppInstallationMigration>,
    pub protected_branches: Vec<ProtectedBranchArchive>,
    pub raw_source: Option<Value>,
}

impl RepositoryArchive {
    pub fn new(owner: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            name: name.into(),
            description: None,
            visibility: Visibility::Private,
            default_branch: "main".to_string(),
            archived: false,
            disabled: false,
            git: None,
            labels: Vec::new(),
            milestones: Vec::new(),
            issues: Vec::new(),
            pull_requests: Vec::new(),
            merge_requests: Vec::new(),
            releases: Vec::new(),
            artifacts: Vec::new(),
            webhooks: Vec::new(),
            app_installations: Vec::new(),
            protected_branches: Vec::new(),
            raw_source: None,
        }
    }

    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitRemoteSnapshot {
    pub remote_url: String,
    pub head_sha: Option<String>,
    pub mirror_ref: Option<String>,
    pub object_format: String,
    pub mirrored_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LabelArchive {
    pub name: String,
    pub color: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneArchive {
    pub title: String,
    pub state: ObjectState,
    pub description: Option<String>,
    pub due_on: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedIssue {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: ObjectState,
    pub author: Option<String>,
    pub labels: Vec<String>,
    pub comments: Vec<NormalizedComment>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub raw_source: Option<Value>,
}

impl NormalizedIssue {
    pub fn new(number: u64, title: impl Into<String>, state: ObjectState) -> Self {
        Self {
            number,
            title: title.into(),
            body: None,
            state,
            author: None,
            labels: Vec::new(),
            comments: Vec::new(),
            created_at: None,
            updated_at: None,
            closed_at: None,
            raw_source: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedPullRequest {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: ObjectState,
    pub author: Option<String>,
    pub head_ref: Option<String>,
    pub base_ref: Option<String>,
    pub merge_commit_sha: Option<String>,
    pub reviews: Vec<ReviewArchive>,
    pub comments: Vec<NormalizedComment>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
    pub merged_at: Option<DateTime<Utc>>,
    pub raw_source: Option<Value>,
}

impl NormalizedPullRequest {
    pub fn new(number: u64, title: impl Into<String>, state: ObjectState) -> Self {
        Self {
            number,
            title: title.into(),
            body: None,
            state,
            author: None,
            head_ref: None,
            base_ref: None,
            merge_commit_sha: None,
            reviews: Vec::new(),
            comments: Vec::new(),
            created_at: None,
            updated_at: None,
            closed_at: None,
            merged_at: None,
            raw_source: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedMergeRequest {
    pub iid: u64,
    pub title: String,
    pub description: Option<String>,
    pub state: ObjectState,
    pub author: Option<String>,
    pub source_branch: Option<String>,
    pub target_branch: Option<String>,
    pub merge_commit_sha: Option<String>,
    pub comments: Vec<NormalizedComment>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub merged_at: Option<DateTime<Utc>>,
    pub raw_source: Option<Value>,
}

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

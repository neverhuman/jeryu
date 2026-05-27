//! Owner: Web Forge BFF — RepoService (W-B-06).
//! Proof: `cargo nextest run -p jeryu --lib repos::service`
//! Invariants:
//!   - The Phase 2 service is a thin pass-through over `GitHost`. It does not
//!     persist or cache locally — the local cache materializes in W-B-* once
//!     `web_repositories` table rows exist.
//!   - `ApiError::Upstream` is the canonical mapping for any underlying host
//!     transport/permission failure. Authentication errors map to
//!     `Upstream` as well (the BFF never re-prompts the viewer for host
//!     creds in Phase 2).


#![allow(clippy::unnecessary_sort_by)]

#![allow(clippy::stable_sort_primitive)]

use std::sync::Arc;

use chrono::Utc;
use jeryu::api::repository::{
    CreateRepositoryPreview, CreateRepositoryRequest, RepositoryFacets, RepositoryHostKind,
    RepositoryListResponse, RepositorySummary, RepositoryVisibility,
};
use jeryu::git_host::{CreateHostRepository, GitHost, GitLabClient, HostError, Page, RepoRef};

use crate::web::auth::Viewer;
use crate::web::error::ApiError;

use super::models::{RepoId, summary_from_host};

/// Filter / pagination input for `RepoService::list`.
#[derive(Debug, Clone, Default)]
pub struct RepoListQuery {
    pub host: Option<String>,
    pub owner: Option<String>,
    pub family: Option<String>,
    pub search: Option<String>,
    pub include_archived: bool,
    pub limit: u32,
    pub cursor: Option<String>,
}

/// Service wrapper around the configured GitLab host. Future phases parametrize
/// over `Vec<Arc<dyn GitHost>>` and resolve per-host; Phase 2 wires the single
/// internal GitLab adapter.
pub struct RepoService {
    host_name: String,
    gitlab: Arc<GitLabClient>,
}

impl RepoService {
    pub fn new(host_name: impl Into<String>, gitlab: Arc<GitLabClient>) -> Self {
        Self {
            host_name: host_name.into(),
            gitlab,
        }
    }

    /// List repositories visible to the authenticated principal. Phase 2:
    /// re-fetches live from GitLab; sorting/filters are best-effort
    /// client-side (the host adapter's `list_repositories` accepts an owner
    /// scope but no full-text search).
    pub async fn list(&self, query: RepoListQuery) -> Result<RepositoryListResponse, ApiError> {
        let per_page = if query.limit == 0 {
            50
        } else {
            query.limit.min(100)
        };
        let page = Page {
            per_page,
            cursor: query.cursor.clone(),
        };
        let result = GitHost::list_repositories(self.gitlab.as_ref(), query.owner.as_deref(), page)
            .await
            .map_err(host_to_api_error)?;
        let mut summaries: Vec<RepositorySummary> = result
            .items
            .into_iter()
            .map(|h| summary_from_host(&self.host_name, h))
            .collect();
        // Apply client-side filters.
        if !query.include_archived {
            summaries.retain(|s| {
                !matches!(s.health, jeryu::api::entity::HealthLevel::Critical) // crude proxy
            });
        }
        if let Some(s) = query.search.as_ref().map(|s| s.to_lowercase()) {
            summaries.retain(|sum| {
                sum.id.full_name().to_lowercase().contains(&s)
                    || sum
                        .description
                        .as_deref()
                        .map(|d| d.to_lowercase().contains(&s))
                        .unwrap_or(false)
            });
        }
        if let Some(host_filter) = query.host.as_deref() {
            summaries.retain(|s| s.id.host == host_filter);
        }
        let total = summaries.len() as u64;
        let facets = facets_from(&summaries);
        Ok(RepositoryListResponse {
            generated_at: Utc::now(),
            total,
            repositories: summaries,
            facets,
        })
    }

    /// Fetch a single repository by opaque id. Phase 2: re-fetches via
    /// `get_repository`. The id format matches `RepoId::parse`.
    pub async fn get(&self, repo_id: &str) -> Result<RepositorySummary, ApiError> {
        let parsed = RepoId::parse(repo_id)
            .ok_or_else(|| ApiError::BadRequest(format!("invalid repo_id: {repo_id}")))?;
        let repo = RepoRef {
            owner: parsed.owner.clone(),
            name: parsed.name.clone(),
        };
        let host_repo = GitHost::get_repository(self.gitlab.as_ref(), &repo)
            .await
            .map_err(host_to_api_error)?;
        Ok(summary_from_host(&self.host_name, host_repo))
    }

    /// Return up to `limit` recent repositories, sorted by `updated_at desc`.
    /// Used by `/api/v1/bootstrap` for the dashboard.
    pub async fn recent(
        &self,
        _viewer: &Viewer,
        limit: usize,
    ) -> Result<Vec<RepositorySummary>, ApiError> {
        let page = Page {
            per_page: limit.min(100) as u32,
            cursor: None,
        };
        match GitHost::list_repositories(self.gitlab.as_ref(), None, page).await {
            Ok(result) => {
                let mut summaries: Vec<RepositorySummary> = result
                    .items
                    .into_iter()
                    .map(|h| summary_from_host(&self.host_name, h))
                    .collect();
                summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
                summaries.truncate(limit);
                Ok(summaries)
            }
            Err(err) => {
                // Phase-2 bootstrap is supposed to gracefully degrade when the
                // GitLab token is missing in CI. Surface a tracing event and
                // return the empty list rather than 502'ing the bootstrap call.
                tracing::warn!(?err, "recent_repositories: host unavailable; returning []");
                Ok(Vec::new())
            }
        }
    }

    /// Compute the create-preview without writes.
    pub async fn preview_create(
        &self,
        req: &CreateRepositoryRequest,
    ) -> Result<CreateRepositoryPreview, ApiError> {
        validate_create_input(req)?;
        let mut warnings: Vec<String> = Vec::new();
        let mut initial_files: Vec<String> = Vec::new();
        if req.initialize_readme {
            initial_files.push("README.md".into());
        }
        if let Some(t) = req.gitignore_template.as_deref() {
            initial_files.push(format!(".gitignore ({t})"));
        }
        if let Some(t) = req.license_template.as_deref() {
            initial_files.push(format!("LICENSE ({t})"));
        }
        if req.host != self.host_name {
            warnings.push(format!(
                "host {} not yet configured; defaulting to {}",
                req.host, self.host_name
            ));
        }
        let mut settings: Vec<String> = Vec::new();
        settings.push(format!("visibility = {:?}", req.visibility).to_lowercase());
        if let Some(b) = req.default_branch.as_deref() {
            settings.push(format!("default_branch = {b}"));
        }
        Ok(CreateRepositoryPreview {
            normalized_name: req.name.trim().to_string(),
            target_owner: req.owner.trim().to_string(),
            visibility: req.visibility.clone(),
            initial_files,
            settings_to_apply: settings,
            side_effects: vec![format!("create project {}/{}", req.owner, req.name)],
            warnings,
        })
    }

    /// Create the repository on the host. Returns the new summary.
    pub async fn create(
        &self,
        req: CreateRepositoryRequest,
    ) -> Result<RepositorySummary, ApiError> {
        validate_create_input(&req)?;
        if req.dry_run {
            return Err(ApiError::BadRequest(
                "dry_run=true must go through /repos/preview".into(),
            ));
        }
        let visibility = visibility_str(&req.visibility);
        let input = CreateHostRepository {
            owner: req.owner.as_str(),
            name: req.name.as_str(),
            description: req.description.as_deref(),
            visibility,
            auto_init: req.initialize_readme,
            gitignore_template: req.gitignore_template.as_deref(),
            license_template: req.license_template.as_deref(),
            default_branch: req.default_branch.as_deref(),
        };
        let host_repo = GitHost::create_repository(self.gitlab.as_ref(), input)
            .await
            .map_err(host_to_api_error)?;
        Ok(summary_from_host(&self.host_name, host_repo))
    }

    pub fn host_name(&self) -> &str {
        &self.host_name
    }

    pub fn gitlab_client(&self) -> Arc<GitLabClient> {
        self.gitlab.clone()
    }
}

pub(crate) fn validate_create_input(req: &CreateRepositoryRequest) -> Result<(), ApiError> {
    if req.name.trim().is_empty() {
        return Err(ApiError::ValidationFailed("name is required".into()));
    }
    if req.owner.trim().is_empty() {
        return Err(ApiError::ValidationFailed("owner is required".into()));
    }
    if req.host.trim().is_empty() {
        return Err(ApiError::ValidationFailed("host is required".into()));
    }
    // Lightweight slug validation: GitLab project names accept letters, digits,
    // dashes, underscores, dots, plus spaces. Reject path-traversal/empty.
    for ch in req.name.chars() {
        if ch == '/' || ch == '\\' || ch == '\0' {
            return Err(ApiError::ValidationFailed(format!(
                "name contains forbidden character: {ch:?}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn visibility_str(v: &RepositoryVisibility) -> &'static str {
    match v {
        RepositoryVisibility::Public => "public",
        RepositoryVisibility::Internal => "internal",
        RepositoryVisibility::Private => "private",
    }
}

fn facets_from(summaries: &[RepositorySummary]) -> RepositoryFacets {
    let mut hosts = std::collections::BTreeSet::new();
    let mut owners = std::collections::BTreeSet::new();
    let mut families = std::collections::BTreeSet::new();
    let mut languages = std::collections::BTreeSet::new();
    for s in summaries {
        hosts.insert(s.id.host.clone());
        owners.insert(s.id.owner.clone());
        if let Some(f) = s.family.as_deref() {
            families.insert(f.to_string());
        }
        if let Some(l) = s.language.as_deref() {
            languages.insert(l.to_string());
        }
    }
    RepositoryFacets {
        hosts: hosts.into_iter().collect(),
        owners: owners.into_iter().collect(),
        families: families.into_iter().collect(),
        languages: languages.into_iter().collect(),
    }
}

/// Map a `HostError` into the public `ApiError` envelope. Phase 2:
///   - `NotImplemented` / `Permanent` / `Transient` → `Upstream`.
///   - `Auth` → `UpstreamForbidden` (the BFF's authenticated viewer is fine;
///     the host token is what's failing).
///   - `RateLimited` → `RateLimited`.
pub fn host_to_api_error(err: HostError) -> ApiError {
    match err {
        HostError::Auth => ApiError::UpstreamForbidden("host auth failed".into()),
        HostError::RateLimited { .. } => ApiError::RateLimited,
        HostError::Transient(msg) | HostError::Permanent(msg) => ApiError::Upstream(msg),
        HostError::Conflict(msg) => ApiError::Conflict(msg),
        HostError::NotImplemented => ApiError::Upstream("not implemented".into()),
    }
}

/// Map a `RepositoryHostKind` enum into the canonical lowercase host string
/// used internally (`gitlab`, `github`, `local`).
pub fn host_kind_str(kind: &RepositoryHostKind) -> &'static str {
    match kind {
        RepositoryHostKind::GitLab => "gitlab",
        RepositoryHostKind::GitHub => "github",
        RepositoryHostKind::Local => "local",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(name: &str) -> CreateRepositoryRequest {
        CreateRepositoryRequest {
            host: "gitlab".into(),
            owner: "neverhuman".into(),
            name: name.into(),
            description: None,
            visibility: RepositoryVisibility::Private,
            initialize_readme: false,
            gitignore_template: None,
            license_template: None,
            default_branch: None,
            topics: vec![],
            family: None,
            template: None,
            dry_run: false,
        }
    }

    #[test]
    fn validate_rejects_empty_name() {
        let r = req("");
        assert!(matches!(
            validate_create_input(&r),
            Err(ApiError::ValidationFailed(_))
        ));
    }

    #[test]
    fn validate_rejects_slash_in_name() {
        let mut r = req("bad/name");
        r.name = "bad/name".into();
        assert!(matches!(
            validate_create_input(&r),
            Err(ApiError::ValidationFailed(_))
        ));
    }

    #[test]
    fn visibility_str_round_trip() {
        assert_eq!(visibility_str(&RepositoryVisibility::Public), "public");
        assert_eq!(visibility_str(&RepositoryVisibility::Internal), "internal");
        assert_eq!(visibility_str(&RepositoryVisibility::Private), "private");
    }

    #[test]
    fn host_to_api_error_mappings() {
        assert!(matches!(
            host_to_api_error(HostError::Auth),
            ApiError::UpstreamForbidden(_)
        ));
        assert!(matches!(
            host_to_api_error(HostError::RateLimited { retry_after_ms: 1 }),
            ApiError::RateLimited
        ));
        assert!(matches!(
            host_to_api_error(HostError::NotImplemented),
            ApiError::Upstream(_)
        ));
    }
}

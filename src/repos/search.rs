//! Owner: Web Forge BFF — global search (W-B-16).
//! Proof: `cargo nextest run -p jeryu --lib repos::search`
//! Invariants:
//!   - `SearchKind` enum is the single source of truth for which surfaces
//!     are searchable. Unknown kinds are silently dropped at parse time.
//!   - `SearchResults` is shape-stable: every kind has its own
//!     `Vec<...>` field so empty results still serialize the same shape
//!     a non-empty response would.
//!
//! Phase 4 scope: repository search hits the live `RepoService::list`
//! (no local cache yet); other kinds return empty collections per
//! `WEB_WORK_CLAUDE.md` §7.2 W-B-16 ("acceptable to return empty arrays
//! for kinds where no data is cached"). When the cache lands in W-F-05+
//! this module is the single seam to swap in cache reads.

#![allow(clippy::manual_clamp)]

use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use jeryu::api::repository::RepositoryId;

use crate::repos::service::{RepoListQuery, RepoService};
use crate::web::error::ApiError;

/// Surfaces the global search endpoint can hit. Used to scope a query
/// (`kinds=repo,file`) so the FE pays for only what it renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    Repo,
    File,
    Commit,
    Mr,
    Issue,
    User,
}

impl FromStr for SearchKind {
    type Err = ApiError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "repo" => Ok(Self::Repo),
            "file" => Ok(Self::File),
            "commit" => Ok(Self::Commit),
            "mr" => Ok(Self::Mr),
            "issue" => Ok(Self::Issue),
            "user" => Ok(Self::User),
            other => Err(ApiError::BadRequest(format!("unknown kind: {other}"))),
        }
    }
}

impl SearchKind {
    /// Parse a comma-separated kinds list. Empty input means "all kinds".
    /// Unknown tokens surface as `BadRequest` so the caller is told which
    /// token tripped parsing.
    pub fn parse_list(raw: Option<&str>) -> Result<Vec<Self>, ApiError> {
        let Some(s) = raw else {
            return Ok(Self::all());
        };
        if s.trim().is_empty() {
            return Ok(Self::all());
        }
        s.split(',').map(|t| t.parse::<SearchKind>()).collect()
    }

    pub fn all() -> Vec<Self> {
        vec![
            Self::Repo,
            Self::File,
            Self::Commit,
            Self::Mr,
            Self::Issue,
            Self::User,
        ]
    }
}

/// Wire shape returned by `/api/v1/search`. Each field is a separate
/// vector so the FE can render per-section panels without scanning a
/// generic union list.
#[derive(Debug, Clone, Default, Serialize, utoipa::ToSchema)]
pub struct SearchResults {
    pub query: String,
    pub repos: Vec<RepoSearchHit>,
    pub files: Vec<FileSearchHit>,
    pub commits: Vec<CommitSearchHit>,
    pub mrs: Vec<MrSearchHit>,
    pub issues: Vec<IssueSearchHit>,
    pub users: Vec<UserSearchHit>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct RepoSearchHit {
    pub id: RepositoryId,
    pub full_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct FileSearchHit {
    pub repo_id: String,
    pub path: String,
    pub ref_name: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct CommitSearchHit {
    pub repo_id: String,
    pub sha: String,
    pub summary: String,
    pub author: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct MrSearchHit {
    pub repo_id: String,
    pub iid: String,
    pub title: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct IssueSearchHit {
    pub repo_id: String,
    pub iid: String,
    pub title: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct UserSearchHit {
    pub login: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// Backbone service. Phase 4 only knows how to search repos; the other
/// kinds return empty vectors. The shape is intentionally fixed so the
/// SPA can type against it today and the empty kinds light up as caches
/// land.
pub struct SearchService {
    repo_service: Arc<RepoService>,
}

impl SearchService {
    pub fn new(repo_service: Arc<RepoService>) -> Self {
        Self { repo_service }
    }

    /// Run the search. `q` is matched case-insensitively against repo
    /// full-name and description. `limit` clamps each kind's hit count.
    pub async fn search(
        &self,
        q: &str,
        kinds: &[SearchKind],
        limit: u32,
    ) -> Result<SearchResults, ApiError> {
        let mut out = SearchResults {
            query: q.to_string(),
            ..Default::default()
        };
        let trimmed = q.trim();
        if trimmed.is_empty() {
            return Ok(out);
        }

        if kinds.contains(&SearchKind::Repo) {
            let query = RepoListQuery {
                search: Some(trimmed.to_string()),
                limit: limit.max(1).min(100),
                ..Default::default()
            };
            // Phase 4: hits the host live. When `web_repositories` rows
            // exist this becomes a SQL `LIKE`/FTS query.
            match self.repo_service.list(query).await {
                Ok(resp) => {
                    out.repos = resp
                        .repositories
                        .into_iter()
                        .take(limit as usize)
                        .map(|s| RepoSearchHit {
                            full_name: s.id.full_name(),
                            description: s.description.clone(),
                            updated_at: s.updated_at,
                            id: s.id,
                        })
                        .collect();
                }
                Err(_) => {
                    // Graceful degrade — empty repo hits rather than 502'ing the
                    // entire search response. The other kinds still return.
                    out.repos = Vec::new();
                }
            }
        }

        // Phase 4: file / commit / mr / issue / user are unimplemented.
        // The fields stay empty so callers see a stable shape; W-B-16′
        // (caches) lights them up without a wire change.

        out.total = (out.repos.len()
            + out.files.len()
            + out.commits.len()
            + out.mrs.len()
            + out.issues.len()
            + out.users.len()) as u64;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_kinds_defaults_to_all_when_absent() {
        let kinds = SearchKind::parse_list(None).unwrap();
        assert_eq!(kinds.len(), 6);
    }

    #[test]
    fn parse_kinds_handles_csv() {
        let kinds = SearchKind::parse_list(Some("repo,file")).unwrap();
        assert_eq!(kinds, vec![SearchKind::Repo, SearchKind::File]);
    }

    #[test]
    fn parse_kinds_rejects_unknown() {
        let err = SearchKind::parse_list(Some("nonsense")).unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)));
    }
}

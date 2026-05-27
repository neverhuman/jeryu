//! Owner: Web Forge BFF — repos domain helper types.
//! Proof: `cargo nextest run -p jeryu --lib repos::models`
//! Invariants:
//!   - `RepoId::parse` is the single decoder for the opaque API id; every
//!     handler / service uses it so an invalid id can never reach the host
//!     adapter.

use jeryu::api::entity::{EntityKind, EntityRef, HealthLevel};
use jeryu::api::repository::{RepositoryId, RepositorySummary, RepositoryVisibility};
use jeryu::git_host::{HostRepository, RepoRef};

/// Phase-2 opaque repo id. Encoded as `host/owner/name` for ops legibility
/// while staying opaque from the client's perspective (§35.1.2 contract is
/// "stable identifier the SPA must treat as a string"; the encoding is
/// internal). Future phases swap the encoding for a UUID without touching
/// API consumers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoId {
    pub host: String,
    pub owner: String,
    pub name: String,
}

impl RepoId {
    /// Parse an opaque `repo_id` from a URL segment. Accepts `host/owner/name`
    /// or the percent-encoded equivalent. Owners that contain `/` (nested
    /// GitLab groups) require the third segment to be the project name.
    pub fn parse(raw: &str) -> Option<Self> {
        let decoded = urlencoding::decode(raw).ok()?.into_owned();
        let parts: Vec<&str> = decoded.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() < 3 {
            return None;
        }
        let host = parts[0].to_string();
        let name = parts[parts.len() - 1].to_string();
        let owner = parts[1..parts.len() - 1].join("/");
        if host.is_empty() || owner.is_empty() || name.is_empty() {
            return None;
        }
        Some(Self { host, owner, name })
    }

    /// Wire-format opaque id (matches `parse`).
    pub fn encode(&self) -> String {
        format!("{}/{}/{}", self.host, self.owner, self.name)
    }

    pub fn repo_ref(&self) -> RepoRef {
        RepoRef {
            owner: self.owner.clone(),
            name: self.name.clone(),
        }
    }
}

impl From<&RepoId> for RepositoryId {
    fn from(value: &RepoId) -> Self {
        RepositoryId {
            id: value.encode(),
            host: value.host.clone(),
            owner: value.owner.clone(),
            name: value.name.clone(),
        }
    }
}

/// Build a `RepositorySummary` from a `HostRepository`. The Phase-2 summary
/// uses placeholder counters (0) for fields the host adapter doesn't yet
/// expose — W-B-* fills those in (open MR count, failing checks, etc.).
pub fn summary_from_host(host_name: &str, h: HostRepository) -> RepositorySummary {
    let visibility = match h.visibility.as_str() {
        "public" => RepositoryVisibility::Public,
        "internal" => RepositoryVisibility::Internal,
        _ => RepositoryVisibility::Private,
    };
    let repo_id = RepoId {
        host: host_name.to_string(),
        owner: h.repo.owner.clone(),
        name: h.repo.name.clone(),
    };
    let id: RepositoryId = (&repo_id).into();
    let entity = EntityRef::new(EntityKind::Repo, id.full_name());
    RepositorySummary {
        id,
        entity,
        description: h.description,
        visibility,
        default_branch: if h.default_branch.is_empty() {
            "main".into()
        } else {
            h.default_branch
        },
        family: None,
        topics: h.topics,
        language: None,
        health: HealthLevel::Unknown,
        open_merge_requests: 0,
        failing_checks: 0,
        running_jobs: 0,
        active_agents: 0,
        blocked_agents: 0,
        updated_at: h.updated_at.unwrap_or_else(chrono::Utc::now),
        clone_http_url: h.clone_http_url,
        clone_ssh_url: h.clone_ssh_url,
        available_actions: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_id_roundtrip_simple() {
        let raw = "gitlab/neverhuman/jeryu";
        let id = RepoId::parse(raw).expect("simple slug");
        assert_eq!(id.host, "gitlab");
        assert_eq!(id.owner, "neverhuman");
        assert_eq!(id.name, "jeryu");
        assert_eq!(id.encode(), raw);
    }

    #[test]
    fn repo_id_handles_nested_groups() {
        let id = RepoId::parse("gitlab/group/subgroup/project").expect("nested");
        assert_eq!(id.host, "gitlab");
        assert_eq!(id.owner, "group/subgroup");
        assert_eq!(id.name, "project");
        assert_eq!(id.encode(), "gitlab/group/subgroup/project");
    }

    #[test]
    fn repo_id_rejects_short_input() {
        assert!(RepoId::parse("").is_none());
        assert!(RepoId::parse("only").is_none());
        assert!(RepoId::parse("only/two").is_none());
    }

    #[test]
    fn repo_id_handles_percent_encoded() {
        let id = RepoId::parse("gitlab%2Fneverhuman%2Fjeryu").expect("encoded");
        assert_eq!(id.host, "gitlab");
        assert_eq!(id.owner, "neverhuman");
        assert_eq!(id.name, "jeryu");
    }

    #[test]
    fn summary_from_host_maps_visibility() {
        let h = HostRepository {
            repo: RepoRef {
                owner: "owner".into(),
                name: "name".into(),
            },
            id: "1".into(),
            description: None,
            visibility: "internal".into(),
            default_branch: "main".into(),
            archived: false,
            topics: vec![],
            clone_http_url: None,
            clone_ssh_url: None,
            updated_at: None,
            web_url: None,
            fork: false,
            template: false,
            default_ref_sha: None,
            provider_etag: None,
        };
        let summary = summary_from_host("gitlab", h);
        assert!(matches!(summary.visibility, RepositoryVisibility::Internal));
        assert_eq!(summary.id.host, "gitlab");
        assert_eq!(summary.default_branch, "main");
    }
}

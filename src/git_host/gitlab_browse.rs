//! Owner: GitLab host adapter — W-H-02 (read) + W-H-03 (write) impl helpers.
//! Proof: `cargo nextest run -p jeryu --lib git_host::gitlab`
//! Invariants:
//!   - Every project path encoded via `encode_project_path` before being
//!     inserted into a URL (§35.1.10 path-safety).
//!   - Read endpoints surface `HostError::NotImplemented` only when the trait
//!     default impl is in use (we override every W-H-02 / W-H-03 method here).
//!
//! Layered onto `GitLabClient` (defined in `gitlab.rs`) via `#[path]` so the
//! browse/write methods live next to their request/response types without
//! inflating the main adapter file.

use base64::Engine;
use reqwest::Method;

use crate::git_host::{
    CreateHostRepository, HostBlob, HostChangedFile, HostCommit, HostCompare, HostError, HostJob,
    HostJobLog, HostPipeline, HostRef, HostRepository, HostRepositorySettingsPatch, HostTreeEntry,
    Page, PageResult, RepoRef,
};

use super::GitLabClient;
use super::gitlab_helpers::{map_error, map_reqwest};
use super::gitlab_types::{
    CreateProjectReq, GitLabBranch, GitLabCommit, GitLabCompare, GitLabCompareDiff, GitLabJobCi,
    GitLabPipelineCi, GitLabProject, GitLabRepoFile, GitLabRepoTreeEntry, GitLabTag,
    UpdateProjectReq,
};

/// URL-encode the slash-joined `owner/name` path into a single GitLab
/// `:id`-shaped path segment (§35.1.10). Mirrors `GitLabClient::project_ref`
/// — kept here so the W-H-02 helpers stay self-contained.
pub(super) fn encode_project_path(repo: &RepoRef) -> String {
    urlencoding::encode(&repo.slug()).into_owned()
}

impl GitLabClient {
    // ── W-H-02 / W-H-03: HTTP helpers that return X-Next-Page cursors ───
    //
    // The standard `get_json` / `get_paginated_json` helpers in
    // `gitlab_client.rs` either return one page or walk every page. The
    // BFF surface paginates externally (cursor lives in the API path),
    // so we provide a single-page variant that exposes the next-page
    // header.

    async fn get_page_with_cursor<T: for<'de> serde::Deserialize<'de>>(
        &self,
        path: &str,
        per_page: u32,
        cursor: Option<&str>,
    ) -> Result<(Vec<T>, Option<String>), HostError> {
        let url = self.inner.api_url(path);
        let page = cursor.unwrap_or("1");
        let joiner = if url.contains('?') { '&' } else { '?' };
        let url_with_page = format!("{url}{joiner}per_page={per_page}&page={page}");
        let resp = self
            .inner
            .authed_request_url(Method::GET, url_with_page)
            .map_err(map_error)?
            .send()
            .await
            .map_err(map_reqwest)?
            .error_for_status()
            .map_err(map_reqwest)?;
        let next_cursor = resp
            .headers()
            .get("x-next-page")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let body = resp.text().await.map_err(map_reqwest)?;
        let items: Vec<T> = serde_json::from_str(&body).map_err(|e| {
            HostError::Permanent(format!("deserialize {path}: {e}; body: {body}"))
        })?;
        Ok((items, next_cursor))
    }

    /// Convenience: walk pages for a path up to `max_pages`, gathering items.
    /// Used by `list_tree` to surface a complete one-level listing (up to 10
    /// pages × 100 = 1000 entries) without forcing the caller to paginate.
    async fn list_pages_capped<T: for<'de> serde::Deserialize<'de>>(
        &self,
        path: &str,
        per_page: u32,
        max_pages: u32,
    ) -> Result<Vec<T>, HostError> {
        let mut items: Vec<T> = Vec::new();
        let mut cursor: Option<String> = None;
        let mut pages = 0u32;
        loop {
            let (mut chunk, next) = self
                .get_page_with_cursor::<T>(path, per_page, cursor.as_deref())
                .await?;
            items.append(&mut chunk);
            pages += 1;
            cursor = next;
            if cursor.is_none() || pages >= max_pages {
                break;
            }
        }
        Ok(items)
    }

    pub(super) async fn put_json<Req: serde::Serialize, Resp: for<'de> serde::Deserialize<'de>>(
        &self,
        path: &str,
        body: &Req,
    ) -> Result<Resp, HostError> {
        let url = self.inner.api_url(path);
        let resp: Resp = self
            .inner
            .authed_request_url(Method::PUT, url)
            .map_err(map_error)?
            .json(body)
            .send()
            .await
            .map_err(map_reqwest)?
            .error_for_status()
            .map_err(map_reqwest)?
            .json()
            .await
            .map_err(map_reqwest)?;
        Ok(resp)
    }
}

// ── W-H-02 / W-H-03 trait method implementations ──────────────────────

/// Implementations are split into a second `impl GitHost for GitLabClient`
/// block; Rust accepts multiple impl blocks for the same trait/type so this
/// keeps gitlab.rs's existing block intact.
#[async_trait::async_trait]
impl GitlabBrowse for GitLabClient {
    async fn list_repositories(
        &self,
        owner: Option<&str>,
        page: Page,
    ) -> Result<PageResult<HostRepository>, HostError> {
        let path = match owner {
            Some(o) if !o.is_empty() => {
                let enc = urlencoding::encode(o);
                format!("/groups/{enc}/projects?with_shared=false")
            }
            _ => "/projects?membership=true&simple=false".to_string(),
        };
        let per_page = clamp_per_page(page.per_page);
        let (projects, next_cursor) = self
            .get_page_with_cursor::<GitLabProject>(&path, per_page, page.cursor.as_deref())
            .await?;
        let items = projects.into_iter().map(project_to_host_repository).collect();
        Ok(PageResult {
            items,
            next_cursor,
            total: None,
        })
    }

    async fn get_repository(&self, repo: &RepoRef) -> Result<HostRepository, HostError> {
        let enc = encode_project_path(repo);
        let project: GitLabProject = self.get_json(&format!("/projects/{enc}")).await?;
        Ok(project_to_host_repository(project))
    }

    async fn create_repository(
        &self,
        input: CreateHostRepository<'_>,
    ) -> Result<HostRepository, HostError> {
        // Determine if `owner` resolves to a group. We probe `/namespaces/{owner}`
        // first — GitLab returns `kind: group` or `kind: user`.
        let mut req = CreateProjectReq {
            name: input.name,
            path: Some(input.name),
            description: input.description,
            visibility: Some(input.visibility),
            default_branch: input.default_branch,
            initialize_with_readme: Some(input.auto_init),
            namespace_id: None,
        };
        if !input.owner.is_empty() {
            // Resolve namespace id (works for both user + group).
            let enc = urlencoding::encode(input.owner);
            #[derive(serde::Deserialize)]
            struct NamespaceProbe {
                id: i64,
                #[serde(default)]
                kind: String,
            }
            let probe: Result<NamespaceProbe, HostError> =
                self.get_json(&format!("/namespaces/{enc}")).await;
            if let Ok(p) = probe {
                req.namespace_id = Some(p.id);
                let _ = p.kind; // not currently used; reserved for future routing.
            }
        }
        let project: GitLabProject = self.post_json("/projects", &req).await?;
        Ok(project_to_host_repository(project))
    }

    async fn update_repository_settings(
        &self,
        repo: &RepoRef,
        patch: HostRepositorySettingsPatch<'_>,
    ) -> Result<HostRepository, HostError> {
        let enc = encode_project_path(repo);
        let body = UpdateProjectReq {
            // outer-Option None = absent; Some(None) = clear (we pass empty);
            // Some(Some(_)) = set. GitLab's update endpoint clears on "".
            description: match patch.description {
                Some(Some(s)) => Some(s),
                Some(None) => Some(""),
                None => None,
            },
            homepage: match patch.homepage {
                Some(Some(s)) => Some(s),
                Some(None) => Some(""),
                None => None,
            },
            visibility: patch.visibility,
            default_branch: patch.default_branch,
            archived: patch.archived,
        };
        let project: GitLabProject = self.put_json(&format!("/projects/{enc}"), &body).await?;
        Ok(project_to_host_repository(project))
    }

    async fn list_refs(&self, repo: &RepoRef) -> Result<Vec<HostRef>, HostError> {
        let enc = encode_project_path(repo);
        let branches = self
            .list_pages_capped::<GitLabBranch>(
                &format!("/projects/{enc}/repository/branches"),
                100,
                10,
            )
            .await?;
        let tags = self
            .list_pages_capped::<GitLabTag>(&format!("/projects/{enc}/repository/tags"), 100, 10)
            .await?;
        let mut out: Vec<HostRef> = Vec::with_capacity(branches.len() + tags.len());
        for b in branches {
            out.push(HostRef {
                name: b.name,
                sha: b.commit.id,
                kind: "branch".into(),
                protected: b.protected,
            });
        }
        for t in tags {
            out.push(HostRef {
                name: t.name,
                sha: t.commit.id,
                kind: "tag".into(),
                protected: t.protected,
            });
        }
        Ok(out)
    }

    async fn list_tree(
        &self,
        repo: &RepoRef,
        ref_name: &str,
        path: &str,
    ) -> Result<Vec<HostTreeEntry>, HostError> {
        let enc = encode_project_path(repo);
        let mut query = format!("ref={}", urlencoding::encode(ref_name));
        if !path.is_empty() {
            query.push_str(&format!("&path={}", urlencoding::encode(path)));
        }
        let entries = self
            .list_pages_capped::<GitLabRepoTreeEntry>(
                &format!("/projects/{enc}/repository/tree?{query}"),
                100,
                10,
            )
            .await?;
        Ok(entries
            .into_iter()
            .map(|e| HostTreeEntry {
                kind: match e.kind.as_str() {
                    "tree" => "tree",
                    "commit" => "submodule",
                    _ => "blob",
                }
                .to_string(),
                name: e.name,
                path: e.path,
                sha: e.id,
                size_bytes: None,
            })
            .collect())
    }

    async fn get_blob(
        &self,
        repo: &RepoRef,
        ref_name: &str,
        path: &str,
    ) -> Result<HostBlob, HostError> {
        let enc = encode_project_path(repo);
        let path_enc = urlencoding::encode(path);
        let ref_enc = urlencoding::encode(ref_name);
        let file: GitLabRepoFile = self
            .get_json(&format!(
                "/projects/{enc}/repository/files/{path_enc}?ref={ref_enc}"
            ))
            .await?;
        let bytes = if file.encoding == "base64" {
            base64::engine::general_purpose::STANDARD
                .decode(file.content.replace('\n', ""))
                .map_err(|e| HostError::Permanent(format!("base64 decode {path}: {e}")))?
        } else {
            file.content.into_bytes()
        };
        // Binary detection: scan first 8 KiB for a NUL byte.
        let probe = &bytes[..bytes.len().min(8192)];
        let is_binary = probe.contains(&0u8);
        let mime = mime_guess::from_path(&file.file_path)
            .first_raw()
            .map(|s| s.to_string());
        Ok(HostBlob {
            path: file.file_path,
            sha: file.blob_id,
            size_bytes: file.size,
            bytes,
            is_binary,
            mime,
        })
    }

    async fn get_readme(
        &self,
        repo: &RepoRef,
        ref_name: Option<&str>,
    ) -> Result<HostBlob, HostError> {
        // README lookup order per §35.1.7. Case-insensitive variants tried last.
        const CANDIDATES: &[&str] = &[
            "README.md",
            "README.markdown",
            "README.mdown",
            "README.txt",
            "readme.md",
            "Readme.md",
            "ReadMe.md",
            "README.MD",
            "readme.markdown",
            "readme.mdown",
            "readme.txt",
        ];
        let default_ref;
        let ref_to_use = match ref_name {
            Some(r) if !r.is_empty() => r,
            _ => {
                let repo_info = <Self as GitlabBrowse>::get_repository(self, repo).await?;
                default_ref = repo_info.default_branch;
                if default_ref.is_empty() {
                    "main"
                } else {
                    default_ref.as_str()
                }
            }
        };
        let mut last_err = None;
        for candidate in CANDIDATES {
            match <Self as GitlabBrowse>::get_blob(self, repo, ref_to_use, candidate).await {
                Ok(blob) => return Ok(blob),
                Err(HostError::Permanent(msg)) if msg.contains("404") => {
                    continue;
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| HostError::Permanent("readme not found".into())))
    }

    async fn compare_refs(
        &self,
        repo: &RepoRef,
        base: &str,
        head: &str,
    ) -> Result<HostCompare, HostError> {
        let enc = encode_project_path(repo);
        let base_enc = urlencoding::encode(base);
        let head_enc = urlencoding::encode(head);
        let cmp: GitLabCompare = self
            .get_json(&format!(
                "/projects/{enc}/repository/compare?from={base_enc}&to={head_enc}"
            ))
            .await?;
        let head_sha = cmp.commit.as_ref().map(|c| c.id.clone()).unwrap_or_default();
        let ahead_by = cmp.commits.len() as u32;
        let files: Vec<HostChangedFile> = cmp
            .diffs
            .into_iter()
            .map(changed_file_from_diff)
            .collect();
        Ok(HostCompare {
            base_sha: base.to_string(),
            head_sha,
            ahead_by,
            behind_by: 0,
            files,
        })
    }

    async fn list_commits(
        &self,
        repo: &RepoRef,
        ref_name: &str,
        path: Option<&str>,
        page: Page,
    ) -> Result<PageResult<HostCommit>, HostError> {
        let enc = encode_project_path(repo);
        let mut query = format!("ref_name={}", urlencoding::encode(ref_name));
        if let Some(p) = path {
            if !p.is_empty() {
                query.push_str(&format!("&path={}", urlencoding::encode(p)));
            }
        }
        let per_page = clamp_per_page(page.per_page);
        let (commits, next_cursor) = self
            .get_page_with_cursor::<GitLabCommit>(
                &format!("/projects/{enc}/repository/commits?{query}"),
                per_page,
                page.cursor.as_deref(),
            )
            .await?;
        let items = commits
            .into_iter()
            .map(|c| HostCommit {
                sha: c.id,
                author: c.author_name,
                message: c.message,
                parent_shas: c.parent_ids,
                committed_at: c.committed_date,
            })
            .collect();
        Ok(PageResult {
            items,
            next_cursor,
            total: None,
        })
    }

    // ── W-H-06: CI pipelines + jobs ──────────────────────────────────

    async fn list_pipelines(
        &self,
        repo: &RepoRef,
        ref_name: Option<&str>,
        page: Page,
    ) -> Result<PageResult<HostPipeline>, HostError> {
        let enc = encode_project_path(repo);
        let mut path = format!("/projects/{enc}/pipelines");
        if let Some(r) = ref_name {
            if !r.is_empty() {
                path.push_str(&format!("?ref={}", urlencoding::encode(r)));
            }
        }
        let per_page = clamp_per_page(page.per_page);
        let (pipelines, next_cursor) = self
            .get_page_with_cursor::<GitLabPipelineCi>(&path, per_page, page.cursor.as_deref())
            .await?;
        let items = pipelines
            .into_iter()
            .map(|p| {
                let created_at = p.created_at.unwrap_or_else(chrono::Utc::now);
                let updated_at = p.updated_at.unwrap_or(created_at);
                HostPipeline {
                    id: p.id.to_string(),
                    ref_name: p.ref_name,
                    sha: p.sha,
                    status: p.status,
                    created_at,
                    updated_at,
                    web_url: p.web_url,
                }
            })
            .collect();
        Ok(PageResult {
            items,
            next_cursor,
            total: None,
        })
    }

    async fn list_jobs(
        &self,
        repo: &RepoRef,
        pipeline_id: &str,
    ) -> Result<Vec<HostJob>, HostError> {
        let enc = encode_project_path(repo);
        let jobs = self
            .list_pages_capped::<GitLabJobCi>(
                &format!("/projects/{enc}/pipelines/{pipeline_id}/jobs"),
                100,
                10,
            )
            .await?;
        Ok(jobs
            .into_iter()
            .map(|j| {
                let duration_secs = j.duration.map(|d| d.max(0.0) as u64);
                HostJob {
                    id: j.id.to_string(),
                    name: j.name,
                    stage: j.stage,
                    status: j.status,
                    started_at: j.started_at,
                    finished_at: j.finished_at,
                    duration_secs,
                }
            })
            .collect())
    }

    async fn get_job_log(
        &self,
        repo: &RepoRef,
        job_id: &str,
    ) -> Result<HostJobLog, HostError> {
        let enc = encode_project_path(repo);
        let url = self
            .inner
            .api_url(&format!("/projects/{enc}/jobs/{job_id}/trace"));
        let resp = self
            .inner
            .authed_request_url(reqwest::Method::GET, url)
            .map_err(map_error)?
            .send()
            .await
            .map_err(map_reqwest)?
            .error_for_status()
            .map_err(map_reqwest)?;
        let body = resp.text().await.map_err(map_reqwest)?;
        let bytes = body.into_bytes();
        Ok(HostJobLog {
            bytes,
            truncated: false,
        })
    }
}

/// Sealed trait that mirrors the GitHost methods we want to dispatch through
/// the W-H-02/03 impl. Splitting into a sibling trait lets us add a second
/// `impl` block from this file without conflicting with the existing `impl
/// GitHost for GitLabClient` block in `gitlab.rs`.
///
/// At call sites we re-export the methods on `GitHost` via override impls in
/// `gitlab.rs` so callers never need to know about this trait. It only exists
/// to host the implementation code in this file.
#[async_trait::async_trait]
pub(super) trait GitlabBrowse: Send + Sync {
    async fn list_repositories(
        &self,
        owner: Option<&str>,
        page: Page,
    ) -> Result<PageResult<HostRepository>, HostError>;

    async fn get_repository(&self, repo: &RepoRef) -> Result<HostRepository, HostError>;

    async fn create_repository(
        &self,
        input: CreateHostRepository<'_>,
    ) -> Result<HostRepository, HostError>;

    async fn update_repository_settings(
        &self,
        repo: &RepoRef,
        patch: HostRepositorySettingsPatch<'_>,
    ) -> Result<HostRepository, HostError>;

    async fn list_refs(&self, repo: &RepoRef) -> Result<Vec<HostRef>, HostError>;

    async fn list_tree(
        &self,
        repo: &RepoRef,
        ref_name: &str,
        path: &str,
    ) -> Result<Vec<HostTreeEntry>, HostError>;

    async fn get_blob(
        &self,
        repo: &RepoRef,
        ref_name: &str,
        path: &str,
    ) -> Result<HostBlob, HostError>;

    async fn get_readme(
        &self,
        repo: &RepoRef,
        ref_name: Option<&str>,
    ) -> Result<HostBlob, HostError>;

    async fn compare_refs(
        &self,
        repo: &RepoRef,
        base: &str,
        head: &str,
    ) -> Result<HostCompare, HostError>;

    async fn list_commits(
        &self,
        repo: &RepoRef,
        ref_name: &str,
        path: Option<&str>,
        page: Page,
    ) -> Result<PageResult<HostCommit>, HostError>;

    // ── W-H-06: CI pipelines + jobs ──────────────────────────────────
    async fn list_pipelines(
        &self,
        repo: &RepoRef,
        ref_name: Option<&str>,
        page: Page,
    ) -> Result<PageResult<HostPipeline>, HostError>;

    async fn list_jobs(
        &self,
        repo: &RepoRef,
        pipeline_id: &str,
    ) -> Result<Vec<HostJob>, HostError>;

    async fn get_job_log(
        &self,
        repo: &RepoRef,
        job_id: &str,
    ) -> Result<HostJobLog, HostError>;
}

// ── Pure conversion helpers ──────────────────────────────────────────

pub(super) fn project_to_host_repository(p: GitLabProject) -> HostRepository {
    let (owner, name) = match p.path_with_namespace.rsplit_once('/') {
        Some((o, n)) => (o.to_string(), n.to_string()),
        None => (String::new(), p.path_with_namespace.clone()),
    };
    let updated_at = p.updated_at.or(p.last_activity_at);
    HostRepository {
        repo: RepoRef { owner, name },
        id: p.id.to_string(),
        description: p.description,
        visibility: p.visibility.unwrap_or_else(|| "private".into()),
        default_branch: p.default_branch.unwrap_or_default(),
        archived: p.archived,
        topics: if !p.topics.is_empty() {
            p.topics
        } else {
            p.tag_list
        },
        clone_http_url: p.http_url_to_repo,
        clone_ssh_url: p.ssh_url_to_repo,
        updated_at,
        web_url: p.web_url,
        fork: p.forked_from_project.is_some(),
        template: false,
        default_ref_sha: None,
        provider_etag: None,
    }
}

fn changed_file_from_diff(d: GitLabCompareDiff) -> HostChangedFile {
    let status = if d.new_file {
        "added"
    } else if d.deleted_file {
        "removed"
    } else if d.renamed_file {
        "renamed"
    } else {
        "modified"
    };
    let path = d
        .new_path
        .clone()
        .or_else(|| d.old_path.clone())
        .unwrap_or_default();
    let (additions, deletions) = count_diff_lines_local(&d.diff);
    HostChangedFile {
        path,
        status: status.to_string(),
        additions,
        deletions,
        patch: if d.diff.is_empty() { None } else { Some(d.diff) },
    }
}

fn count_diff_lines_local(diff: &str) -> (u32, u32) {
    let mut added = 0u32;
    let mut removed = 0u32;
    for line in diff.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            added = added.saturating_add(1);
        } else if line.starts_with('-') {
            removed = removed.saturating_add(1);
        }
    }
    (added, removed)
}

fn clamp_per_page(n: u32) -> u32 {
    n.clamp(1, 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_project_path_handles_nested_groups() {
        let r = RepoRef::parse("group/subgroup_proj").unwrap();
        let enc = encode_project_path(&r);
        // `/` → `%2F` so the path is safe to splice into a URL.
        assert_eq!(enc, "group%2Fsubgroup_proj");
    }

    #[test]
    fn project_to_host_repository_extracts_owner_and_name() {
        let p = GitLabProject {
            id: 42,
            name: "jeryu".into(),
            path: "jeryu".into(),
            path_with_namespace: "neverhuman/jeryu".into(),
            description: Some("control plane".into()),
            visibility: Some("internal".into()),
            default_branch: Some("main".into()),
            archived: false,
            topics: vec!["ci".into()],
            tag_list: vec![],
            http_url_to_repo: Some("https://gitlab.example/x/y.git".into()),
            ssh_url_to_repo: None,
            web_url: Some("https://gitlab.example/x/y".into()),
            forked_from_project: None,
            updated_at: None,
            last_activity_at: None,
            namespace: None,
        };
        let h = project_to_host_repository(p);
        assert_eq!(h.id, "42");
        assert_eq!(h.repo.owner, "neverhuman");
        assert_eq!(h.repo.name, "jeryu");
        assert_eq!(h.default_branch, "main");
        assert_eq!(h.visibility, "internal");
        assert!(!h.archived);
        assert_eq!(h.topics, vec!["ci".to_string()]);
    }

    #[test]
    fn changed_file_from_diff_maps_status_correctly() {
        let added = changed_file_from_diff(GitLabCompareDiff {
            old_path: None,
            new_path: Some("a.rs".into()),
            new_file: true,
            renamed_file: false,
            deleted_file: false,
            diff: "+a\n+b\n".into(),
        });
        assert_eq!(added.status, "added");
        assert_eq!(added.path, "a.rs");
        assert_eq!(added.additions, 2);

        let removed = changed_file_from_diff(GitLabCompareDiff {
            old_path: Some("b.rs".into()),
            new_path: None,
            new_file: false,
            renamed_file: false,
            deleted_file: true,
            diff: "-x\n".into(),
        });
        assert_eq!(removed.status, "removed");
        assert_eq!(removed.path, "b.rs");
        assert_eq!(removed.deletions, 1);

        let renamed = changed_file_from_diff(GitLabCompareDiff {
            old_path: Some("old.rs".into()),
            new_path: Some("new.rs".into()),
            new_file: false,
            renamed_file: true,
            deleted_file: false,
            diff: String::new(),
        });
        assert_eq!(renamed.status, "renamed");
        assert_eq!(renamed.path, "new.rs");
        assert!(renamed.patch.is_none());

        let modified = changed_file_from_diff(GitLabCompareDiff {
            old_path: Some("x.rs".into()),
            new_path: Some("x.rs".into()),
            new_file: false,
            renamed_file: false,
            deleted_file: false,
            diff: "+a\n-b\n".into(),
        });
        assert_eq!(modified.status, "modified");
        assert_eq!(modified.additions, 1);
        assert_eq!(modified.deletions, 1);
    }

    #[test]
    fn clamp_per_page_within_bounds() {
        assert_eq!(clamp_per_page(0), 1);
        assert_eq!(clamp_per_page(50), 50);
        assert_eq!(clamp_per_page(500), 100);
    }
}

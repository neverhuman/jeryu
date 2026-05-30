//! Owner: Web Forge BFF — RepoBrowserService (W-B-09 + W-B-10).
//! Proof: `cargo nextest run -p jeryu --lib repo_browser::service`
//! Invariants:
//!   - Every `path` argument is validated through `normalize_path`; the host
//!     adapter sees only safe paths (no `..`, no leading `/`, no NUL bytes
//!     or backslashes) — §35.1.10.
//!   - Markdown rendering ONLY runs when the caller opts in
//!     (`blob(..., render=Some("html"))` with a `.md` extension or
//!     `readme`).
//!   - Binary blobs surface with `is_binary = true` so the wire format
//!     ships base64 rather than text.

use std::sync::Arc;

use base64::Engine;
use chrono::Utc;

use crate::api::repo_browser::{
    BlobEncoding, BlobResponse, RefKind, RefSelectorItem, RenderedMarkdown, TreeEntry,
    TreeEntryKind,
};
use crate::api::repository::RepositoryId;
use crate::git_host::{
    GitHost, GitLabClient, HostBlame, HostCommit, HostCompare, HostError, Page, PageResult, RepoRef,
};
use crate::repo_browser::markdown::{MarkdownContext, render_markdown};

/// Service surface for the repo-browser endpoints.
///
/// Phase-2 wiring uses a single GitLab client (the BFF only speaks to
/// internal GitLab in the v1 release). Future phases swap in a registry that
/// dispatches by host scheme.
pub struct RepoBrowserService {
    gitlab: Arc<GitLabClient>,
}

impl RepoBrowserService {
    pub fn new(gitlab: Arc<GitLabClient>) -> Self {
        Self { gitlab }
    }

    pub async fn list_refs(&self, repo: &RepoRef) -> Result<Vec<RefSelectorItem>, BrowserError> {
        let refs = GitHost::list_refs(self.gitlab.as_ref(), repo)
            .await
            .map_err(BrowserError::from_host)?;
        Ok(refs
            .into_iter()
            .map(|r| RefSelectorItem {
                name: r.name,
                sha: r.sha,
                kind: match r.kind.as_str() {
                    "tag" => RefKind::Tag,
                    "commit" => RefKind::Commit,
                    _ => RefKind::Branch,
                },
                protected: r.protected,
            })
            .collect())
    }

    pub async fn tree(
        &self,
        repo: &RepoRef,
        ref_name: &str,
        path: &str,
    ) -> Result<Vec<TreeEntry>, BrowserError> {
        let safe_path = normalize_path(path)?;
        let entries = GitHost::list_tree(self.gitlab.as_ref(), repo, ref_name, &safe_path)
            .await
            .map_err(BrowserError::from_host)?;
        Ok(entries
            .into_iter()
            .map(|e| TreeEntry {
                path: e.path,
                name: e.name,
                kind: match e.kind.as_str() {
                    "tree" => TreeEntryKind::Directory,
                    "symlink" => TreeEntryKind::Symlink,
                    "submodule" => TreeEntryKind::Submodule,
                    _ => TreeEntryKind::File,
                },
                sha: e.sha,
                size_bytes: e.size_bytes,
                last_commit_sha: None,
                last_commit_message: None,
                last_commit_at: None,
            })
            .collect())
    }

    pub async fn blob(
        &self,
        repo_id: &RepositoryId,
        repo: &RepoRef,
        ref_name: &str,
        path: &str,
        render: Option<&str>,
    ) -> Result<BlobResponse, BrowserError> {
        let safe_path = normalize_path(path)?;
        let blob = GitHost::get_blob(self.gitlab.as_ref(), repo, ref_name, &safe_path)
            .await
            .map_err(BrowserError::from_host)?;
        let (text, base64, encoding) = if blob.is_binary {
            let enc = base64::engine::general_purpose::STANDARD.encode(&blob.bytes);
            (None, Some(enc), BlobEncoding::Base64)
        } else {
            let s = String::from_utf8(blob.bytes.clone()).unwrap_or_else(|_| {
                // Fallback: treat as binary if UTF-8 decoding fails after the
                // adapter said it was text — surfaces as base64 in the
                // response.
                String::new()
            });
            if s.is_empty() && !blob.bytes.is_empty() {
                let enc = base64::engine::general_purpose::STANDARD.encode(&blob.bytes);
                (None, Some(enc), BlobEncoding::Base64)
            } else {
                (Some(s), None, BlobEncoding::Utf8)
            }
        };
        let rendered_markdown =
            if render == Some("html") && is_markdown_path(&safe_path) && text.is_some() {
                Some(render_markdown_safe(
                    text.as_deref().unwrap_or(""),
                    &repo_id.id,
                    ref_name,
                    &safe_path,
                )?)
            } else {
                None
            };
        Ok(BlobResponse {
            repo: repo_id.clone(),
            path: safe_path,
            ref_name: ref_name.to_string(),
            sha: blob.sha,
            size_bytes: blob.size_bytes,
            mime: blob
                .mime
                .unwrap_or_else(|| "application/octet-stream".into()),
            encoding,
            text,
            base64,
            rendered_markdown,
            is_binary: blob.is_binary,
        })
    }

    pub async fn readme(
        &self,
        repo_id: &RepositoryId,
        repo: &RepoRef,
        ref_name: Option<&str>,
    ) -> Result<BlobResponse, BrowserError> {
        let blob = GitHost::get_readme(self.gitlab.as_ref(), repo, ref_name)
            .await
            .map_err(BrowserError::from_host)?;
        let ref_to_use = ref_name.unwrap_or("HEAD");
        let text = String::from_utf8(blob.bytes.clone())
            .map_err(|e| BrowserError::Invalid(format!("readme decode: {e}")))?;
        let rendered = render_markdown_safe(&text, &repo_id.id, ref_to_use, &blob.path)?;
        Ok(BlobResponse {
            repo: repo_id.clone(),
            path: blob.path,
            ref_name: ref_to_use.to_string(),
            sha: blob.sha,
            size_bytes: blob.size_bytes,
            mime: blob.mime.unwrap_or_else(|| "text/markdown".into()),
            encoding: BlobEncoding::Utf8,
            text: Some(text),
            base64: None,
            rendered_markdown: Some(rendered),
            is_binary: false,
        })
    }

    pub async fn compare(
        &self,
        repo: &RepoRef,
        base: &str,
        head: &str,
    ) -> Result<HostCompare, BrowserError> {
        GitHost::compare_refs(self.gitlab.as_ref(), repo, base, head)
            .await
            .map_err(BrowserError::from_host)
    }

    pub async fn commits(
        &self,
        repo: &RepoRef,
        ref_name: &str,
        path: Option<&str>,
        page: Page,
    ) -> Result<PageResult<HostCommit>, BrowserError> {
        let safe_path = match path {
            Some(p) => Some(normalize_path(p)?),
            None => None,
        };
        GitHost::list_commits(
            self.gitlab.as_ref(),
            repo,
            ref_name,
            safe_path.as_deref(),
            page,
        )
        .await
        .map_err(BrowserError::from_host)
    }

    pub async fn blame(
        &self,
        repo: &RepoRef,
        ref_name: &str,
        path: &str,
    ) -> Result<HostBlame, BrowserError> {
        let safe_path = normalize_path(path)?;
        GitHost::get_blame(self.gitlab.as_ref(), repo, ref_name, &safe_path)
            .await
            .map_err(BrowserError::from_host)
    }

    /// Raw byte download per §35.1.9 — distinct from JSON blob fetch.
    pub async fn raw(
        &self,
        repo: &RepoRef,
        ref_name: &str,
        path: &str,
    ) -> Result<(Vec<u8>, String, bool), BrowserError> {
        let safe_path = normalize_path(path)?;
        let blob = GitHost::get_blob(self.gitlab.as_ref(), repo, ref_name, &safe_path)
            .await
            .map_err(BrowserError::from_host)?;
        let mime = blob
            .mime
            .unwrap_or_else(|| "application/octet-stream".into());
        Ok((blob.bytes, mime, blob.is_binary))
    }
}

fn render_markdown_safe(
    text: &str,
    repo_id: &str,
    ref_name: &str,
    current_path: &str,
) -> Result<RenderedMarkdown, BrowserError> {
    let ctx = MarkdownContext {
        repo_id,
        ref_name,
        current_path,
    };
    render_markdown(text, &ctx).map_err(|e| BrowserError::Invalid(e.to_string()))
}

fn is_markdown_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".mdown")
}

/// Reject path-traversal, leading `/`, NUL, backslash per §35.1.10. Returns
/// the canonical (trimmed) form.
pub fn normalize_path(path: &str) -> Result<String, BrowserError> {
    if path.contains('\0') {
        return Err(BrowserError::Invalid("path contains NUL byte".into()));
    }
    if path.contains('\\') {
        return Err(BrowserError::Invalid("path contains backslash".into()));
    }
    if path.starts_with('/') {
        return Err(BrowserError::Invalid("path must be relative".into()));
    }
    for segment in path.split('/') {
        if segment == ".." {
            return Err(BrowserError::Invalid("path contains traversal".into()));
        }
    }
    Ok(path.trim_end_matches('/').to_string())
}

/// Distinct browser-service error so the REST handler can map it to the
/// canonical `ApiError` envelope without exposing host transport detail
/// directly.
#[derive(Debug)]
pub enum BrowserError {
    Invalid(String),
    NotFound(String),
    UpstreamForbidden(String),
    RateLimited,
    Upstream(String),
}

impl BrowserError {
    pub fn from_host(err: HostError) -> Self {
        match err {
            HostError::Auth => Self::UpstreamForbidden("host auth failed".into()),
            HostError::RateLimited { .. } => Self::RateLimited,
            HostError::Transient(msg) | HostError::Permanent(msg) => {
                if msg.contains("404") {
                    Self::NotFound(msg)
                } else {
                    Self::Upstream(msg)
                }
            }
            HostError::Conflict(msg) => Self::Upstream(msg),
            HostError::NotImplemented => Self::Upstream("browser adapter unavailable".into()),
        }
    }
}

// Tiny helper to keep the timestamps consistent.
pub fn now_ts() -> chrono::DateTime<chrono::Utc> {
    Utc::now()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_accepts_simple() {
        assert_eq!(normalize_path("src/lib.rs").unwrap(), "src/lib.rs");
        assert_eq!(normalize_path("").unwrap(), "");
        assert_eq!(normalize_path("a/b/").unwrap(), "a/b");
    }

    #[test]
    fn normalize_path_rejects_traversal() {
        assert!(normalize_path("../x").is_err());
        assert!(normalize_path("a/../b").is_err());
        assert!(normalize_path("/etc/passwd").is_err());
        assert!(normalize_path("a\\b").is_err());
        assert!(normalize_path("a\0b").is_err());
    }

    #[test]
    fn is_markdown_path_accepts_common_extensions() {
        assert!(is_markdown_path("README.md"));
        assert!(is_markdown_path("docs/setup.markdown"));
        assert!(is_markdown_path("a/b.mdown"));
        assert!(!is_markdown_path("foo.txt"));
        assert!(!is_markdown_path("foo.rs"));
    }

    #[test]
    fn browser_error_maps_host_404_to_not_found() {
        let err = BrowserError::from_host(HostError::Permanent("404 Project Not Found".into()));
        assert!(matches!(err, BrowserError::NotFound(_)));
    }
}

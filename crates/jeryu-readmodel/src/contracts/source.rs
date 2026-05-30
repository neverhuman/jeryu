//! Source-browsing contracts: refs, trees, blobs, and rendered markdown.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::repository::RepositoryId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum RefKind {
    Branch,
    Tag,
    Commit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RefSelectorItem {
    pub name: String,
    pub sha: String,
    pub kind: RefKind,
    pub protected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum TreeEntryKind {
    File,
    Directory,
    Symlink,
    Submodule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TreeEntry {
    pub path: String,
    pub name: String,
    pub kind: TreeEntryKind,
    pub sha: String,
    pub size_bytes: Option<u64>,
    pub last_commit_sha: Option<String>,
    pub last_commit_message: Option<String>,
    pub last_commit_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum BlobEncoding {
    Utf8,
    Base64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BlobResponse {
    pub repo: RepositoryId,
    pub path: String,
    pub ref_name: String,
    pub sha: String,
    pub size_bytes: u64,
    pub mime: String,
    pub encoding: BlobEncoding,
    pub text: Option<String>,
    pub base64: Option<String>,
    pub rendered_markdown: Option<RenderedMarkdown>,
    pub is_binary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MarkdownHeading {
    pub depth: u32,
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MarkdownLink {
    pub href: String,
    pub resolved_route: Option<String>,
    pub external: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RenderedMarkdown {
    pub html: String,
    pub toc: Vec<MarkdownHeading>,
    pub links: Vec<MarkdownLink>,
    /// `jeryu-md-renderer.v<n>` — pulldown-cmark / comrak options identity.
    pub renderer_version: String,
    /// `jeryu-md-sanitizer.v<n>` — ammonia allowlist identity (per §35.1.4).
    /// Optional for backward compatibility with v1 caches; W-B-08 will fill
    /// it in mandatorily once the sanitizer pipeline ships.
    pub sanitizer_version: Option<String>,
    pub rendered_at: String,
}

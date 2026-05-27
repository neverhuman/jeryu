//! Owner: TUI Control-Plane API + Web Forge BFF — repo browser DTOs
//! Proof: `cargo nextest run -p jeryu --lib api`
//! Invariants: API contract types only; no I/O, no domain side effects.
//!
//! Source: FINAL spec §6.2.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::repository::RepositoryId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct RefSelectorItem {
    pub name: String,
    pub sha: String,
    pub kind: RefKind,
    pub protected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub enum RefKind {
    Branch,
    Tag,
    Commit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct TreeEntry {
    pub path: String,
    pub name: String,
    pub kind: TreeEntryKind,
    pub sha: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_commit_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_commit_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_commit_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub enum TreeEntryKind {
    File,
    Directory,
    Symlink,
    Submodule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct BlobResponse {
    pub repo: RepositoryId,
    pub path: String,
    pub ref_name: String,
    pub sha: String,
    pub size_bytes: u64,
    pub mime: String,
    pub encoding: BlobEncoding,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_markdown: Option<RenderedMarkdown>,
    pub is_binary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub enum BlobEncoding {
    Utf8,
    Base64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct RenderedMarkdown {
    pub html: String,
    pub toc: Vec<MarkdownHeading>,
    pub links: Vec<MarkdownLink>,
    /// `jeryu-md-renderer.v<n>` — pulldown-cmark / comrak options identity.
    pub renderer_version: String,
    /// `jeryu-md-sanitizer.v<n>` — ammonia allowlist identity (per §35.1.4).
    /// Optional for backward compatibility with v1 caches; W-B-08 will fill
    /// it in mandatorily once the sanitizer pipeline ships.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sanitizer_version: Option<String>,
    pub rendered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct MarkdownHeading {
    pub depth: u8,
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(utoipa::ToSchema, schemars::JsonSchema, ts_rs::TS)]
#[ts(export, export_to = "../../contracts/generated/")]
pub struct MarkdownLink {
    pub href: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_route: Option<String>,
    pub external: bool,
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_kind_serializes_snake_case() {
        let json = serde_json::to_string(&RefKind::Branch).unwrap();
        assert_eq!(json, "\"branch\"");
    }

    #[test]
    fn tree_entry_kind_serializes_snake_case() {
        let json = serde_json::to_string(&TreeEntryKind::Directory).unwrap();
        assert_eq!(json, "\"directory\"");
    }

    #[test]
    fn blob_encoding_serializes_snake_case() {
        let json = serde_json::to_string(&BlobEncoding::Base64).unwrap();
        assert_eq!(json, "\"base64\"");
    }

    #[test]
    fn tree_entry_roundtrip() {
        let entry = TreeEntry {
            path: "src/lib.rs".into(),
            name: "lib.rs".into(),
            kind: TreeEntryKind::File,
            sha: "deadbeef".into(),
            size_bytes: Some(1024),
            last_commit_sha: None,
            last_commit_message: None,
            last_commit_at: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: TreeEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.path, "src/lib.rs");
        assert!(matches!(back.kind, TreeEntryKind::File));
        assert_eq!(back.size_bytes, Some(1024));
    }

    #[test]
    fn rendered_markdown_omits_optional_sanitizer_when_none() {
        let rm = RenderedMarkdown {
            html: "<p>x</p>".into(),
            toc: vec![],
            links: vec![],
            renderer_version: "jeryu-md-renderer.v1".into(),
            sanitizer_version: None,
            rendered_at: chrono::DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap(),
        };
        let json = serde_json::to_string(&rm).unwrap();
        assert!(!json.contains("sanitizer_version"));
        let back: RenderedMarkdown = serde_json::from_str(&json).unwrap();
        assert!(back.sanitizer_version.is_none());
    }
}

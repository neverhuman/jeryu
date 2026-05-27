//! Owner: Web Forge BFF — repository code-browser domain.
//! Proof: `cargo nextest run -p jeryu --lib repo_browser`
//! Invariants: Markdown rendering pipeline is the single source of truth for
//! sanitized HTML served to the SPA. Renderer and sanitizer versions are
//! tracked separately (§35.1.4) so cache invalidation is independent.
//!
//! Source: WEB_WORK_CLAUDE.md §7.2 W-B-08, §28.1, §35.1.4, §35.3.5.
//! W-B-09 / W-B-10 add the service-level surface (`RepoBrowserService`) plus
//! placeholder sibling modules that keep the FINAL §6.6 file map intact for
//! future per-host enrichment.

pub mod blame;
pub mod blob;
pub mod commits;
pub mod compare;
pub mod diff;
pub mod git_tree;
pub mod markdown;
pub mod render_cache;
pub mod service;

pub use markdown::{
    MarkdownContext, MarkdownError, RENDERER_VERSION, SANITIZER_VERSION, render_markdown,
};
pub use render_cache::{CacheKey, MarkdownCache};
pub use service::{BrowserError, RepoBrowserService, normalize_path};

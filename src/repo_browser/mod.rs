//! Owner: Web Forge BFF — repository code-browser domain.
//! Proof: `cargo nextest run -p jeryu --lib repo_browser`
//! Invariants: Markdown rendering pipeline is the single source of truth for
//! sanitized HTML served to the SPA. Renderer and sanitizer versions are
//! tracked separately (§35.1.4) so cache invalidation is independent.
//!
//! Source: WEB_WORK_CLAUDE.md §7.2 W-B-08, §28.1, §35.1.4, §35.3.5.

pub mod markdown;
pub mod render_cache;

pub use markdown::{render_markdown, MarkdownContext, MarkdownError, RENDERER_VERSION, SANITIZER_VERSION};
pub use render_cache::{CacheKey, MarkdownCache};

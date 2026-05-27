//! Owner: Web Forge BFF — host_sync stub.
//! Proof: `cargo nextest run -p jeryu --lib repos::host_sync`
//! Invariants:
//!   - Phase 2 keeps host_sync as a logged no-op. The full implementation
//!     (background tokio::spawn writing into the local DB cache) lands with
//!     W-B-* when `web_repositories` cache rows materialize.
//!
//! See `WEB_WORK_CLAUDE.md` §7.2 W-B-06.

use std::sync::Arc;
use tracing::{info, warn};

use jeryu::git_host::{GitHost, GitLabClient, Page};

/// Kick a background refresh task that fans out a `list_repositories` call
/// against the configured GitLab host. Phase 2 logs progress only; the
/// returned future resolves immediately.
pub async fn refresh_all(client: Arc<GitLabClient>) {
    let page = Page {
        per_page: 50,
        cursor: None,
    };
    match GitHost::list_repositories(client.as_ref(), None, page).await {
        Ok(result) => {
            info!(
                count = result.items.len(),
                "host_sync: list_repositories sample succeeded"
            );
        }
        Err(err) => {
            warn!(?err, "host_sync: list_repositories failed (expected in CI)");
        }
    }
}

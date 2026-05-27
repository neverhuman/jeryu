//! `WebState` — Arc bundle of services for the JeRyu Web Forge BFF.
//!
//! Phase 2 extends the Phase-1 shell with the W-B-06/W-B-07/W-B-09/W-B-10
//! services (RepoService / SettingsService / RepoBrowserService) plus a
//! shared `Arc<GitLabClient>` used by every service that talks to the host.
//! See `WEB_WORK_CLAUDE.md` §35.7 + FINAL §6.3 for the eventual full bag
//! shape.

use std::sync::Arc;

use jeryu::git_host::GitLabClient;
use jeryu::repo_browser::RepoBrowserService;
use jeryu::web_events::WebEventBus;

use crate::repos::{RepoService, SettingsService};
use crate::web::idempotency::IdempotencyStore;

#[derive(Clone)]
pub struct WebState {
    pub app_name: String,
    pub event_bus: Arc<WebEventBus>,
    pub feature_flags: WebFeatureFlagsConfig,
    pub idempotency: Arc<IdempotencyStore>,
    pub repo_service: Arc<RepoService>,
    pub browser_service: Arc<RepoBrowserService>,
    pub settings_service: Arc<SettingsService>,
    pub gitlab_client: Arc<GitLabClient>,
}

#[derive(Clone, Default)]
pub struct WebFeatureFlagsConfig {
    pub repo_create: bool,
    pub settings_write: bool,
    pub merge_write: bool,
    pub markdown_html: bool,
    pub agents: bool,
    pub mcp: bool,
}

impl WebState {
    /// Build a default Phase-2 state suitable for `jeryu web serve` boot.
    ///
    /// `JERYU_GITLAB_BASE_URL` / `JERYU_GITLAB_TOKEN` env vars wire the
    /// shared `GitLabClient`. If missing, the client is constructed with a
    /// placeholder URL — every trait call returns `HostError::Auth` (mapped
    /// to `upstream_forbidden`) or `HostError::Permanent` (mapped to
    /// `upstream_unavailable`), so the routes register and return the
    /// canonical structured error per §35.1.11.
    pub fn new_for_serve(event_bus: Arc<WebEventBus>) -> Self {
        use jeryu::gitlab_client::GitlabClient;

        let base_url = std::env::var("JERYU_GITLAB_BASE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:0".to_string());
        let token = std::env::var("JERYU_GITLAB_TOKEN").ok();
        let inner = GitlabClient::new(&base_url, token);
        let gitlab = Arc::new(GitLabClient::new(inner));
        let repo_service = Arc::new(RepoService::new("gitlab", gitlab.clone()));
        let settings_service = Arc::new(SettingsService::new("gitlab", gitlab.clone()));
        let browser_service = Arc::new(RepoBrowserService::new(gitlab.clone()));

        Self {
            app_name: "jeryu".into(),
            event_bus,
            feature_flags: WebFeatureFlagsConfig {
                markdown_html: true,
                ..Default::default()
            },
            idempotency: Arc::new(IdempotencyStore::new()),
            repo_service,
            browser_service,
            settings_service,
            gitlab_client: gitlab,
        }
    }
}

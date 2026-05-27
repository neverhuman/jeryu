//! `WebState` — Arc bundle of services for the JeRyu Web Forge BFF.
//!
//! Phase 1 keeps the bundle minimal so the shell compiles before the
//! W-B-06+ services land. Future fields (repo_service, browser_service,
//! merge_service, settings_service, action_service) plug into this struct
//! without changing the router or middleware signatures.
//!
//! See `WEB_WORK_CLAUDE.md` §35.7 and §28 for the eventual service-bag
//! shape.

use std::sync::Arc;

use jeryu::web_events::WebEventBus;

use crate::web::idempotency::IdempotencyStore;

#[derive(Clone)]
pub struct WebState {
    pub app_name: String,
    pub event_bus: Arc<WebEventBus>,
    pub feature_flags: WebFeatureFlagsConfig,
    pub idempotency: Arc<IdempotencyStore>,
    // Future: repo_service, browser_service, merge_service, settings_service,
    // action_service, audit_store, host_registry. The Phase-1 shell deliberately
    // omits these so the router/middleware compile in isolation.
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
    /// Build a default Phase-1 state suitable for `jeryu web serve` boot.
    /// `markdown_html` is on by default (W-B-08 is wired up); the other
    /// feature flags stay off until their services land.
    pub fn new_for_serve(event_bus: Arc<WebEventBus>) -> Self {
        Self {
            app_name: "jeryu".into(),
            event_bus,
            feature_flags: WebFeatureFlagsConfig {
                markdown_html: true,
                ..Default::default()
            },
            idempotency: Arc::new(IdempotencyStore::new()),
        }
    }
}

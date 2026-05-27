//! `WebState` — Arc bundle of services for the JeRyu Web Forge BFF.
//!
//! Phase 2 added the W-B-06/07/09/10 services (RepoService / SettingsService
//! / RepoBrowserService) plus a shared `Arc<GitLabClient>`. Phase 3 layered
//! on W-B-11/12/13 (MergeService / ReviewService / MergePassportService).
//! Phase 4 added a rolling [`ActivityBuffer`] for W-B-17 — every event
//! published on the [`WebEventBus`] is mirrored into this buffer via a
//! background tap so `/api/v1/activity` can return recent history without
//! requiring a live WebSocket subscription.
//! See `WEB_WORK_CLAUDE.md` §35.7 + FINAL §6.3 for the full bag shape.

use std::collections::VecDeque;
use std::sync::Arc;

use jeryu::api::websocket::WebEvent;
use jeryu::git_host::GitLabClient;
use jeryu::repo_browser::RepoBrowserService;
use jeryu::web_events::WebEventBus;
use parking_lot::Mutex;

use crate::merge::{MergePassportService, MergeService, ReviewService};
use crate::repos::{RepoService, SettingsService};
use crate::web::action_receipts::WebActionReceiptStore;
use crate::web::idempotency::IdempotencyStore;

/// Max events retained in the in-memory activity tap. Sized per
/// `WEB_WORK_CLAUDE.md` §7.2 W-B-17 ("last 500"). Older events are dropped
/// FIFO when the buffer fills.
pub const ACTIVITY_BUFFER_CAPACITY: usize = 500;

/// Rolling buffer of recent `WebEvent`s mirrored from the [`WebEventBus`].
/// Used by `GET /api/v1/activity` so the SPA's `LiveActivityDock` can render
/// recent history on first paint without waiting on a WebSocket replay.
#[derive(Debug, Default)]
pub struct ActivityBuffer {
    inner: Mutex<VecDeque<WebEvent>>,
}

impl ActivityBuffer {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(VecDeque::with_capacity(ACTIVITY_BUFFER_CAPACITY)),
        }
    }

    /// Push an event; oldest entries are evicted to keep the buffer at
    /// [`ACTIVITY_BUFFER_CAPACITY`].
    pub fn push(&self, event: WebEvent) {
        let mut guard = self.inner.lock();
        if guard.len() == ACTIVITY_BUFFER_CAPACITY {
            guard.pop_front();
        }
        guard.push_back(event);
    }

    /// Snapshot the buffer ordered newest-first. Callers may further
    /// filter / paginate.
    pub fn recent(&self) -> Vec<WebEvent> {
        let guard = self.inner.lock();
        let mut out: Vec<WebEvent> = guard.iter().cloned().collect();
        out.reverse();
        out
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
}

#[derive(Clone)]
pub struct WebState {
    pub app_name: String,
    pub event_bus: Arc<WebEventBus>,
    pub feature_flags: WebFeatureFlagsConfig,
    pub idempotency: Arc<IdempotencyStore>,
    /// Rolling receipt log for the §35.1.14 action-execute pipeline.
    /// Memory window + JSONL stamp until the engine's `sqlx` pool is
    /// plumbed through here (then this swaps for a `web_action_receipts`
    /// row writer). See `web::action_receipts` for the trade-off note.
    pub action_receipts: Arc<WebActionReceiptStore>,
    pub repo_service: Arc<RepoService>,
    pub browser_service: Arc<RepoBrowserService>,
    pub settings_service: Arc<SettingsService>,
    pub merge_service: Arc<MergeService>,
    pub review_service: Arc<ReviewService>,
    pub passport_service: Arc<MergePassportService>,
    pub gitlab_client: Arc<GitLabClient>,
    /// Rolling 500-event window populated by a tap on `event_bus`. Read by
    /// `GET /api/v1/activity` (W-B-17).
    pub activity_buffer: Arc<ActivityBuffer>,
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
        let passport_service = Arc::new(MergePassportService::new(gitlab.clone()));
        let merge_service = Arc::new(MergeService::new(
            "gitlab",
            gitlab.clone(),
            event_bus.clone(),
            passport_service.clone(),
        ));
        let review_service = Arc::new(ReviewService::new(gitlab.clone(), event_bus.clone()));
        let activity_buffer = Arc::new(ActivityBuffer::new());

        // ── W-B-17: bus tap → activity buffer ──
        // Subscribe to both priority channels and mirror everything into the
        // rolling buffer so `/api/v1/activity` can return recent history
        // without a live WS subscription. The tap absorbs `Lagged` (under
        // load some events are dropped; the buffer is best-effort).
        spawn_activity_tap(event_bus.clone(), activity_buffer.clone());

        Self {
            app_name: "jeryu".into(),
            event_bus,
            feature_flags: WebFeatureFlagsConfig {
                markdown_html: true,
                ..Default::default()
            },
            idempotency: Arc::new(IdempotencyStore::new()),
            action_receipts: Arc::new(WebActionReceiptStore::new()),
            repo_service,
            browser_service,
            settings_service,
            merge_service,
            review_service,
            passport_service,
            gitlab_client: gitlab,
            activity_buffer,
        }
    }
}

/// Spawn the W-B-17 tap that mirrors every published `WebEvent` into the
/// activity buffer. Returns immediately; the tap lives for the lifetime of
/// the broadcast channels (i.e. as long as the `WebEventBus` does).
fn spawn_activity_tap(bus: Arc<WebEventBus>, buffer: Arc<ActivityBuffer>) {
    let mut high = bus.subscribe_high();
    let mut low = bus.subscribe_low();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                evt = high.recv() => match evt {
                    Ok(event) => buffer.push(event),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                },
                evt = low.recv() => match evt {
                    Ok(event) => buffer.push(event),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                },
            }
        }
    });
}

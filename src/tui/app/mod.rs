//! Owner: Interactive TUI subsystem — application state and refresh loop
//! Proof: `cargo nextest run -p jeryu --lib tui::`
//! Invariants: UI state refreshes are bounded, non-blocking, and derived from durable control-plane state.
//!
//! First-cut module split per `TUI_RESET_PLAN_FINAL.md` U09 (claim row
//! TUI-RESET-20260526-011). The big `App` struct + `TuiStateSnapshot`
//! live in `state.rs`; small enums/data types live in `types.rs`. The
//! constructors (`App::new`, `App::new_render_only`, `App::build`) are
//! still in `app_runtime.rs` because U09 first-cut is forbidden from
//! editing that file — a follow-up unit will relocate them into
//! `builder.rs`, and likewise bundle the bare channel pairs into
//! `channels.rs::AppChannels`.

mod builder;
mod channels;
mod state;
mod types;

pub use state::{App, TuiStateSnapshot};
pub use types::{
    ActivePane, ActiveTab, DeliverySourceStatus, EvidenceViewMode, LiveLogState, LogTarget,
    NodeSummary, PendingApproval, PipelineMetrics, PipelineProgressView, ReleaseStageCard,
    ReleaseStageSnapshot, ReleaseSubPane, RunnerFeed, StageProgress, StorageBreakdown,
    TestViewMode,
};

// Imports re-exported into `super::*` for `app_runtime.rs` and its
// descendants (which depend on these symbols being in scope via
// `use super::*`).
use crate::{
    docker::DockerCtl,
    gitlab_client::GitlabClient,
    release,
    state::{JobEvent, Pool, TrackedPipeline, TuiSession}, // allowlist: TUI session import
};
use anyhow::Result;
use tokio::sync::mpsc;
use tokio::sync::watch;

const LIVE_LOG_MAX_BYTES: usize = 160_000;
const FEED_MAX_LINES: usize = 80;
const FEED_CYCLE_TICKS: u64 = 20;

// ---------------------------------------------------------------------------
// Demo data helpers (extracted)
// ---------------------------------------------------------------------------

#[cfg(any(feature = "demo-fixtures", test))]
#[path = "../app_demo.rs"]
mod app_demo;
#[cfg(any(feature = "demo-fixtures", test))]
pub(crate) use app_demo::*;

#[path = "../app_runtime.rs"]
mod app_runtime;
#[cfg(test)]
pub(crate) use app_runtime::test_app;

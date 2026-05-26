//! Owner: Interactive TUI subsystem — sync-side `App` action methods
//! Proof: `cargo nextest run -p jeryu --lib tui::`
//! Invariants: each submodule augments `impl App` with one focused capability
//! (tabs, navigation, focus, queue, runners, workflow, delivery). No
//! behaviour change relative to the pre-split `app_runtime_sync_actions.rs`;
//! every public method retains its previous signature so callers in
//! `app_runtime.rs` / `app_runtime_sync.rs` compile unchanged.
//!
//! Split per `TUI_RESET_PLAN_FINAL.md` §3.2 (claim row TUI-RESET-20260526-036).
//! Re-exports nothing — siblings only contribute new `impl App` blocks.

use super::*;

mod delivery;
mod focus;
mod navigation;
mod queue;
mod runners;
mod tabs;
mod workflow;

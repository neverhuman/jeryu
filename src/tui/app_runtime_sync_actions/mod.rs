//! Owner: Interactive TUI subsystem — sync-driven action handlers
//! Proof: `cargo nextest run -p jeryu -- tui`
//! Invariants: Action methods live on `App` and are split by real seams (tab/pane nav,
//! focus/logs, queue actions, runner feed, workflow DAG, delivery PR rotation).

#[path = "tabs.rs"]
mod tabs;

#[path = "navigation.rs"]
mod navigation;

#[path = "focus.rs"]
mod focus;

#[path = "queue.rs"]
mod queue;

#[path = "runners.rs"]
mod runners;

#[path = "workflow.rs"]
mod workflow;

#[path = "delivery.rs"]
mod delivery;

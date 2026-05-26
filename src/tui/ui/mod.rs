//! Owner: Interactive TUI subsystem — rendering logic root
//! Proof: `cargo nextest run -p jeryu -- tui::ui`
//! Invariants: Rendering redacts sensitive text and does not perform
//!             control-plane mutations directly. Pure draw from
//!             `&mut App` borrow (immutable read for most fields;
//!             scratch state on `app.focus_map` mutated for hit-testing).
//!
//! Layout: mod.rs is the entry point; draw.rs holds the top-level
//! orchestrator; overlay.rs holds the workflow inspect overlay (the only
//! function large enough to deserve its own file at first cut). Sibling
//! files `ui_chrome.rs` and `ui_panels.rs` keep their flat paths; we
//! mount them via `#[path = "../ui_chrome.rs"]` so the existing
//! `use super::*;` chains inside those files keep resolving.

// Re-export the parent-scope items that the path-mounted ui_chrome.rs and
// ui_panels.rs files reach via `use super::*;`. Keeping the imports here
// preserves the legacy split surface without modifying those sibling files.
pub(super) use crate::tui::app::{ActiveTab, App};
pub(super) use crate::tui::{
    activity,
    focus::{self, PaneId},
};
pub(super) use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span, Text},
    widgets::{
        Axis, Block, Borders, Chart, Clear, Dataset, Gauge, GraphType, List, ListItem, Paragraph,
        Wrap,
    },
};

#[path = "../ui_chrome.rs"]
pub(crate) mod ui_chrome;
#[path = "../ui_panels.rs"]
pub(crate) mod ui_panels;
pub(super) use ui_chrome::*;
pub(super) use ui_panels::*;

mod draw;
mod overlay;

pub use draw::draw;

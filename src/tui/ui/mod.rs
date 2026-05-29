//! Owner: Interactive TUI subsystem — rendering logic root
//! Proof: `cargo nextest run -p jeryu -- tui::ui`
//! Invariants: Rendering redacts sensitive text and does not perform
//!             control-plane mutations directly. Pure draw from
//!             `&mut App` borrow (immutable read for most fields;
//!             scratch state on `app.focus_map` mutated for hit-testing).
//!
//! Layout: mod.rs is the entry point; draw.rs holds the top-level orchestrator
//! that routes every tab through `flight_deck` lenses (Workflow/Repos keep their
//! own wrappers); overlay.rs holds the workflow inspect overlay; overlays.rs
//! holds the global command-palette/help overlays. The flat `ui_chrome.rs`
//! (header/footer/status helpers) is mounted via `#[path]`. The legacy
//! `ui_panels.*` per-tab panel tree was gutted by the Flight Deck cutover.

// Re-export the parent-scope items that the path-mounted ui_chrome.rs reaches
// via `use super::*;`, and that overlays.rs reuses (e.g. `short_text`).
pub(super) use crate::tui::app::{ActiveTab, App};
pub(super) use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

#[path = "../ui_chrome.rs"]
pub(crate) mod ui_chrome;
pub(super) use ui_chrome::*;

mod draw;
pub mod flight_deck;
mod overlay;
// Global cross-cutting overlays (command palette + help), relocated here when
// the Flight Deck cutover gutted the legacy `ui_panels.*` chain.
mod overlays;
pub(super) use overlays::{draw_command_palette, draw_help_overlay};

pub use draw::draw;

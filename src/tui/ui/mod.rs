//! Owner: Interactive TUI subsystem — rendering orchestrator
//! Proof: `cargo nextest run -p jeryu -- tui::ui`
//! Invariants: Rendering redacts sensitive text and does not perform control-plane mutations directly.
//! v3: Integrated theme system, VTI badges, and contextual keybindings.
//! U14 (first-cut): file-move split from src/tui/ui.rs (no behaviour changes).
//! Directory layout: `mod.rs` (entry), `layout.rs` (vertical chunks),
//! `header.rs` / `footer.rs` (chrome pass-throughs), `body.rs` (per-tab
//! dispatch), `overlay.rs` (narrow-terminal inspect modal).

// ui_chrome / ui_panels stay at src/tui/ui_*.rs (U14 follow-up work);
// re-mount here so the legacy `use super::*;` chains in those files keep
// resolving and child modules can refer to them as `super::ui_chrome::*`.
#[path = "../ui_chrome.rs"]
pub(crate) mod ui_chrome;
#[path = "../ui_panels.rs"]
pub(crate) mod ui_panels;

mod body;
mod footer;
mod header;
mod layout;
mod overlay;

// DO NOT REMOVE (U14 first-cut invariant): `ui_chrome.rs` / `ui_panels.rs`
// and their nested children open with `use super::*;` and rely on these
// names being in the parent `ui` module's namespace (visible to child
// globs as private items of the parent). When ui_chrome / ui_panels are
// eventually moved into ui/ proper (U14 follow-up), these can be folded
// into per-file imports.
#[allow(unused_imports)]
use super::app::{ActiveTab, App};
#[allow(unused_imports)]
use crate::tui::{
    activity,
    focus::{self, PaneId},
};
#[allow(unused_imports)]
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
};
#[allow(unused_imports)]
use ui_chrome::*;
#[allow(unused_imports)]
use ui_panels::*;

pub fn draw(f: &mut Frame, app: &mut App) {
    if app.focus.active.tab() != app.active_tab {
        app.maximize_logs = false;
        app.focus.set_tab(app.active_tab);
    }
    app.focus_map.clear_for_tab(app.active_tab);

    let fullscreen_activity = app.maximize_logs
        || app.focus.fullscreen == Some(crate::tui::focus::PaneId::ActivityLog(app.active_tab));

    if fullscreen_activity {
        let chunks = layout::fullscreen_layout(f.area());
        header::draw(f, app, chunks[0]);
        crate::tui::activity::draw_activity_pane(f, app, chunks[1]);
        footer::draw(f, app, chunks[2]);
        return;
    }

    let chunks = layout::standard_layout(f.area());
    header::draw(f, app, chunks[0]);
    body::draw(f, app, chunks[1]);
    crate::tui::activity::draw_activity_pane(f, app, chunks[2]);
    footer::draw(f, app, chunks[3]);

    if app.command_palette_open {
        body::draw_command_palette(f, app);
    }
    if app.help_overlay_open {
        body::draw_help_overlay(f, app);
    }
}

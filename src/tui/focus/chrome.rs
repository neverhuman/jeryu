//! Owner: Interactive TUI subsystem — pane chrome (border style + esc affordance).
//! Proof: `cargo nextest run -p jeryu --lib tui::focus::`
//! Invariants: `PaneChrome` is pure view-model — no app/state access. Border
//! colors encode focus state (`active`/`stack-prev`/`fullscreen`/`idle`).
//! The `[esc]` affordance only appears while the user is in a drilled view
//! and only on the active/previous pane (no extra clickable real estate
//! when not drilled).

use super::pane::PaneId;
use crate::tui::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
};

#[derive(Debug, Clone, Copy)]
pub struct PaneChrome {
    pub border_style: Style,
    pub show_esc: bool,
}

impl PaneChrome {
    pub fn title(self, label: &str) -> String {
        title_with_esc(label, self.show_esc)
    }
}

pub fn border_color(app: &App, pane: PaneId) -> Color {
    let is_maximized_active_log =
        app.maximize_logs && matches!(pane, PaneId::ActivityLog(tab) if tab == app.active_tab);

    if app.focus.fullscreen == Some(pane) || is_maximized_active_log {
        Color::Cyan
    } else if app.focus.active == pane {
        Color::Yellow
    } else if app.focus.stack.last().copied() == Some(pane) {
        Color::Magenta
    } else {
        Color::DarkGray
    }
}

pub fn border_style(app: &App, pane: PaneId) -> Style {
    let mut style = Style::default().fg(border_color(app, pane));
    if app.focus.active == pane || app.focus.fullscreen == Some(pane) {
        style = style.add_modifier(Modifier::BOLD);
    }
    style
}

pub fn is_active(app: &App, pane: PaneId) -> bool {
    app.focus.active == pane
        || app.focus.fullscreen == Some(pane)
        || (app.maximize_logs && matches!(pane, PaneId::ActivityLog(tab) if tab == app.active_tab))
}

pub fn should_show_esc(app: &App, pane: PaneId) -> bool {
    is_active(app, pane) || app.focus.stack.last().copied() == Some(pane)
}

pub fn should_show_drill_esc(app: &App, pane: PaneId) -> bool {
    app.focus.is_drilled() && should_show_esc(app, pane)
}

pub fn pane_chrome(app: &App, pane: PaneId) -> PaneChrome {
    PaneChrome {
        border_style: border_style(app, pane),
        show_esc: should_show_drill_esc(app, pane),
    }
}

pub fn register_pane(app: &mut App, pane: PaneId, rect: Rect) {
    app.focus_map.register(pane, rect);
}

pub fn register_esc_hotspot(app: &mut App, pane: PaneId, rect: Rect) {
    if should_show_esc(app, pane) {
        // Make the full title row clickable so the close affordance is not
        // sensitive to title placement or terminal width differences.
        if rect.width > 0 {
            let esc = Rect::new(rect.x, rect.y, rect.width, 1);
            app.focus_map.register_esc(pane, esc);
        }
    }
}

pub fn register_drill_esc_hotspot(app: &mut App, pane: PaneId, rect: Rect) {
    if should_show_drill_esc(app, pane) && rect.width > 0 {
        let esc = Rect::new(rect.x, rect.y, rect.width, 1);
        app.focus_map.register_esc(pane, esc);
    }
}

pub fn esc_label(active: bool) -> &'static str {
    if active { " [esc] " } else { "" }
}

pub fn title_with_esc(label: &str, show_esc: bool) -> String {
    format!(" {label}{} ", esc_label(show_esc))
}

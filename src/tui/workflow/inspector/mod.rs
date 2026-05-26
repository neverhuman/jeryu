//! Owner: Interactive TUI subsystem — Delivery inspector pane
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::inspector`
//! Invariants: Render-only; reads app state, never mutates it.
//!
//! Right-side detail pane for the Delivery view. Five sub-tabs:
//!   * Overview — status / kind / command / deps / timing / badges / reason
//!   * Logs — live tail from LiveLogState (per-node)
//!   * Deps — incoming + outgoing dependency lists
//!   * Evidence — capsule id, artifacts, related PR labels (stub)
//!   * Actions — context-sensitive buttons (Rerun, Open in GitLab,
//!     View capsule, Rollback for promote nodes)
//!
//! When the terminal is too narrow for a side pane, the legacy modal
//! overlay in `ui.rs` is rendered instead.

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::model::DeliverySnapshot;
use crate::tui::{app::LiveLogState, focus::PaneChrome, theme::Theme};

mod actions;
mod agent;
mod card;
mod log_tail;
mod tabs;

#[cfg(test)]
mod tests;

pub use tabs::InspectorTab;

use actions::draw_actions;
use agent::draw_agent;
use card::{draw_deps, draw_evidence, draw_overview};
use log_tail::draw_logs;
use tabs::draw_tab_strip;

/// Recommended width of the inspector pane in cols.
pub const INSPECTOR_W: u16 = 48;
/// Terminal width below which the pane collapses to a modal overlay.
pub const INSPECTOR_MIN_TERM_W: u16 = 140;

/// Render the inspector pane in `area`. The selected node is drawn from
/// `pr.snapshot` using the (phase_idx, node_idx) cursor on `nav`.
#[allow(clippy::too_many_arguments)]
pub fn draw_inspector_pane(
    f: &mut Frame,
    area: Rect,
    delivery: &DeliverySnapshot,
    nav_node_id: Option<&str>,
    tab: InspectorTab,
    live_log: &LiveLogState,
    action_message: Option<&str>,
    theme: &Theme,
) {
    draw_inspector_pane_with_chrome(
        f,
        area,
        delivery,
        nav_node_id,
        tab,
        live_log,
        action_message,
        theme,
        None,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn draw_inspector_pane_with_chrome(
    f: &mut Frame,
    area: Rect,
    delivery: &DeliverySnapshot,
    nav_node_id: Option<&str>,
    tab: InspectorTab,
    live_log: &LiveLogState,
    action_message: Option<&str>,
    theme: &Theme,
    chrome: Option<PaneChrome>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let Some(pr) = delivery.selected() else {
        let title = match chrome {
            Some(chrome) => chrome.title("Inspector"),
            None => " Inspector ".into(),
        };
        let border_style = match chrome {
            Some(chrome) => chrome.border_style,
            None => Style::default().fg(theme.border_subtle),
        };
        f.render_widget(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
            area,
        );
        return;
    };
    let node = nav_node_id.and_then(|id| pr.snapshot.node(id));

    // Header (tab strip) + content split.
    let header_h: u16 = 3;
    let header_area = Rect::new(area.x, area.y, area.width, header_h.min(area.height));
    let content_area = Rect::new(
        area.x,
        area.y + header_h,
        area.width,
        area.height.saturating_sub(header_h),
    );

    draw_tab_strip(f, header_area, pr, node, tab, theme, chrome);

    if content_area.height == 0 {
        return;
    }

    match tab {
        InspectorTab::Overview => draw_overview(f, content_area, node, theme),
        InspectorTab::Agent => draw_agent(f, content_area, node, theme),
        InspectorTab::Logs => draw_logs(f, content_area, node, live_log, theme),
        InspectorTab::Deps => draw_deps(f, content_area, &pr.snapshot, node, theme),
        InspectorTab::Evidence => draw_evidence(f, content_area, node, theme),
        InspectorTab::Actions => draw_actions(f, content_area, node, action_message, theme),
    }
}

pub(super) fn empty_block<'a>(theme: &Theme, title: &'a str) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_subtle))
}

pub(super) fn draw_placeholder(f: &mut Frame, area: Rect, msg: &str, theme: &Theme) {
    f.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(format!("  {}", msg), theme.muted())),
        ])
        .block(empty_block(theme, "")),
        area,
    );
}

pub(super) fn row<'a>(label: &str, value: &str, value_style: Style, theme: &Theme) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("  {:<10}", label), theme.muted()),
        Span::styled(value.to_string(), value_style),
    ])
}

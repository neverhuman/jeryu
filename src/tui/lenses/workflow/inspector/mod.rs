//! Workflow inspector pane for the Flight Deck lens.

mod actions;
mod agent;
mod chrome;
mod deps;
mod evidence;
mod logs;
mod overview;
mod tabs;

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    widgets::{Block, Borders},
};

pub use tabs::InspectorTab;

use crate::tui::{
    app::LiveLogState, focus::PaneChrome, lenses::workflow::model::DeliverySnapshot, theme::Theme,
};

/// Recommended width of the inspector pane in cols.
pub const INSPECTOR_W: u16 = 48;
/// Terminal width below which the pane collapses to a modal overlay.
pub const INSPECTOR_MIN_TERM_W: u16 = 140;

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

    let header_h: u16 = 3;
    let header_area = Rect::new(area.x, area.y, area.width, header_h.min(area.height));
    let content_area = Rect::new(
        area.x,
        area.y + header_h,
        area.width,
        area.height.saturating_sub(header_h),
    );

    chrome::draw_tab_strip(f, header_area, pr, node, tab, theme, chrome);
    if content_area.height == 0 {
        return;
    }

    match tab {
        InspectorTab::Overview => overview::draw_overview(f, content_area, node, theme),
        InspectorTab::Agent => agent::draw_agent(f, content_area, node, theme),
        InspectorTab::Logs => logs::draw_logs(f, content_area, node, live_log, theme),
        InspectorTab::Deps => deps::draw_deps(f, content_area, &pr.snapshot, node, theme),
        InspectorTab::Evidence => evidence::draw_evidence(f, content_area, node, theme),
        InspectorTab::Actions => {
            actions::draw_actions(f, content_area, node, action_message, theme);
        }
    }
}

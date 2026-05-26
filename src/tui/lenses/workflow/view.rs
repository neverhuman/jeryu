//! Workflow lens view orchestration.

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    widgets::{Block, Borders},
};

use super::{
    canvas,
    model::*,
    rails::{
        minimap::draw_minimap_with_chrome, mission::draw_mission_strip_with_chrome,
        phase::draw_phase_rail_with_chrome, pr::draw_pr_rail_with_chrome,
    },
    regions::{DeliveryRegions, compute_regions},
};
use crate::tui::{
    focus::{PaneChrome, title_with_esc},
    theme::Theme,
    workflow::{hit_map::DeliveryHitMap, nav::WorkflowNav},
};

#[derive(Debug, Clone, Copy, Default)]
pub struct DeliveryChrome {
    pub mission: Option<PaneChrome>,
    pub pr_rail: Option<PaneChrome>,
    pub phase_rail: Option<PaneChrome>,
    pub canvas: Option<PaneChrome>,
    pub minimap: Option<PaneChrome>,
}

/// Draw the legacy single-workflow tab: summary banner + scrollable DAG.
pub fn draw_workflow_tab(
    f: &mut Frame,
    area: Rect,
    snapshot: &WorkflowSnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
) {
    if snapshot.phases.is_empty() {
        canvas::draw_empty_state(f, area, snapshot, theme);
        return;
    }

    let banner_h = crate::tui::workflow::nav::BANNER_H.min(area.height);
    let banner_area = Rect::new(area.x, area.y, area.width, banner_h);
    canvas::draw_summary_banner(f, banner_area, snapshot, nav, theme);

    let dag_y = area.y + banner_h;
    let dag_h = area.height.saturating_sub(banner_h);
    if dag_h > 0 {
        let dag_area = Rect::new(area.x, dag_y, area.width, dag_h);
        canvas::draw_dag_canvas(f, dag_area, snapshot, nav, theme, tick);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_delivery_tab(
    f: &mut Frame,
    area: Rect,
    delivery: &DeliverySnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
    hit_map: &mut DeliveryHitMap,
) {
    draw_delivery_tab_with_chrome(
        f,
        area,
        delivery,
        nav,
        theme,
        tick,
        hit_map,
        DeliveryChrome::default(),
        crate::repo_fleet::RepoFilter::All,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn draw_delivery_tab_with_chrome(
    f: &mut Frame,
    area: Rect,
    delivery: &DeliverySnapshot,
    nav: &WorkflowNav,
    theme: &Theme,
    tick: u64,
    hit_map: &mut DeliveryHitMap,
    chrome: DeliveryChrome,
    repo_filter: crate::repo_fleet::RepoFilter<'_>,
) {
    let regions = compute_regions(area);
    hit_map.mission = visible(regions.mission);
    hit_map.pr_rail = visible(regions.pr_rail);
    hit_map.phase_rail = visible(regions.phase_rail);
    hit_map.canvas = visible(regions.canvas);
    hit_map.minimap = visible(regions.minimap);
    hit_map.cards.clear();

    if DeliveryRegions::is_visible(regions.mission) {
        draw_mission_strip_with_chrome(f, regions.mission, delivery, theme, chrome.mission);
    }
    if DeliveryRegions::is_visible(regions.pr_rail) {
        draw_pr_rail_with_chrome(
            f,
            regions.pr_rail,
            delivery,
            theme,
            chrome.pr_rail,
            repo_filter,
        );
    }
    if DeliveryRegions::is_visible(regions.phase_rail) {
        draw_phase_rail_with_chrome(f, regions.phase_rail, delivery, theme, chrome.phase_rail);
    }
    if DeliveryRegions::is_visible(regions.canvas) {
        let canvas_inner = draw_canvas_frame(f, regions.canvas, theme, chrome.canvas);
        if let Some(pr) = delivery.selected() {
            if pr.snapshot.phases.is_empty() {
                canvas::draw_empty_state(f, canvas_inner, &pr.snapshot, theme);
            } else {
                canvas::draw_dag_canvas_with_hits(
                    f,
                    canvas_inner,
                    &pr.snapshot,
                    nav,
                    theme,
                    tick,
                    hit_map,
                );
            }
        } else {
            canvas::draw_no_pr_state(f, canvas_inner, theme);
        }
    }
    if DeliveryRegions::is_visible(regions.minimap) {
        draw_minimap_with_chrome(f, regions.minimap, delivery, nav, theme, chrome.minimap);
    }
    if DeliveryRegions::is_visible(regions.footer) {
        canvas::draw_delivery_footer(f, regions.footer, delivery, theme);
    }
}

fn draw_canvas_frame(f: &mut Frame, area: Rect, theme: &Theme, chrome: Option<PaneChrome>) -> Rect {
    let title = match chrome {
        Some(chrome) => chrome.title("Canvas"),
        None => title_with_esc("Canvas", false),
    };
    let border_style = match chrome {
        Some(chrome) => chrome.border_style,
        None => Style::default().fg(theme.border_subtle),
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}

fn visible(rect: Rect) -> Option<Rect> {
    if rect.width == 0 || rect.height == 0 {
        None
    } else {
        Some(rect)
    }
}

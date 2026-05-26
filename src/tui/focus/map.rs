//! Owner: Interactive TUI subsystem — focus pane rect map and hit-test.
//! Proof: `cargo nextest run -p jeryu --lib tui::focus::`
//! Invariants: `FocusMap.panes` records rects in registration (paint) order
//! so `pane_at` returns the topmost overlapping pane via reverse-scan. Zero-
//! area rects are ignored. `neighbor` finds the nearest pane in a direction
//! using axis-overlap as a gate and weighted Manhattan distance as score.

use super::pane::{NavDirection, PaneId};
use crate::tui::app::ActiveTab;
use ratatui::layout::Rect;

#[derive(Debug, Clone)]
pub struct FocusPane {
    pub id: PaneId,
    pub rect: Rect,
}

#[derive(Debug, Clone)]
pub struct FocusMap {
    pub tab: ActiveTab,
    pub panes: Vec<FocusPane>,
    pub esc_targets: Vec<(PaneId, Rect)>,
}

impl Default for FocusMap {
    fn default() -> Self {
        Self {
            tab: ActiveTab::Workflow,
            panes: Vec::new(),
            esc_targets: Vec::new(),
        }
    }
}

impl FocusMap {
    pub fn clear_for_tab(&mut self, tab: ActiveTab) {
        self.tab = tab;
        self.panes.clear();
        self.esc_targets.clear();
    }

    pub fn register(&mut self, id: PaneId, rect: Rect) {
        if rect.width > 0 && rect.height > 0 {
            self.panes.push(FocusPane { id, rect });
        }
    }

    pub fn register_esc(&mut self, id: PaneId, rect: Rect) {
        if rect.width > 0 && rect.height > 0 {
            self.esc_targets.push((id, rect));
        }
    }

    pub fn pane_at(&self, x: u16, y: u16) -> Option<PaneId> {
        self.panes
            .iter()
            .rev()
            .find(|pane| rect_contains(pane.rect, x, y))
            .map(|pane| pane.id)
    }

    pub fn esc_at(&self, x: u16, y: u16) -> Option<PaneId> {
        self.esc_targets
            .iter()
            .rev()
            .find(|(_, rect)| rect_contains(*rect, x, y))
            .map(|(id, _)| *id)
    }

    pub fn rect_of(&self, id: PaneId) -> Option<Rect> {
        self.panes
            .iter()
            .rev()
            .find(|pane| pane.id == id)
            .map(|pane| pane.rect)
    }

    pub fn first_visible(&self) -> Option<PaneId> {
        self.panes.first().map(|pane| pane.id)
    }

    pub fn neighbor(&self, from: PaneId, direction: NavDirection) -> Option<PaneId> {
        let origin = self.rect_of(from)?;
        let ox = center_x(origin);
        let oy = center_y(origin);

        self.panes
            .iter()
            .filter(|pane| pane.id != from)
            .filter_map(|pane| {
                let cx = center_x(pane.rect);
                let cy = center_y(pane.rect);
                let axis_overlaps = match direction {
                    NavDirection::Left | NavDirection::Right => {
                        ranges_overlap(origin.y, origin.height, pane.rect.y, pane.rect.height)
                    }
                    NavDirection::Up | NavDirection::Down => {
                        ranges_overlap(origin.x, origin.width, pane.rect.x, pane.rect.width)
                    }
                };
                if !axis_overlaps {
                    return None;
                }
                let directional_ok = match direction {
                    NavDirection::Left => cx < ox,
                    NavDirection::Right => cx > ox,
                    NavDirection::Up => cy < oy,
                    NavDirection::Down => cy > oy,
                };
                if !directional_ok {
                    return None;
                }
                let dx = (cx as i32 - ox as i32).abs();
                let dy = (cy as i32 - oy as i32).abs();
                let score = match direction {
                    NavDirection::Left | NavDirection::Right => dx * 4 + dy,
                    NavDirection::Up | NavDirection::Down => dy * 4 + dx,
                };
                Some((score, pane.id))
            })
            .min_by_key(|(score, _)| *score)
            .map(|(_, id)| id)
    }
}

fn center_x(rect: Rect) -> u16 {
    rect.x + rect.width.saturating_div(2)
}

fn center_y(rect: Rect) -> u16 {
    rect.y + rect.height.saturating_div(2)
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

fn ranges_overlap(a_start: u16, a_len: u16, b_start: u16, b_len: u16) -> bool {
    let a_end = a_start.saturating_add(a_len);
    let b_end = b_start.saturating_add(b_len);
    a_start < b_end && b_start < a_end
}

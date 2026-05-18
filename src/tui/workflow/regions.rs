//! Owner: Interactive TUI subsystem — Delivery view region layout
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::regions`
//! Invariants: Layout is a pure function of the available area.
//!
//! Splits the Delivery tab area into named zones. Each zone collapses to
//! zero-area at narrow terminal widths so the canvas always has room.

use ratatui::layout::Rect;

/// Concrete widths for each chrome region.
pub const PHASE_RAIL_W: u16 = 13;
pub const MINIMAP_W: u16 = 14;

/// Vertical heights.
pub const MISSION_H: u16 = 3;
pub const PR_RAIL_H: u16 = 3;
pub const FOOTER_H: u16 = 1;

/// Above this terminal width the phase rail is rendered.
pub const SHOW_PHASE_RAIL_AT: u16 = 120;
/// Above this width the minimap is rendered.
pub const SHOW_MINIMAP_AT: u16 = 160;
/// Above this width the PR rail strip is rendered.
pub const SHOW_PR_RAIL_AT: u16 = 80;

/// Computed region rectangles. Any region with zero width or height is
/// considered hidden and should not be rendered.
#[derive(Debug, Clone, Copy)]
pub struct DeliveryRegions {
    pub mission: Rect,
    pub pr_rail: Rect,
    pub phase_rail: Rect,
    pub canvas: Rect,
    pub minimap: Rect,
    pub footer: Rect,
}

impl DeliveryRegions {
    pub fn is_visible(rect: Rect) -> bool {
        rect.width > 0 && rect.height > 0
    }
}

/// Lay out the Delivery tab regions inside `area`.
pub fn compute_regions(area: Rect) -> DeliveryRegions {
    // Vertical stack: mission strip → optional PR rail → middle row → footer.
    let mission_h = MISSION_H.min(area.height);
    let footer_h = FOOTER_H.min(area.height.saturating_sub(mission_h));
    let pr_rail_h = if area.width >= SHOW_PR_RAIL_AT {
        PR_RAIL_H.min(area.height.saturating_sub(mission_h + footer_h))
    } else {
        0
    };

    let mission = Rect::new(area.x, area.y, area.width, mission_h);
    let pr_rail = Rect::new(area.x, area.y + mission_h, area.width, pr_rail_h);

    let middle_y = area.y + mission_h + pr_rail_h;
    let middle_h = area.height.saturating_sub(mission_h + pr_rail_h + footer_h);

    // Horizontal split of the middle row.
    let phase_rail_w = if area.width >= SHOW_PHASE_RAIL_AT {
        PHASE_RAIL_W.min(area.width)
    } else {
        0
    };
    let minimap_w = if area.width >= SHOW_MINIMAP_AT {
        MINIMAP_W.min(area.width.saturating_sub(phase_rail_w))
    } else {
        0
    };
    let canvas_w = area.width.saturating_sub(phase_rail_w + minimap_w);

    let phase_rail = Rect::new(area.x, middle_y, phase_rail_w, middle_h);
    let canvas = Rect::new(area.x + phase_rail_w, middle_y, canvas_w, middle_h);
    let minimap = Rect::new(
        area.x + phase_rail_w + canvas_w,
        middle_y,
        minimap_w,
        middle_h,
    );

    let footer = Rect::new(
        area.x,
        area.y + area.height - footer_h,
        area.width,
        footer_h,
    );

    DeliveryRegions {
        mission,
        pr_rail,
        phase_rail,
        canvas,
        minimap,
        footer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_layout_has_all_regions() {
        let r = compute_regions(Rect::new(0, 0, 200, 60));
        assert!(DeliveryRegions::is_visible(r.mission));
        assert!(DeliveryRegions::is_visible(r.pr_rail));
        assert!(DeliveryRegions::is_visible(r.phase_rail));
        assert!(DeliveryRegions::is_visible(r.canvas));
        assert!(DeliveryRegions::is_visible(r.minimap));
        assert!(DeliveryRegions::is_visible(r.footer));
        // Canvas claims the remainder of the middle row width.
        assert_eq!(
            r.canvas.width,
            200 - PHASE_RAIL_W - MINIMAP_W,
            "canvas should fill the middle row"
        );
    }

    #[test]
    fn narrow_layout_collapses_minimap_then_phase_rail() {
        let medium = compute_regions(Rect::new(0, 0, 130, 40));
        assert!(DeliveryRegions::is_visible(medium.phase_rail));
        assert!(!DeliveryRegions::is_visible(medium.minimap));

        let small = compute_regions(Rect::new(0, 0, 100, 40));
        assert!(!DeliveryRegions::is_visible(small.phase_rail));
        assert!(!DeliveryRegions::is_visible(small.minimap));
        assert!(DeliveryRegions::is_visible(small.pr_rail));
    }

    #[test]
    fn very_narrow_layout_drops_pr_rail() {
        let tiny = compute_regions(Rect::new(0, 0, 70, 30));
        assert!(!DeliveryRegions::is_visible(tiny.pr_rail));
        assert!(DeliveryRegions::is_visible(tiny.mission));
        assert!(DeliveryRegions::is_visible(tiny.canvas));
    }

    #[test]
    fn regions_dont_overlap() {
        let r = compute_regions(Rect::new(0, 0, 180, 50));
        // mission + pr_rail + canvas + footer = area.height (vertically)
        let bottom_of_canvas = r.canvas.y + r.canvas.height;
        let top_of_footer = r.footer.y;
        assert!(
            bottom_of_canvas <= top_of_footer,
            "canvas must end at or before footer"
        );

        // phase_rail + canvas + minimap = area.width
        assert_eq!(r.phase_rail.width + r.canvas.width + r.minimap.width, 180);
    }
}

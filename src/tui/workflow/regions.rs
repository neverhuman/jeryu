//! Compatibility shim for the split workflow region layout.

pub use crate::tui::lenses::workflow::regions::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_shim_exports_regions() {
        assert_eq!(PHASE_RAIL_W, 17);
        let regions = compute_regions(ratatui::layout::Rect::new(0, 0, 200, 60));
        assert!(DeliveryRegions::is_visible(regions.canvas));
    }
}

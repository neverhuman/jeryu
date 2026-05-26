//! Compatibility shim for the split workflow minimap.

pub use crate::tui::lenses::workflow::rails::minimap::{draw_minimap, locate_minimap_click};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_shim_exports_minimap_hit_test() {
        let _ = locate_minimap_click;
    }
}

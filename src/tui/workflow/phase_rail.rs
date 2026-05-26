//! Compatibility shim for the split workflow phase rail.

pub use crate::tui::lenses::workflow::rails::phase::draw_phase_rail;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_shim_exports_phase_rail() {
        let _ = draw_phase_rail;
    }
}

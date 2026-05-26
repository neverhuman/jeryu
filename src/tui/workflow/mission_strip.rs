//! Compatibility shim for the split workflow mission strip.

pub use crate::tui::lenses::workflow::rails::mission::draw_mission_strip;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_shim_exports_mission_strip() {
        let _ = draw_mission_strip;
    }
}

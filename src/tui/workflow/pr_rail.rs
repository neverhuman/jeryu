//! Compatibility shim for the split workflow PR rail.

pub use crate::tui::lenses::workflow::rails::pr::{
    CHIP_W, draw_pr_rail, pr_at_column, pr_at_column_filtered,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_shim_exports_pr_hit_test() {
        assert_eq!(CHIP_W, 30);
        let _ = pr_at_column;
    }
}

//! Compatibility shim for the split workflow inspector.
//!
//! New code should import `crate::tui::lenses::workflow::inspector::*`.

pub use crate::tui::lenses::workflow::inspector::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_shim_exports_split_inspector() {
        assert_eq!(INSPECTOR_W, 48);
        assert_eq!(INSPECTOR_MIN_TERM_W, 140);
        assert_eq!(InspectorTab::Logs.label(), "Logs");
    }
}

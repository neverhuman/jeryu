//! Compatibility shim for the split workflow model.
//!
//! New code should import `crate::tui::lenses::workflow::model::*`.

pub use crate::tui::lenses::workflow::model::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_shim_exports_split_model() {
        assert_eq!(WorkflowStatus::Ran.label(), "RAN");
        assert_eq!(WorkflowSnapshot::empty().source, WorkflowSource::Demo);
    }
}

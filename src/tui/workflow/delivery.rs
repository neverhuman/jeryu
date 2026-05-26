//! Compatibility shim for the split workflow delivery model.
//!
//! New code should import `crate::tui::lenses::workflow::delivery::*`.

pub use crate::tui::lenses::workflow::delivery::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_shim_exports_split_delivery() {
        let snapshot = build_demo_delivery();
        assert_eq!(snapshot.pull_requests.len(), 5);
        assert_eq!(AGENT_REVIEW_AUTO_PASS_DELAY_SECS, 5);
    }
}

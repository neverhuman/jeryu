//! Unified entity model for the read-model contract.
//!
//! Every TUI/web-rendered object maps to exactly one [`EntityKind`]; entity IDs
//! are globally unique within a kind. Provider-neutral: no SCM-vendor names.

mod kind;
mod refs;
mod support;

pub use kind::EntityKind;
pub use refs::{EntityRef, HealthLevel, Severity};
pub use support::{
    ActionRef, BlockerSummary, Bug, BugAttempt, DataFreshness, EntityDetail, EvidenceRef, Project,
    TimelineEvent,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_ref_display_uses_label() {
        let r = EntityRef::new(EntityKind::Job, "14445");
        assert_eq!(r.display(), "job:14445");
        assert_eq!(format!("{r}"), "job:14445");
    }

    #[test]
    fn pull_request_kind_is_provider_neutral() {
        assert_eq!(EntityKind::PullRequest.label(), "pr");
        assert_eq!(EntityKind::PullRequest.badge(), "PR");
        assert_eq!(EntityKind::PullRequest.route_segment(), "pull-requests");
    }

    #[test]
    fn all_entity_kinds_have_distinct_serde_tags() {
        // Every kind must round-trip through serde and ALL must be exhaustive.
        for kind in EntityKind::ALL {
            let json = serde_json::to_string(kind).unwrap();
            let back: EntityKind = serde_json::from_str(&json).unwrap();
            assert_eq!(*kind, back);
        }
    }

    #[test]
    fn severity_orders_critical_first() {
        assert!(Severity::Critical < Severity::Error);
        assert!(Severity::Error < Severity::Warning);
        assert!(Severity::Warning < Severity::Info);
    }
}

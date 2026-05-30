//! The canonical bug domain: types, enums, records, and pure ops.
//!
//! Invariants carried from jeryu: canonical reports validate before insert; the
//! durable backend is RedlineDB (wired later behind [`crate::store::BugStore`]).

pub mod enums;
pub mod ops;
pub mod records;
pub mod types;

pub use enums::{AttemptStatus, BugPriority, BugSeverity, BugSort, BugStatus};
pub use ops::{
    branch_name, generate_bug_id, parse_report_json, ranking_key, sort_bugs, validate_transition,
};
pub use records::{
    BugAttempt, BugAttemptInput, BugDetail, BugEvent, BugEvidenceInput, BugProject,
    BugProjectInput, BugRecord,
};
pub use types::CanonicalBugReport;

//! Owner: bug-tracker domain
//! Proof: `cargo test -p jeryu --lib bugtracker`
//! Invariants: RedlineDB is the only durable backend; canonical reports validate before insert.

mod ops;
pub mod render;
mod types;

pub use ops::{branch_name, generate_bug_id, parse_report_json, ranking_key, validate_transition};
pub use types::{
    AttemptStatus, BugAttempt, BugAttemptInput, BugDetail, BugEvent, BugEvidenceInput, BugPriority,
    BugProject, BugProjectInput, BugRecord, BugSeverity, BugSort, BugStatus, CanonicalBugReport,
};

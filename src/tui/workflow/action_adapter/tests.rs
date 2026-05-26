//! Owner: Interactive TUI subsystem — action_adapter tests root
//! Proof: `cargo test -p jeryu --lib tui::workflow::action_adapter`
//! Invariants:
//!   - All adapter tests live under one `cargo test` filter
//!     (`tui::workflow::action_adapter`). New tests must declare in one of
//!     the existing submodules (`unit`, `handler`, `app`) or in a new
//!     submodule declared here.
//!   - `sample_entry` is the canonical fixture for `LaunchLedgerEntry`
//!     across the unit + handler suites; do not inline a parallel fixture.

use crate::autonomy::signing::Signature;
use crate::autonomy::types::{LaunchLedgerEntry, LedgerKind, SchemaTag};
use chrono::Utc;

pub(super) fn sample_entry(kind: LedgerKind, actor: &str) -> LaunchLedgerEntry {
    LaunchLedgerEntry {
        schema: SchemaTag::default(),
        id: format!("ll_test_{}", uuid::Uuid::new_v4()),
        kind,
        subject_id: "subj-1".into(),
        repo: Some("acme/widgets".into()),
        payload: serde_json::json!({"hello": "world"}),
        recorded_at: Utc::now(),
        actor: actor.into(),
        signature: Signature::stub(),
    }
}

mod app;
mod handler;
mod unit;

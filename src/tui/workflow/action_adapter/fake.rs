//! Owner: Interactive TUI subsystem — Mission Control fake adapter (Wave 6.A)
//! Proof: `cargo test -p jeryu --lib tui::workflow::action_adapter`
//! Invariants: Records every call; never performs I/O.

use std::sync::Arc;

use async_trait::async_trait;

use crate::autonomy::types::{GateDecision, LaunchLedgerEntry};

use super::ActionAdapter;

/// A single call recorded by [`FakeActionAdapter`]. The variants mirror the
/// trait methods so tests can assert exact call order and arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordedCall {
    PostPassportCheck {
        repo: String,
        head_sha: String,
        decision: GateDecision,
        summary: String,
    },
    PostMrComment {
        repo: String,
        mr_iid: String,
        body: String,
    },
    PauseKillBell {
        reason: String,
        paused_by: String,
        ttl_seconds: u64,
    },
    AppendLedger {
        kind: String, // snake_case ledger-kind label, mirrors SQL serialization
        actor: String,
        subject_id: String,
        payload: serde_json::Value,
    },
}

/// In-memory adapter used by unit tests AND by the TUI's dry-run mode. Every
/// call is appended to `calls`. When `return_error_on` matches the method
/// name (e.g. "post_passport_check"), that method returns `Err(...)`.
#[derive(Default)]
pub struct FakeActionAdapter {
    pub calls: Arc<std::sync::Mutex<Vec<RecordedCall>>>,
    pub return_error_on: Option<String>,
}

impl FakeActionAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_error_on(method: impl Into<String>) -> Self {
        Self {
            calls: Arc::new(std::sync::Mutex::new(Vec::new())),
            return_error_on: Some(method.into()),
        }
    }

    /// Configure the adapter to fail the next call matching `method` (e.g.
    /// `"post_passport_check"`). Spelled to mirror "fail next" assertions
    /// used in newer test patterns; equivalent to [`with_error_on`].
    pub fn fail_next(method: impl Into<String>) -> Self {
        Self::with_error_on(method)
    }

    pub fn calls(&self) -> Vec<RecordedCall> {
        self.calls.lock().expect("fake adapter mutex").clone()
    }

    fn record(&self, call: RecordedCall) {
        self.calls.lock().expect("fake adapter mutex").push(call);
    }

    fn err_if_matches(&self, method: &str) -> Result<(), String> {
        match &self.return_error_on {
            Some(m) if m == method => Err(format!("fake adapter error injected on {method}")),
            _ => Ok(()),
        }
    }
}

#[async_trait]
impl ActionAdapter for FakeActionAdapter {
    async fn post_passport_check(
        &self,
        repo: &str,
        head_sha: &str,
        decision: GateDecision,
        summary: &str,
    ) -> Result<String, String> {
        self.record(RecordedCall::PostPassportCheck {
            repo: repo.into(),
            head_sha: head_sha.into(),
            decision,
            summary: summary.into(),
        });
        self.err_if_matches("post_passport_check")?;
        Ok(format!("fake-check-run::{head_sha}"))
    }

    async fn post_mr_comment(
        &self,
        repo: &str,
        mr_iid: &str,
        body: &str,
    ) -> Result<String, String> {
        self.record(RecordedCall::PostMrComment {
            repo: repo.into(),
            mr_iid: mr_iid.into(),
            body: body.into(),
        });
        self.err_if_matches("post_mr_comment")?;
        Ok(format!("fake-comment::{mr_iid}"))
    }

    async fn pause_kill_bell(
        &self,
        reason: &str,
        paused_by: &str,
        ttl_seconds: u64,
    ) -> Result<(), String> {
        self.record(RecordedCall::PauseKillBell {
            reason: reason.into(),
            paused_by: paused_by.into(),
            ttl_seconds,
        });
        self.err_if_matches("pause_kill_bell")
    }

    async fn append_ledger(&self, entry: LaunchLedgerEntry) -> Result<(), String> {
        // Mirror SqlLedger's snake_case kind serialization without depending
        // on the private helper.
        let kind = match serde_json::to_value(entry.kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
        {
            Some(kind) => kind,
            None => String::new(),
        };
        self.record(RecordedCall::AppendLedger {
            kind,
            actor: entry.actor.clone(),
            subject_id: entry.subject_id.clone(),
            payload: entry.payload.clone(),
        });
        self.err_if_matches("append_ledger")
    }

    fn kind(&self) -> &'static str {
        "fake"
    }
}

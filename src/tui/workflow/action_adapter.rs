//! Owner: Interactive TUI subsystem — Mission Control action adapter (Wave 6.A)
//! Proof: `cargo test -p jeryu --lib tui::workflow::action_adapter`
//! Invariants:
//!   - The trait is the ONLY surface `handle_delivery_action` touches. The TUI
//!     never imports `GitHubClient` or `KillBell` directly — those concrete
//!     types live behind [`ProductionActionAdapter`]. This keeps the action
//!     handler unit-testable against [`FakeActionAdapter`] without any
//!     network, FS, or embedded-database dependency.
//!   - Every public method returns `Result<_, String>` so failures surface as
//!     `ActionOutcome::Failed(msg)` in the TUI without exposing the underlying
//!     error type (which would otherwise leak `reqwest`/`anyhow` into the
//!     pure-render layer).
//!   - The `ProductionActionAdapter` is the only place that touches the
//!     signed-ledger pool. Cloning it is cheap (every wrapped field is
//!     already `Arc` / `AnyPool`-shaped).
//!
//! Wave 6.A wires the Wave 5.B action buttons (Approve / Block / Repair /
//! Freeze / KillBell) into real backends. Before this module the TUI logged
//! intent but did not act — see `docs/evidence-gate-spec.md` and
//! `tips/fullauto/tip8.txt` ("humans interrupt only at irreversible or
//! high-risk boundaries; every interrupt is a signed audit event").

use std::sync::Arc;

use async_trait::async_trait;

use crate::autonomy::kill_bell::KillBell;
use crate::autonomy::signing::EdSigningKey;
use crate::autonomy::types::{GateDecision, LaunchLedgerEntry};
use crate::autonomy::{SqlLedger, sign_entry};
use crate::db::AnyPool;
use crate::git_host::{GitHost, GitHubClient, RepoRef};

/// Side-effect surface invoked by `App::handle_delivery_action` once an
/// operator confirms one of the 5 Mission Control buttons. Every method
/// returns `Result<_, String>` so the UI can surface failures verbatim via
/// `ActionOutcome::Failed(msg)`.
///
/// The trait is intentionally narrow:
/// - `post_passport_check` covers ApproveOnce + BlockVerdict (the GitHub-side
///   required check that gates merge).
/// - `post_mr_comment` covers BlockVerdict's secondary reason comment and
///   RequestRepair.
/// - `pause_kill_bell` covers the KillBell button (engaging the global pause).
/// - `append_ledger` covers EVERY action's "human intervention" audit row —
///   Tip4/6/8 require a signed ledger event per interrupt. This lives on the
///   adapter (not on a separate ledger seam) so the fake adapter has a single
///   place to record + assert ordering across calls.
#[async_trait]
pub trait ActionAdapter: Send + Sync {
    /// Post the canonical `vibegate/merge-passport` GitHub check on `head_sha`
    /// with the given `decision` and human-readable `summary`. Returns the
    /// host-side check-run id (or other dispatcher token) on success.
    async fn post_passport_check(
        &self,
        repo: &str,
        head_sha: &str,
        decision: GateDecision,
        summary: &str,
    ) -> Result<String, String>;

    /// Post a markdown comment on the merge request / pull request.
    async fn post_mr_comment(&self, repo: &str, mr_iid: &str, body: &str)
    -> Result<String, String>;

    /// Engage the Kill Bell for `ttl_seconds`. The adapter is responsible for
    /// signing + appending the canonical `KillBellEngaged` ledger event.
    /// Callers MUST NOT also append a `KillBellEngaged` row — see the
    /// `kill_bell_action_does_not_double_append_ledger_entry` regression
    /// guard in this module's tests.
    async fn pause_kill_bell(
        &self,
        reason: &str,
        paused_by: &str,
        ttl_seconds: u64,
    ) -> Result<(), String>;

    /// Append a signed human-intervention ledger entry. The handler builds
    /// the entry (kind, payload, actor) and the adapter signs + persists it.
    /// In tests this records the entry in `RecordedCall::AppendLedger` so
    /// assertions can verify ordering and duplicate-suppression.
    async fn append_ledger(&self, entry: LaunchLedgerEntry) -> Result<(), String>;

    /// Introspection hook: `"production"` for the real GitHub + SQL adapter,
    /// `"fake"` for the in-memory test/dry-run adapter. The App uses this to
    /// surface whether `try_install_production_adapter` swapped in a real
    /// backend without exposing the concrete adapter type.
    fn kind(&self) -> &'static str {
        "production"
    }
}

// ─── Production adapter ────────────────────────────────────────────────────

/// Real-world adapter used by the live TUI. Cheap to clone (`github` is `Arc`,
/// `pool` is `AnyPool`-shaped, `signing_key` is `Arc`).
#[derive(Clone)]
pub struct ProductionActionAdapter {
    pub github: Arc<GitHubClient>,
    pub pool: AnyPool,
    pub signing_key: Arc<EdSigningKey>,
}

impl ProductionActionAdapter {
    pub fn new(github: Arc<GitHubClient>, pool: AnyPool, signing_key: Arc<EdSigningKey>) -> Self {
        Self {
            github,
            pool,
            signing_key,
        }
    }
}

#[async_trait]
impl ActionAdapter for ProductionActionAdapter {
    async fn post_passport_check(
        &self,
        repo: &str,
        head_sha: &str,
        decision: GateDecision,
        summary: &str,
    ) -> Result<String, String> {
        let repo_ref = RepoRef::parse(repo)
            .ok_or_else(|| format!("invalid repo slug '{repo}' (expected owner/name)"))?;
        let res = self
            .github
            .post_merge_passport_check(&repo_ref, head_sha, decision, summary, None)
            .await
            .map_err(|e| e.to_string())?;
        Ok(res.id)
    }

    async fn post_mr_comment(
        &self,
        repo: &str,
        mr_iid: &str,
        body: &str,
    ) -> Result<String, String> {
        let repo_ref = RepoRef::parse(repo)
            .ok_or_else(|| format!("invalid repo slug '{repo}' (expected owner/name)"))?;
        self.github
            .post_mr_comment(&repo_ref, mr_iid, body)
            .await
            .map_err(|e| e.to_string())
    }

    async fn pause_kill_bell(
        &self,
        reason: &str,
        paused_by: &str,
        ttl_seconds: u64,
    ) -> Result<(), String> {
        let bell = KillBell::new(self.pool.clone());
        bell.pause(
            reason,
            paused_by,
            ttl_seconds,
            self.signing_key.as_ref(),
            chrono::Utc::now(),
        )
        .await
        .map_err(|e| e.to_string())
    }

    async fn append_ledger(&self, mut entry: LaunchLedgerEntry) -> Result<(), String> {
        // Sign and persist. The handler hands us an unsigned entry (stub
        // signature) so the adapter has a single, auditable place to apply
        // the operator's signing key.
        sign_entry(&mut entry, self.signing_key.as_ref());
        let ledger = SqlLedger::new(self.pool.clone());
        ledger.append(&entry).await.map_err(|e| e.to_string())
    }
}

// ─── Fake adapter (tests + dry-run mode) ───────────────────────────────────

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

// ─── Handler helpers ───────────────────────────────────────────────────────

/// Pure helpers consumed by `App::handle_delivery_action`. Kept in this module
/// so the trait, the production wiring, and the per-action plumbing all live
/// under one `cargo test` filter (`tui::workflow::action_adapter`).
pub mod handler_helpers {
    use crate::autonomy::signing::Signature;
    use crate::autonomy::types::{LaunchLedgerEntry, LedgerKind, SchemaTag};
    use crate::tui::workflow::model::DeliverySnapshot;
    use chrono::{DateTime, Utc};
    use uuid::Uuid;

    /// Minimal projection of the currently focused PR; chosen via `pr_idx`.
    /// The handler builds this once at the top of each branch so every
    /// downstream call (passport, comment, ledger entry) sees a consistent
    /// view of the PR even if the snapshot mutates mid-await.
    #[derive(Debug, Clone)]
    pub struct PrCtx {
        pub pr_number: u64,
        pub head_sha: String,
        /// `owner/name` repo slug. The demo snapshot does not carry a repo
        /// field, so we synthesize a placeholder; the production wiring
        /// will surface the real slug once `PullRequestView::repo` lands.
        pub repo_slug: String,
    }

    pub fn pr_ctx(snapshot: &DeliverySnapshot, pr_idx: usize) -> Option<PrCtx> {
        let pr = snapshot.pull_requests.get(pr_idx)?;
        Some(PrCtx {
            pr_number: pr.number,
            head_sha: pr.head_sha.clone(),
            // TODO(wave6.B): `PullRequestView` does not yet carry the host
            // slug. Until then we tag the synthetic value so tests can
            // assert the seam was hit without asserting the placeholder.
            repo_slug: "tui-cockpit/demo".to_string(),
        })
    }

    /// Build an UNSIGNED `LaunchLedgerEntry` for a PR-scoped human action.
    /// The adapter signs + persists it. `actor` is always `"tui.cockpit.v1"`.
    pub fn ledger_entry(
        kind: LedgerKind,
        ctx: &PrCtx,
        payload: serde_json::Value,
        now: DateTime<Utc>,
    ) -> LaunchLedgerEntry {
        LaunchLedgerEntry {
            schema: SchemaTag::default(),
            id: format!("ll_tui_{}", Uuid::new_v4()),
            kind,
            subject_id: format!("pr#{}", ctx.pr_number),
            repo: Some(ctx.repo_slug.clone()),
            payload,
            recorded_at: now,
            actor: "tui.cockpit.v1".into(),
            signature: Signature::stub(),
        }
    }

    /// Build an UNSIGNED `LaunchLedgerEntry` for a non-PR-scoped intent
    /// (e.g. `FreezeAutonomy`, whose subject is the autonomy plane, not
    /// any single PR).
    pub fn ledger_entry_subject(
        kind: LedgerKind,
        subject_id: &str,
        repo: Option<String>,
        payload: serde_json::Value,
        now: DateTime<Utc>,
    ) -> LaunchLedgerEntry {
        LaunchLedgerEntry {
            schema: SchemaTag::default(),
            id: format!("ll_tui_{}", Uuid::new_v4()),
            kind,
            subject_id: subject_id.into(),
            repo,
            payload,
            recorded_at: now,
            actor: "tui.cockpit.v1".into(),
            signature: Signature::stub(),
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::signing::Signature;
    use crate::autonomy::types::{LaunchLedgerEntry, LedgerKind, SchemaTag};
    use chrono::Utc;

    fn sample_entry(kind: LedgerKind, actor: &str) -> LaunchLedgerEntry {
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

    #[tokio::test]
    async fn fake_adapter_records_post_passport_check() {
        let fake = FakeActionAdapter::new();
        let id = fake
            .post_passport_check(
                "acme/widgets",
                "deadbeef",
                GateDecision::AllowMerge,
                "all green",
            )
            .await
            .expect("ok");
        assert!(id.contains("deadbeef"));
        let calls = fake.calls();
        assert_eq!(calls.len(), 1);
        match &calls[0] {
            RecordedCall::PostPassportCheck {
                repo,
                head_sha,
                decision,
                summary,
            } => {
                assert_eq!(repo, "acme/widgets");
                assert_eq!(head_sha, "deadbeef");
                assert_eq!(*decision, GateDecision::AllowMerge);
                assert_eq!(summary, "all green");
            }
            other => panic!("expected PostPassportCheck, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fake_adapter_returns_error_when_configured() {
        let fake = FakeActionAdapter::with_error_on("post_passport_check");
        let r = fake
            .post_passport_check("a/b", "sha", GateDecision::Reject, "bad")
            .await;
        assert!(r.is_err(), "expected injected error");
        // The call is still recorded so tests can assert the surface was hit.
        assert_eq!(fake.calls().len(), 1);
    }

    #[tokio::test]
    async fn fake_adapter_records_append_ledger_with_snake_case_kind() {
        let fake = FakeActionAdapter::new();
        let entry = sample_entry(LedgerKind::HumanDecisionRecorded, "tui.cockpit.v1");
        fake.append_ledger(entry).await.expect("ok");
        let calls = fake.calls();
        assert_eq!(calls.len(), 1);
        match &calls[0] {
            RecordedCall::AppendLedger { kind, actor, .. } => {
                assert_eq!(kind, "human_decision_recorded");
                assert_eq!(actor, "tui.cockpit.v1");
            }
            other => panic!("expected AppendLedger, got {other:?}"),
        }
    }

    // ─── handle_delivery_action integration tests ─────────────────────────
    //
    // These bind a real `App` (in-memory store) to a `FakeActionAdapter` so
    // we can assert the wave-6.A behaviour spec end-to-end without any
    // network or signed-ledger SQL setup.

    use crate::tui::workflow::actions::{ActionOutcome, DeliveryAction};
    use crate::tui::workflow::delivery::build_demo_delivery;

    async fn app_with_demo_delivery() -> crate::tui::app::App {
        let mut app = crate::tui::app::test_app()
            .await
            .expect("build in-memory test app");
        app.delivery_snapshot = build_demo_delivery();
        app
    }

    fn ledger_kinds(fake: &FakeActionAdapter) -> Vec<String> {
        fake.calls()
            .into_iter()
            .filter_map(|c| match c {
                RecordedCall::AppendLedger { kind, .. } => Some(kind),
                _ => None,
            })
            .collect()
    }

    fn call_names(fake: &FakeActionAdapter) -> Vec<&'static str> {
        fake.calls()
            .into_iter()
            .map(|c| match c {
                RecordedCall::PostPassportCheck { .. } => "post_passport_check",
                RecordedCall::PostMrComment { .. } => "post_mr_comment",
                RecordedCall::PauseKillBell { .. } => "pause_kill_bell",
                RecordedCall::AppendLedger { .. } => "append_ledger",
            })
            .collect()
    }

    #[tokio::test]
    async fn handle_approve_once_calls_post_passport_check_with_allow_merge() {
        let mut app = app_with_demo_delivery().await;
        let fake = FakeActionAdapter::new();
        app.handle_delivery_action(DeliveryAction::ApproveOnce { pr_idx: 0 }, &fake)
            .await;
        let calls = fake.calls();
        // First call must be the passport check with AllowMerge.
        match &calls[0] {
            RecordedCall::PostPassportCheck { decision, .. } => {
                assert_eq!(*decision, GateDecision::AllowMerge);
            }
            other => panic!("expected PostPassportCheck first, got {other:?}"),
        }
        // The action pane should show Submitted.
        assert!(matches!(
            app.action_pane.last_result.as_ref().map(|r| &r.outcome),
            Some(ActionOutcome::Submitted)
        ));
    }

    #[tokio::test]
    async fn handle_block_verdict_calls_passport_check_then_comment() {
        let mut app = app_with_demo_delivery().await;
        let fake = FakeActionAdapter::new();
        app.handle_delivery_action(
            DeliveryAction::BlockVerdict {
                pr_idx: 0,
                reason: "regression in checkout".into(),
            },
            &fake,
        )
        .await;
        let names = call_names(&fake);
        // Exact order: passport_check (Reject) → comment → ledger
        assert_eq!(
            &names[..3],
            &["post_passport_check", "post_mr_comment", "append_ledger"],
            "BLOCK must passport-check first, then comment, then ledger; got {names:?}",
        );
        // Reject decision and reason are surfaced in the call args.
        match &fake.calls()[0] {
            RecordedCall::PostPassportCheck {
                decision, summary, ..
            } => {
                assert_eq!(*decision, GateDecision::Reject);
                assert!(summary.contains("regression in checkout"));
            }
            other => panic!("expected PostPassportCheck, got {other:?}"),
        }
        match &fake.calls()[1] {
            RecordedCall::PostMrComment { body, .. } => {
                assert!(body.contains("regression in checkout"));
                assert!(body.starts_with("Agent: BLOCK"));
            }
            other => panic!("expected PostMrComment, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn handle_request_repair_calls_post_mr_comment_only() {
        let mut app = app_with_demo_delivery().await;
        let fake = FakeActionAdapter::new();
        app.handle_delivery_action(DeliveryAction::RequestRepair { pr_idx: 0 }, &fake)
            .await;
        let names = call_names(&fake);
        // Repair must NOT touch the passport check.
        assert!(
            !names.contains(&"post_passport_check"),
            "RequestRepair must not post a passport check; got {names:?}"
        );
        assert_eq!(
            &names[..2],
            &["post_mr_comment", "append_ledger"],
            "RequestRepair = comment + ledger; got {names:?}"
        );
        match &fake.calls()[0] {
            RecordedCall::PostMrComment { body, .. } => {
                assert!(
                    body.contains("repair this MR"),
                    "repair-comment body should reference repair: {body}"
                );
            }
            other => panic!("expected PostMrComment, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn handle_freeze_autonomy_appends_ledger_event_no_adapter_call() {
        let mut app = app_with_demo_delivery().await;
        let fake = FakeActionAdapter::new();
        app.handle_delivery_action(DeliveryAction::FreezeAutonomy { hours: 12 }, &fake)
            .await;
        let calls = fake.calls();
        // Freeze must ONLY append a ledger event (no passport / comment /
        // kill-bell calls). Ops finalizes via CLI.
        assert_eq!(calls.len(), 1, "freeze emits exactly one ledger row");
        match &calls[0] {
            RecordedCall::AppendLedger { kind, payload, .. } => {
                assert_eq!(kind, "human_escalation_requested");
                assert_eq!(payload["action"], "freeze_intent");
                assert_eq!(payload["hours"], 12);
            }
            other => panic!("expected AppendLedger only, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn handle_kill_bell_calls_pause_then_returns_submitted() {
        let mut app = app_with_demo_delivery().await;
        let fake = FakeActionAdapter::new();
        app.handle_delivery_action(
            DeliveryAction::KillBell {
                reason: "incident-42".into(),
            },
            &fake,
        )
        .await;
        let calls = fake.calls();
        assert_eq!(
            calls.len(),
            1,
            "KillBell must invoke exactly one adapter call"
        );
        match &calls[0] {
            RecordedCall::PauseKillBell {
                reason,
                paused_by,
                ttl_seconds,
            } => {
                assert_eq!(reason, "incident-42");
                assert_eq!(paused_by, "tui.cockpit.v1");
                assert_eq!(*ttl_seconds, 86_400);
            }
            other => panic!("expected PauseKillBell, got {other:?}"),
        }
        assert!(matches!(
            app.action_pane.last_result.as_ref().map(|r| &r.outcome),
            Some(ActionOutcome::Submitted)
        ));
        // Mission strip mirror updates so operators see paused state.
        assert_eq!(app.delivery_snapshot.kill_bell_state, "paused");
    }

    #[tokio::test]
    async fn adapter_error_surfaces_as_failed_outcome() {
        let mut app = app_with_demo_delivery().await;
        let fake = FakeActionAdapter::with_error_on("post_passport_check");
        app.handle_delivery_action(DeliveryAction::ApproveOnce { pr_idx: 0 }, &fake)
            .await;
        let outcome = app
            .action_pane
            .last_result
            .as_ref()
            .map(|r| r.outcome.clone());
        match outcome {
            Some(ActionOutcome::Failed(msg)) => {
                assert!(msg.contains("post_passport_check"));
            }
            other => panic!("expected Failed outcome, got {other:?}"),
        }
        // No ledger row written on failure path.
        assert!(
            !call_names(&fake).contains(&"append_ledger"),
            "failed approve must not append a ledger row"
        );
    }

    #[tokio::test]
    async fn approve_once_appends_ledger_entry_with_human_decision_kind() {
        let mut app = app_with_demo_delivery().await;
        let fake = FakeActionAdapter::new();
        app.handle_delivery_action(DeliveryAction::ApproveOnce { pr_idx: 0 }, &fake)
            .await;
        let kinds = ledger_kinds(&fake);
        assert_eq!(
            kinds,
            vec!["human_decision_recorded".to_string()],
            "approve must record exactly one HumanDecisionRecorded entry"
        );
        // Actor is the canonical TUI cockpit stamp.
        let actor_ok = fake.calls().into_iter().any(|c| match c {
            RecordedCall::AppendLedger { actor, .. } => actor == "tui.cockpit.v1",
            _ => false,
        });
        assert!(actor_ok, "ledger entry must be stamped 'tui.cockpit.v1'");
    }

    #[tokio::test]
    async fn kill_bell_action_does_not_double_append_ledger_entry() {
        // The KillBell::pause path inside ProductionActionAdapter ALREADY
        // signs + appends a `KillBellEngaged` row; the handler MUST NOT
        // also call `adapter.append_ledger(KillBellEngaged{...})`. Verify
        // by inspecting the fake adapter's recorded calls.
        let mut app = app_with_demo_delivery().await;
        let fake = FakeActionAdapter::new();
        app.handle_delivery_action(
            DeliveryAction::KillBell {
                reason: "regression-fence".into(),
            },
            &fake,
        )
        .await;
        let kinds = ledger_kinds(&fake);
        assert!(
            kinds.is_empty(),
            "KillBell handler must defer ALL ledger append to the adapter; got {kinds:?}"
        );
        // And no extra calls besides the one pause.
        assert_eq!(fake.calls().len(), 1);
    }

    #[tokio::test]
    async fn request_repair_with_no_reason_still_succeeds() {
        // RequestRepair takes no reason argument; ensure the handler
        // doesn't accidentally require one (edge case) and still posts a
        // valid comment.
        let mut app = app_with_demo_delivery().await;
        let fake = FakeActionAdapter::new();
        app.handle_delivery_action(DeliveryAction::RequestRepair { pr_idx: 0 }, &fake)
            .await;
        assert!(matches!(
            app.action_pane.last_result.as_ref().map(|r| &r.outcome),
            Some(ActionOutcome::Submitted)
        ));
        // Non-empty comment body even without explicit reason text.
        let has_nonempty_comment = fake.calls().into_iter().any(|c| {
            matches!(
                c,
                RecordedCall::PostMrComment { body, .. } if !body.is_empty()
            )
        });
        assert!(has_nonempty_comment, "repair must post a non-empty comment");
    }

    // ─── Wave 6.A bin-integration tests ───────────────────────────────────
    //
    // These bind the action_adapter trait to App-level wiring (default
    // FakeActionAdapter, `try_install_production_adapter`, kind() seam).
    // They are the contract that `bin/autonomy` (or `tui/runner.rs`) calls
    // `try_install_production_adapter` once at startup and the rest of the
    // TUI flows through `App::action_adapter` without ever touching the
    // concrete GitHubClient / SqlLedger types directly.

    /// Serializes the env-var dependent tests so parallel runs don't race
    /// each other when toggling `GITHUB_TOKEN` / `JERYU_DATABASE_URL`.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[tokio::test]
    async fn app_default_action_adapter_is_fake() {
        let app = app_with_demo_delivery().await;
        assert_eq!(
            app.action_adapter.kind(),
            "fake",
            "App must default to FakeActionAdapter so unit tests don't need a DB"
        );
    }

    #[tokio::test]
    async fn app_action_pane_key_uses_installed_adapter() {
        // Install a known FakeActionAdapter, route a key through the pane,
        // and verify the recorded call lands on THAT instance — proving the
        // App routes through `self.action_adapter` instead of synthesizing a
        // throw-away adapter per keystroke (the Wave-6.A regression).
        let mut app = app_with_demo_delivery().await;
        let fake = Arc::new(FakeActionAdapter::new());
        // Capture a handle to the inner Mutex<Vec<RecordedCall>> so we can
        // observe calls after the App takes ownership of the Arc.
        let calls_handle = fake.calls.clone();
        app.action_adapter = fake.clone();
        app.action_pane.visible = true;

        // 'A' triggers ApproveOnce on the focused PR (idx 0).
        let consumed = app
            .action_pane_key(crossterm::event::KeyCode::Char('A'))
            .await;
        assert!(consumed, "action pane should consume the key while visible");
        let calls = calls_handle.lock().unwrap().clone();
        assert!(
            calls
                .iter()
                .any(|c| matches!(c, RecordedCall::PostPassportCheck { .. })),
            "the installed adapter must record the ApproveOnce passport check; got {calls:?}",
        );
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn try_install_production_adapter_succeeds_when_secrets_resolve() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        // SAFETY: tests in this crate co-operatively guard env mutation via
        // ENV_LOCK; we restore the prior values before returning.
        let prev_token = std::env::var("GITHUB_TOKEN").ok();
        let prev_db = std::env::var("JERYU_DATABASE_URL").ok();

        // `Db::open` memoizes the first successful pool in a global
        // OnceCell. Use a tempfile-backed SQLite DB so the multi-connection
        // pool (`open_url` uses max_connections=4) sees a shared schema
        // after migration, instead of the per-connection isolation that
        // in-memory DB would create. Safe to set even if the
        // singleton was already initialized — the call just reuses the
        // cached pool.
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        let db_url = crate::db::config::sqlite_url(tmp.path());
        // Rust 2024 marks `std::env::set_var` as unsafe because it
        // races with other threads reading env. Test runs single-threaded
        // via `cargo test -- --test-threads=1` (see scripts/pre-pr.sh) and
        // we restore env before the assertion barrier below.
        // SAFETY: env mutation is serialized by ENV_LOCK above.
        unsafe {
            // test-fixture: synthetic GitHub token shape, not a real credential.
            let synth_token = concat!("ghp_", "test_wave6a_install");
            std::env::set_var("GITHUB_TOKEN", synth_token);
            std::env::set_var("JERYU_DATABASE_URL", &db_url);
        }

        let mut app = app_with_demo_delivery().await;
        assert_eq!(app.action_adapter.kind(), "fake", "starts as fake");

        let result = app.try_install_production_adapter().await;

        // Rust 2024 marks `set_var`/`remove_var` unsafe due to
        // unsynchronized env reads; tests are serialized
        // (`--test-threads=1`) so no concurrent reader exists.
        // Restore env before any assertions so a panic doesn't leak state.
        // SAFETY: env mutation is serialized by ENV_LOCK above.
        unsafe {
            match prev_token {
                Some(v) => std::env::set_var("GITHUB_TOKEN", v),
                None => std::env::remove_var("GITHUB_TOKEN"),
            }
            match prev_db {
                Some(v) => std::env::set_var("JERYU_DATABASE_URL", v),
                None => std::env::remove_var("JERYU_DATABASE_URL"),
            }
        }

        assert!(
            result.is_ok(),
            "expected production adapter install to succeed: {:?}",
            result.err()
        );
        assert_eq!(
            app.action_adapter.kind(),
            "production",
            "kind() should flip to 'production' after successful install"
        );
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn try_install_production_adapter_keeps_fake_when_token_missing() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let prev_token = std::env::var("GITHUB_TOKEN").ok();
        let prev_ci = std::env::var("CI").ok();
        // SAFETY: Rust 2024 marks env mutation unsafe (unsynchronized
        // reads). The test holds `ENV_LOCK` for the duration of these
        // mutations and restores prior values before the assertion barrier.
        unsafe {
            std::env::remove_var("GITHUB_TOKEN");
            // SecretResolver walks local file tiers when not in CI mode. Force CI=true so
            // the test never picks up a developer-machine GITHUB_TOKEN from
            // one of those files.
            std::env::set_var("CI", "true");
        }

        let mut app = app_with_demo_delivery().await;
        let result = app.try_install_production_adapter().await;

        // SAFETY: same lock + serialization invariants as the prior block.
        // Restore env before assertions so a panic doesn't leak state.
        unsafe {
            if let Some(v) = prev_token {
                std::env::set_var("GITHUB_TOKEN", v);
            }
            match prev_ci {
                Some(v) => std::env::set_var("CI", v),
                None => std::env::remove_var("CI"),
            }
        }

        assert!(
            result.is_err(),
            "missing GITHUB_TOKEN must return Err; got Ok"
        );
        let msg = format!("{:?}", result.err().unwrap());
        assert!(
            msg.contains("GITHUB_TOKEN"),
            "error should name the missing secret: {msg}"
        );
        assert_eq!(
            app.action_adapter.kind(),
            "fake",
            "App must keep the fake adapter on failure so the TUI stays usable"
        );
    }

    #[test]
    fn production_adapter_kind_returns_production() {
        // Build a ProductionActionAdapter without going through Db::open so
        // we exercise the `kind()` default impl without env/DB side effects.
        use crate::autonomy::signing::EdSigningKey;
        use crate::db::{AnyPoolOptions, config as db_config, install_default_drivers};
        use crate::git_host::GitHubClient;
        use tempfile::NamedTempFile;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let adapter = rt.block_on(async {
            install_default_drivers();
            let tmp = NamedTempFile::new().expect("tempfile for production adapter test");
            let url = db_config::sqlite_url(tmp.path());
            let pool = AnyPoolOptions::new()
                .max_connections(4)
                .connect(&url)
                .await
                .expect("file-backed sqlite pool");
            std::mem::forget(tmp);
            ProductionActionAdapter::new(
                Arc::new(GitHubClient::new("ghp_test_kind")),
                pool,
                Arc::new(EdSigningKey::generate("tui.cockpit.v1.test")),
            )
        });
        assert_eq!(adapter.kind(), "production");
    }

    #[test]
    fn fake_adapter_kind_returns_fake() {
        let fake = FakeActionAdapter::new();
        assert_eq!(fake.kind(), "fake");
    }

    #[test]
    fn app_action_adapter_is_send_sync_for_tokio() {
        // Compile-time guarantee: the field type must be Send + Sync so
        // tokio tasks can `tokio::spawn` work that holds an `Arc<dyn
        // ActionAdapter>` (the auto-rejudge / background-sync paths in the
        // App). If anyone makes the trait `?Send`, this fails to compile.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Arc<dyn ActionAdapter>>();
    }

    #[tokio::test]
    async fn action_pane_key_propagates_adapter_errors_to_outcome() {
        // Install a FakeActionAdapter pre-armed to fail on
        // `post_passport_check`, then dispatch the 'A' (Approve) key
        // through the pane. The handler must surface the error as
        // `ActionOutcome::Failed(msg)` in the action pane's last_result so
        // operators see the failure rather than a silent no-op.
        let mut app = app_with_demo_delivery().await;
        let fake = Arc::new(FakeActionAdapter::fail_next("post_passport_check"));
        app.action_adapter = fake.clone();
        app.action_pane.visible = true;

        let consumed = app
            .action_pane_key(crossterm::event::KeyCode::Char('A'))
            .await;
        assert!(consumed);

        match app.action_pane.last_result.as_ref().map(|r| &r.outcome) {
            Some(ActionOutcome::Failed(msg)) => {
                assert!(!msg.is_empty(), "failed outcome must carry a message");
                assert!(
                    msg.contains("post_passport_check"),
                    "error should name the failing seam: {msg}"
                );
            }
            other => panic!("expected Failed outcome, got {other:?}"),
        }
    }
}

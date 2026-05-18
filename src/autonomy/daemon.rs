//! Owner: Evidence Gate / autonomous-delivery daemon (Wave 7.C)
//! Proof: `cargo test -p jeryu --lib autonomy::daemon`
//! Invariants:
//!   - Every detected drift produces a signed `MergePassportInvalidated`
//!     entry in the launch ledger (tip1 Law 10, tip9 "every autonomous
//!     decision creates a signed receipt").
//!   - The Kill Bell (Law 9) is the emergency stop. When `kill_bell_check_enabled`
//!     is true and the bell is paused: the daemon still SCANS for observability
//!     metrics, but it MUST NOT supersede verdicts, MUST NOT append ledger
//!     entries, and MUST NOT dispatch escalations.
//!   - Per-PR errors NEVER abort a whole tick — they are captured into
//!     `TickReport.errors` and the remaining PRs continue to be polled.
//!   - **Wave 7 scope is "detect + escalate"**: the daemon only DETECTS that
//!     a previously-issued verdict is stale (`RejudgeReason` triggered) and
//!     escalates via webhook for a human to re-run the gate. It does NOT
//!     re-run `judge()` itself — that requires re-running every reviewer
//!     agent (LLM calls, evidence-pack rebuild, etc.), which is the Wave 8
//!     "auto-rejudge" surface. Until then, `RejudgeRecord::new_decision`
//!     is always `None` and the escalation payload says "stale-verdict
//!     invalidation — please re-run the gate".
//!
//! TODO(Wave 8): when the auto-rejudge engine is available, replace the
//! `new_decision: None` path with an in-process `judge()` call and fill
//! `verdict_id_after` from the freshly-saved verdict.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::agent_review::rejudge::{LiveState, RejudgeReason, check};
use crate::autonomy::auto_rejudge::AutoRejudgeService;
use crate::autonomy::escalation::{
    DispatchResult, EscalationConfig, EscalationDispatcher, EscalationEvent, EscalationKind,
    dispatch_all,
};
use crate::autonomy::kill_bell::KillBell;
use crate::autonomy::ledger::{SqlLedger, sign_entry};
use crate::autonomy::signing::{EdSigningKey, Signature};
use crate::autonomy::types::{
    GateDecision, LaunchLedgerEntry, LedgerKind, SchemaTag, VibeGateVerdict,
};
use crate::autonomy::verdict_store::VerdictStore;
use crate::git_host::{GitHost, RepoRef};

// ---------------------------------------------------------------------------
// Public configuration + handle
// ---------------------------------------------------------------------------

/// Static configuration for one daemon instance. Cloned cheaply so callers
/// can stamp out per-repo / per-environment variants.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub repos: Vec<RepoRef>,
    pub interval_secs: u64,
    pub tick_once: bool,
    pub kill_bell_check_enabled: bool,
    pub escalation_enabled: bool,
    /// Wave 8.C — when true AND `auto_rejudge_service.is_some()`, the
    /// daemon runs the in-process rejudge pipeline on every drift hit
    /// instead of leaving `RejudgeRecord::new_decision = None`. Default
    /// `false` so existing Wave 7 deployments stay in detect-only mode.
    pub auto_rejudge_enabled: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            repos: Vec::new(),
            interval_secs: 60,
            tick_once: false,
            kill_bell_check_enabled: true,
            escalation_enabled: true,
            auto_rejudge_enabled: false,
        }
    }
}

/// The live daemon. Composes existing primitives — never mutates their
/// inner state through anything other than the documented surfaces.
pub struct Daemon {
    pub config: DaemonConfig,
    pub git_host: Arc<dyn GitHost>,
    pub verdict_store: Arc<dyn VerdictStore>,
    pub ledger: SqlLedger,
    pub kill_bell: KillBell,
    pub escalation_config: EscalationConfig,
    pub escalation_dispatcher: Arc<dyn EscalationDispatcher>,
    pub signing_key: Arc<EdSigningKey>,
    /// Wave 8.C — optional in-process rejudge service. `None` keeps the
    /// daemon in Wave 7 detect-only mode (back-compatible with every
    /// existing deployment that hasn't wired up the Wave 8 primitives).
    pub auto_rejudge_service: Option<Arc<AutoRejudgeService>>,
}

impl Daemon {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: DaemonConfig,
        git_host: Arc<dyn GitHost>,
        verdict_store: Arc<dyn VerdictStore>,
        ledger: SqlLedger,
        kill_bell: KillBell,
        escalation_config: EscalationConfig,
        escalation_dispatcher: Arc<dyn EscalationDispatcher>,
        signing_key: Arc<EdSigningKey>,
        auto_rejudge_service: Option<Arc<AutoRejudgeService>>,
    ) -> Self {
        Self {
            config,
            git_host,
            verdict_store,
            ledger,
            kill_bell,
            escalation_config,
            escalation_dispatcher,
            signing_key,
            auto_rejudge_service,
        }
    }

    /// Run one tick. See module docs for the algorithm.
    pub async fn tick(&self) -> TickReport {
        let started_at = Utc::now();
        let mut report = TickReport::new(started_at);

        // Step 1 + 2: check the Kill Bell. We still SCAN when paused so
        // operators can see the system is alive (tip1 Law 9: pause is a
        // pause on *actions*, not on *observation*).
        if self.config.kill_bell_check_enabled {
            match self.kill_bell.current(started_at).await {
                Ok(state) => report.kill_bell_paused = state.is_paused(),
                Err(e) => {
                    report.errors.push(TickError {
                        stage: "kill_bell.current".into(),
                        repo: None,
                        mr_iid: None,
                        message: e.to_string(),
                    });
                }
            }
        }

        // Step 3: scan each configured repo. Per-repo and per-PR errors are
        // captured into the report and never abort the whole tick.
        report.repos_scanned = self.config.repos.len();
        for repo in &self.config.repos {
            self.scan_repo(repo, &mut report).await;
        }

        report.finished_at = Utc::now();
        report
    }

    /// Body of the tick loop — extracted so the long-running `run()` path
    /// stays small.
    async fn scan_repo(&self, repo: &RepoRef, report: &mut TickReport) {
        let prs = match self.git_host.list_open_prs(repo).await {
            Ok(p) => p,
            Err(e) => {
                report.errors.push(TickError {
                    stage: "list_open_prs".into(),
                    repo: Some(repo.slug()),
                    mr_iid: None,
                    message: e.to_string(),
                });
                return;
            }
        };

        for pr in prs {
            if let Err(e) = self.scan_pr(repo, &pr.mr_iid, report).await {
                report.errors.push(TickError {
                    stage: "scan_pr".into(),
                    repo: Some(repo.slug()),
                    mr_iid: Some(pr.mr_iid.clone()),
                    message: e.to_string(),
                });
            }
        }
    }

    /// Inspect a single PR for drift. Returns `Err` only when something
    /// surprising and structural happens (DB failure on the ledger append,
    /// for instance) — the caller folds that into `report.errors`.
    async fn scan_pr(&self, repo: &RepoRef, mr_iid: &str, report: &mut TickReport) -> Result<()> {
        // 3a. Load the most recent non-superseded verdict for this PR. If
        // none, there is nothing to invalidate — skip silently.
        let verdict = match self
            .verdict_store
            .load_latest(&repo.slug(), Some(mr_iid))
            .await?
        {
            Some(v) => v,
            None => return Ok(()),
        };
        report.verdicts_inspected += 1;

        // 3b. Fetch live PR state from the host.
        let live_state = match self.git_host.get_pr_state(repo, mr_iid).await {
            Ok(s) => s,
            Err(e) => {
                report.errors.push(TickError {
                    stage: "get_pr_state".into(),
                    repo: Some(repo.slug()),
                    mr_iid: Some(mr_iid.to_string()),
                    message: e.to_string(),
                });
                return Ok(());
            }
        };

        // 3c. Run the rejudge check.
        let now = Utc::now();
        let live = LiveState {
            head_sha: Some(&live_state.head_sha),
            target_branch_sha: Some(&live_state.target_branch_sha),
            target_policy_sha: live_state.target_policy_sha.as_deref(),
            now: Some(now),
        };
        let triggers = check(&verdict, &live);
        if triggers.is_empty() {
            return Ok(()); // verdict is still bound to live state — nothing to do
        }

        // 3d. Drift detected. Append a signed ledger entry FIRST so the
        // audit trail leads the state change. We do this even when the
        // Kill Bell is paused — observation is allowed; only *actions*
        // (supersede + dispatch) are gated below.
        if !report.kill_bell_paused {
            let mut entry = build_invalidated_entry(&verdict, &triggers, now, &self.signing_key);
            sign_entry(&mut entry, &self.signing_key);
            self.ledger.append(&entry).await?;
        }

        // 3e. Mark the verdict superseded. Skipped under Kill Bell.
        if !report.kill_bell_paused {
            self.verdict_store.supersede(&verdict.id, now).await?;
        }

        // 3f. Wave 8.C auto-rejudge — when enabled AND a service is wired
        // AND we're not under Kill Bell pause, run the full rejudge pipeline
        // and use its outcome to populate the `RejudgeRecord` fields that
        // Wave 7 left as `None`. Failures here are recorded as `TickError`
        // but the drift record is still pushed (with `new_decision = None`)
        // so operators don't lose visibility on the detection event.
        let (verdict_id_after, new_decision) = if self.config.auto_rejudge_enabled
            && self.auto_rejudge_service.is_some()
            && !report.kill_bell_paused
        {
            let svc = self.auto_rejudge_service.as_ref().expect("checked above");
            match svc.rejudge(repo, mr_iid, &verdict).await {
                Ok(outcome) => (Some(outcome.new_verdict_id), Some(outcome.new_decision)),
                Err(e) => {
                    report.errors.push(TickError {
                        stage: "auto_rejudge".into(),
                        repo: Some(repo.slug()),
                        mr_iid: Some(mr_iid.to_string()),
                        message: e.to_string(),
                    });
                    (None, None)
                }
            }
        } else {
            (None, None)
        };

        // 3g. Build the report record (always pushed so paused-mode runs
        // still surface the metrics to operators). With auto-rejudge
        // disabled OR under pause, `verdict_id_after` + `new_decision`
        // stay `None` — that's the original Wave 7 detect-only behavior.
        report.rejudge_triggered.push(RejudgeRecord {
            repo: repo.slug(),
            mr_iid: mr_iid.to_string(),
            verdict_id_before: verdict.id.clone(),
            verdict_id_after,
            triggers: triggers
                .iter()
                .map(|r| r.short_name().to_string())
                .collect(),
            new_decision,
        });

        // 3g. Escalate, unless either disabled or under Kill Bell.
        if self.config.escalation_enabled && !report.kill_bell_paused {
            let event = EscalationEvent::RequireHuman {
                verdict: Box::new(verdict.clone()),
            };
            let results = dispatch_all(
                &self.escalation_config,
                &event,
                self.escalation_dispatcher.as_ref(),
            )
            .await;
            // `dispatch_all` returns empty when the config doesn't permit
            // the event name — in that case we still want to record the
            // intent (no webhook fired), so we don't filter on `is_empty`.
            report.escalations_dispatched.push(EscalationRecord {
                repo: repo.slug(),
                mr_iid: Some(mr_iid.to_string()),
                event_name: event.name().to_string(),
                results: results.iter().map(SerializedDispatch::from).collect(),
            });
        }

        Ok(())
    }

    /// Loop driver. Honors `tick_once` for the test path; otherwise sleeps
    /// `interval_secs` between ticks until `ctrl_c` arrives.
    pub async fn run(&self) -> Result<()> {
        if self.config.tick_once {
            let _ = self.tick().await;
            return Ok(());
        }

        let interval = Duration::from_secs(self.config.interval_secs.max(1));
        let mut ticker = tokio::time::interval(interval);
        // `tokio::time::interval` fires immediately on first `tick().await`;
        // that's the behavior we want (run a tick at startup, then space).

        #[cfg(unix)]
        let mut shutdown = Box::pin(tokio::signal::ctrl_c());

        loop {
            #[cfg(unix)]
            tokio::select! {
                _ = ticker.tick() => {
                    let _ = self.tick().await;
                }
                _ = &mut shutdown => {
                    return Ok(());
                }
            }
            #[cfg(not(unix))]
            {
                ticker.tick().await;
                let _ = self.tick().await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Report types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TickReport {
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub kill_bell_paused: bool,
    pub repos_scanned: usize,
    pub verdicts_inspected: usize,
    pub rejudge_triggered: Vec<RejudgeRecord>,
    pub escalations_dispatched: Vec<EscalationRecord>,
    pub errors: Vec<TickError>,
}

impl TickReport {
    fn new(started_at: DateTime<Utc>) -> Self {
        Self {
            started_at,
            finished_at: started_at,
            kill_bell_paused: false,
            repos_scanned: 0,
            verdicts_inspected: 0,
            rejudge_triggered: Vec::new(),
            escalations_dispatched: Vec::new(),
            errors: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RejudgeRecord {
    pub repo: String,
    pub mr_iid: String,
    pub verdict_id_before: String,
    pub verdict_id_after: Option<String>,
    /// `RejudgeReason::short_name()` for each trigger that fired, in order.
    pub triggers: Vec<String>,
    pub new_decision: Option<GateDecision>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EscalationRecord {
    pub repo: String,
    pub mr_iid: Option<String>,
    pub event_name: String,
    pub results: Vec<SerializedDispatch>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TickError {
    pub stage: String,
    pub repo: Option<String>,
    pub mr_iid: Option<String>,
    pub message: String,
}

/// `DispatchResult` is not `Serialize` (the `webhook_kind` field uses a
/// non-Serialize repr). We fold it into a thin serializable form for the
/// report payload — see spec note about `jeryu_escalation_serialized_dispatch_result`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SerializedDispatch {
    pub kind: String,
    pub status: Option<u16>,
    pub error: Option<String>,
}

impl From<&DispatchResult> for SerializedDispatch {
    fn from(r: &DispatchResult) -> Self {
        Self {
            kind: kind_to_str(r.webhook_kind).to_string(),
            status: r.status,
            error: r.error.clone(),
        }
    }
}

fn kind_to_str(k: EscalationKind) -> &'static str {
    match k {
        EscalationKind::Slack => "slack",
        EscalationKind::PagerDuty => "pagerduty",
        EscalationKind::GenericJson => "generic_json",
    }
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/// Build an unsigned `MergePassportInvalidated` ledger entry recording
/// which trigger(s) fired. Caller signs it before append.
fn build_invalidated_entry(
    verdict: &VibeGateVerdict,
    triggers: &[RejudgeReason],
    now: DateTime<Utc>,
    signing_key: &EdSigningKey,
) -> LaunchLedgerEntry {
    let trigger_names: Vec<&'static str> = triggers.iter().map(|r| r.short_name()).collect();
    let payload = serde_json::json!({
        "verdict_id": verdict.id,
        "repo": verdict.repo,
        "merge_request": verdict.merge_request,
        "head_sha": verdict.head_sha,
        "policy_sha": verdict.policy_sha,
        "triggers": trigger_names,
        // Full structured trigger detail so downstream consumers (TUI, escalation
        // payload) don't have to re-derive the before/after SHAs.
        "trigger_detail": triggers,
        // Wave 7 is detect-only; record the design choice for replay clarity.
        "wave_scope": "detect_only",
    });
    LaunchLedgerEntry {
        schema: SchemaTag::default(),
        id: format!("ll_invalidated_{}", Uuid::new_v4()),
        kind: LedgerKind::MergePassportInvalidated,
        subject_id: verdict.id.clone(),
        repo: Some(verdict.repo.clone()),
        payload,
        recorded_at: now,
        actor: format!("autonomy-daemon ({})", signing_key.key_id),
        // Replaced by `sign_entry` immediately after construction.
        signature: Signature::stub(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::escalation::{
        EscalationConfig, EscalationError, EscalationKind, WebhookConfig,
    };
    use crate::autonomy::ledger::LedgerFilter;
    use crate::autonomy::signing::Signature;
    use crate::autonomy::types::{
        GateDecision, RiskTier, SchemaTag, VerdictReceiptRef, VibeGateVerdict,
    };
    use crate::autonomy::verdict_store::SqlVerdictStore;
    use crate::db::AnyPool;
    use crate::db::autonomy_repo::fresh_autonomy_pool;
    use crate::git_host::test_utils::FakeGitHost;
    use crate::git_host::{PrLiveState, PrSummary, RepoRef};
    use async_trait::async_trait;
    use chrono::Duration;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // -- DB harness -----------------------------------------------------

    async fn fresh_db() -> AnyPool {
        // Test fixture moved to the db boundary so this file no longer
        // imports `sqlx::` (closes HLT-006). Schema mirrors the autonomy
        // tables used by ledger, kill bell, and verdict store.
        fresh_autonomy_pool().await
    }

    // -- Verdict helper -------------------------------------------------

    /// Mint a synthetic `VibeGateVerdict` with predictable id, head + policy
    /// SHAs and TTL.
    fn mint_verdict(
        repo: &str,
        mr: &str,
        head_sha_tail: &str,
        policy_sha_tail: &str,
        decision: GateDecision,
        expires_in_minutes: i64,
    ) -> VibeGateVerdict {
        let now = Utc::now();
        let head_sha = format!("{head_sha_tail:0>40}");
        let policy_sha = format!("{policy_sha_tail:0>40}");
        let id = format!(
            "vgv_{}_{}",
            now.timestamp_millis(),
            &head_sha[head_sha.len() - 8..]
        );
        VibeGateVerdict {
            schema: SchemaTag::new(),
            id,
            evidence_pack_id: "ep_test".into(),
            merge_request: Some(mr.into()),
            repo: repo.into(),
            target_branch: "main".into(),
            head_sha,
            policy_sha,
            evidence_pack_digest: "sha256:deadbeef".into(),
            risk: RiskTier::R2,
            hard_stops: vec![],
            required_reviews: vec![],
            approval_receipts: Vec::<VerdictReceiptRef>::new(),
            decision,
            valid_for_head_sha_only: true,
            rebind_on_train: true,
            expires_at: now + Duration::minutes(expires_in_minutes),
            created_at: now,
            signature: Signature::stub(),
        }
    }

    // -- Escalation dispatcher fake -------------------------------------

    struct CapturingDispatcher {
        calls: Arc<Mutex<Vec<(WebhookConfig, serde_json::Value)>>>,
        outcomes: Mutex<Vec<Result<u16, EscalationError>>>,
    }

    impl CapturingDispatcher {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                outcomes: Mutex::new(Vec::new()),
            })
        }
        fn calls(&self) -> Vec<(WebhookConfig, serde_json::Value)> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl EscalationDispatcher for CapturingDispatcher {
        async fn post(
            &self,
            webhook: &WebhookConfig,
            payload: serde_json::Value,
        ) -> Result<u16, EscalationError> {
            self.calls
                .lock()
                .unwrap()
                .push((webhook.clone(), payload.clone()));
            let mut outcomes = self.outcomes.lock().unwrap();
            if outcomes.is_empty() {
                Ok(200)
            } else {
                outcomes.remove(0)
            }
        }
    }

    // -- Test builders --------------------------------------------------

    fn signing_key() -> Arc<EdSigningKey> {
        Arc::new(EdSigningKey::generate("autonomy-daemon.test"))
    }

    fn escalation_cfg() -> EscalationConfig {
        EscalationConfig {
            enabled: true,
            on_events: vec!["require_human".into()],
            webhooks: vec![WebhookConfig {
                kind: EscalationKind::GenericJson,
                url_secret_name: "ESCALATION_URL".into(),
                channel: None,
                severity: None,
                headers: HashMap::new(),
            }],
        }
    }

    fn pr_summary(iid: &str, head_sha: &str) -> PrSummary {
        PrSummary {
            mr_iid: iid.into(),
            head_sha: head_sha.into(),
            target_branch: "main".into(),
            author: "octocat".into(),
            title: "test PR".into(),
            draft: false,
            labels: vec![],
        }
    }

    fn pr_state(
        iid: &str,
        head_sha: &str,
        base_sha: &str,
        policy_sha: Option<&str>,
    ) -> PrLiveState {
        PrLiveState {
            mr_iid: iid.into(),
            head_sha: head_sha.into(),
            target_branch: "main".into(),
            target_branch_sha: base_sha.into(),
            target_policy_sha: policy_sha.map(|s| s.to_string()),
            fetched_at: Utc::now(),
        }
    }

    struct Harness {
        daemon: Daemon,
        verdict_store: Arc<SqlVerdictStore>,
        ledger: SqlLedger,
        kill_bell: KillBell,
        dispatcher: Arc<CapturingDispatcher>,
        fake_host: Arc<FakeGitHost>,
        signing_key: Arc<EdSigningKey>,
    }

    impl Harness {
        async fn new(repos: Vec<&str>, escalation_enabled: bool) -> Self {
            let pool = fresh_db().await;
            let verdict_store = Arc::new(SqlVerdictStore::new(pool.clone()));
            let ledger = SqlLedger::new(pool.clone());
            let kill_bell = KillBell::new(pool.clone());
            let dispatcher = CapturingDispatcher::new();
            let fake_host = Arc::new(FakeGitHost::new());
            let signing_key = signing_key();
            let cfg = DaemonConfig {
                repos: repos
                    .into_iter()
                    .map(|s| RepoRef::parse(s).expect("repo slug"))
                    .collect(),
                interval_secs: 1,
                tick_once: true,
                kill_bell_check_enabled: true,
                escalation_enabled,
                auto_rejudge_enabled: false,
            };
            let daemon = Daemon::new(
                cfg,
                fake_host.clone() as Arc<dyn GitHost>,
                verdict_store.clone() as Arc<dyn VerdictStore>,
                ledger.clone(),
                kill_bell.clone(),
                escalation_cfg(),
                dispatcher.clone() as Arc<dyn EscalationDispatcher>,
                signing_key.clone(),
                None, // auto_rejudge_service — Wave 7 detect-only
            );
            Self {
                daemon,
                verdict_store,
                ledger,
                kill_bell,
                dispatcher,
                fake_host,
                signing_key,
            }
        }
    }

    // -- Tests ----------------------------------------------------------

    #[tokio::test]
    async fn tick_with_no_repos_returns_zero_scanned() {
        let h = Harness::new(vec![], true).await;
        let report = h.daemon.tick().await;
        assert_eq!(report.repos_scanned, 0);
        assert_eq!(report.verdicts_inspected, 0);
        assert!(report.rejudge_triggered.is_empty());
        assert!(report.errors.is_empty());
        assert!(!report.kill_bell_paused);
    }

    #[tokio::test]
    async fn tick_with_kill_bell_paused_marks_report_paused_but_still_scans() {
        let h = Harness::new(vec!["owner/repo"], true).await;
        // Pre-seed an active verdict + matching live state so the scan
        // would otherwise drift.
        let v = mint_verdict(
            "owner/repo",
            "!42",
            "aaaa1111",
            "cccc0001",
            GateDecision::AllowMerge,
            60,
        );
        h.verdict_store.save(&v).await.unwrap();
        let summary = pr_summary("!42", "differenthead");
        h.fake_host
            .open_prs
            .lock()
            .unwrap()
            .insert("owner/repo".into(), vec![summary]);
        h.fake_host.pr_states.lock().unwrap().insert(
            ("owner/repo".into(), "!42".into()),
            pr_state("!42", "differenthead", "basebase", None),
        );
        // Engage the bell.
        h.kill_bell
            .pause("freeze", "alice", 3600, &h.signing_key, Utc::now())
            .await
            .unwrap();

        let report = h.daemon.tick().await;

        assert!(report.kill_bell_paused, "report must surface paused state");
        assert_eq!(report.repos_scanned, 1, "scan continues under pause");
        assert_eq!(report.verdicts_inspected, 1, "verdict still inspected");
        // Wave 7 detect-only: trigger was detected (record pushed) but
        // *actions* (supersede + ledger + dispatch) MUST NOT fire.
        assert_eq!(report.rejudge_triggered.len(), 1);
        // No escalation while paused (Law 9).
        assert!(report.escalations_dispatched.is_empty());
        assert!(h.dispatcher.calls().is_empty());
        // Verdict must NOT be superseded (Kill Bell freezes mutations).
        let still_there = h
            .verdict_store
            .load_latest("owner/repo", Some("!42"))
            .await
            .unwrap();
        assert!(still_there.is_some(), "verdict must survive paused tick");
        // No invalidation entries in the ledger.
        let entries = h
            .ledger
            .list(&LedgerFilter {
                kind: Some(LedgerKind::MergePassportInvalidated),
                ..Default::default()
            })
            .await
            .unwrap();
        // The kill-bell pause itself appended an entry, but no
        // MergePassportInvalidated entries should exist.
        assert!(
            entries.is_empty(),
            "no MergePassportInvalidated entries while paused; got {entries:?}"
        );
    }

    #[tokio::test]
    async fn tick_with_no_open_prs_records_zero_inspected() {
        let h = Harness::new(vec!["owner/repo"], true).await;
        let report = h.daemon.tick().await;
        assert_eq!(report.repos_scanned, 1);
        assert_eq!(report.verdicts_inspected, 0);
        assert!(report.errors.is_empty());
    }

    #[tokio::test]
    async fn tick_with_open_pr_and_no_existing_verdict_skips_silently() {
        let h = Harness::new(vec!["owner/repo"], true).await;
        h.fake_host
            .open_prs
            .lock()
            .unwrap()
            .insert("owner/repo".into(), vec![pr_summary("!7", "headhead")]);
        // Note: get_pr_state is NOT seeded because the daemon should
        // short-circuit on "no verdict" BEFORE asking the host for state.
        let report = h.daemon.tick().await;
        assert_eq!(report.repos_scanned, 1);
        assert_eq!(
            report.verdicts_inspected, 0,
            "no verdict -> no inspection counted"
        );
        assert!(report.rejudge_triggered.is_empty());
        assert!(report.errors.is_empty(), "no error from missing state");
    }

    #[tokio::test]
    async fn tick_with_clean_pr_state_makes_no_changes() {
        let h = Harness::new(vec!["owner/repo"], true).await;
        let v = mint_verdict(
            "owner/repo",
            "!1",
            "abcd1234",
            "policy01",
            GateDecision::AllowMerge,
            60,
        );
        h.verdict_store.save(&v).await.unwrap();
        h.fake_host
            .open_prs
            .lock()
            .unwrap()
            .insert("owner/repo".into(), vec![pr_summary("!1", &v.head_sha)]);
        h.fake_host.pr_states.lock().unwrap().insert(
            ("owner/repo".into(), "!1".into()),
            pr_state("!1", &v.head_sha, "basebase", Some(&v.policy_sha)),
        );
        let report = h.daemon.tick().await;
        assert_eq!(report.verdicts_inspected, 1);
        assert!(report.rejudge_triggered.is_empty(), "no drift -> no record");
        assert!(report.escalations_dispatched.is_empty());
        assert!(report.errors.is_empty());
        // The verdict is still active (not superseded).
        let still = h
            .verdict_store
            .load_latest("owner/repo", Some("!1"))
            .await
            .unwrap();
        assert!(still.is_some());
    }

    #[tokio::test]
    async fn tick_with_head_sha_drift_records_rejudge_with_new_commit_trigger() {
        let h = Harness::new(vec!["owner/repo"], true).await;
        let v = mint_verdict(
            "owner/repo",
            "!1",
            "aaaa1111",
            "policy01",
            GateDecision::AllowMerge,
            60,
        );
        h.verdict_store.save(&v).await.unwrap();
        let new_head = "b".repeat(40);
        h.fake_host
            .open_prs
            .lock()
            .unwrap()
            .insert("owner/repo".into(), vec![pr_summary("!1", &new_head)]);
        h.fake_host.pr_states.lock().unwrap().insert(
            ("owner/repo".into(), "!1".into()),
            pr_state("!1", &new_head, "basebase", Some(&v.policy_sha)),
        );
        let report = h.daemon.tick().await;
        assert_eq!(report.rejudge_triggered.len(), 1);
        let rec = &report.rejudge_triggered[0];
        assert_eq!(rec.verdict_id_before, v.id);
        assert!(rec.verdict_id_after.is_none(), "Wave 7 is detect-only");
        assert!(rec.new_decision.is_none());
        assert!(
            rec.triggers.iter().any(|t| t == "new_commit_on_pr"),
            "got triggers={:?}",
            rec.triggers
        );
    }

    #[tokio::test]
    async fn tick_with_policy_sha_drift_records_rejudge_with_policy_trigger() {
        let h = Harness::new(vec!["owner/repo"], true).await;
        let v = mint_verdict(
            "owner/repo",
            "!1",
            "aaaa1111",
            "policy01",
            GateDecision::AllowMerge,
            60,
        );
        h.verdict_store.save(&v).await.unwrap();
        let new_policy = "e".repeat(40);
        h.fake_host
            .open_prs
            .lock()
            .unwrap()
            .insert("owner/repo".into(), vec![pr_summary("!1", &v.head_sha)]);
        h.fake_host.pr_states.lock().unwrap().insert(
            ("owner/repo".into(), "!1".into()),
            pr_state("!1", &v.head_sha, "basebase", Some(&new_policy)),
        );
        let report = h.daemon.tick().await;
        assert_eq!(report.rejudge_triggered.len(), 1);
        let triggers = &report.rejudge_triggered[0].triggers;
        assert!(
            triggers.iter().any(|t| t == "policy_change_on_target"),
            "got triggers={triggers:?}"
        );
    }

    #[tokio::test]
    async fn tick_with_expired_verdict_records_rejudge_with_ttl_trigger() {
        let h = Harness::new(vec!["owner/repo"], true).await;
        // Verdict expires in the past — TTL trigger MUST fire.
        let v = mint_verdict(
            "owner/repo",
            "!1",
            "aaaa1111",
            "policy01",
            GateDecision::AllowMerge,
            -10, // expired 10 minutes ago
        );
        h.verdict_store.save(&v).await.unwrap();
        h.fake_host
            .open_prs
            .lock()
            .unwrap()
            .insert("owner/repo".into(), vec![pr_summary("!1", &v.head_sha)]);
        h.fake_host.pr_states.lock().unwrap().insert(
            ("owner/repo".into(), "!1".into()),
            pr_state("!1", &v.head_sha, "basebase", Some(&v.policy_sha)),
        );
        let report = h.daemon.tick().await;
        assert_eq!(report.rejudge_triggered.len(), 1);
        let triggers = &report.rejudge_triggered[0].triggers;
        assert!(
            triggers.iter().any(|t| t == "verdict_ttl_expired"),
            "got triggers={triggers:?}"
        );
    }

    #[tokio::test]
    async fn tick_supersedes_old_verdict_on_drift() {
        let h = Harness::new(vec!["owner/repo"], true).await;
        let v = mint_verdict(
            "owner/repo",
            "!1",
            "aaaa1111",
            "policy01",
            GateDecision::AllowMerge,
            60,
        );
        h.verdict_store.save(&v).await.unwrap();
        let new_head = "b".repeat(40);
        h.fake_host
            .open_prs
            .lock()
            .unwrap()
            .insert("owner/repo".into(), vec![pr_summary("!1", &new_head)]);
        h.fake_host.pr_states.lock().unwrap().insert(
            ("owner/repo".into(), "!1".into()),
            pr_state("!1", &new_head, "basebase", Some(&v.policy_sha)),
        );
        // Before the tick, the verdict is loadable.
        assert!(
            h.verdict_store
                .load_latest("owner/repo", Some("!1"))
                .await
                .unwrap()
                .is_some()
        );
        let _ = h.daemon.tick().await;
        // After the tick, the verdict is superseded → load_latest returns None.
        let after = h
            .verdict_store
            .load_latest("owner/repo", Some("!1"))
            .await
            .unwrap();
        assert!(
            after.is_none(),
            "drift must supersede the stale verdict; got {after:?}"
        );
    }

    #[tokio::test]
    async fn tick_appends_signed_ledger_entry_on_drift() {
        let h = Harness::new(vec!["owner/repo"], true).await;
        let v = mint_verdict(
            "owner/repo",
            "!1",
            "aaaa1111",
            "policy01",
            GateDecision::AllowMerge,
            60,
        );
        h.verdict_store.save(&v).await.unwrap();
        let new_head = "b".repeat(40);
        h.fake_host
            .open_prs
            .lock()
            .unwrap()
            .insert("owner/repo".into(), vec![pr_summary("!1", &new_head)]);
        h.fake_host.pr_states.lock().unwrap().insert(
            ("owner/repo".into(), "!1".into()),
            pr_state("!1", &new_head, "basebase", Some(&v.policy_sha)),
        );
        let _ = h.daemon.tick().await;
        let entries = h
            .ledger
            .list(&LedgerFilter {
                kind: Some(LedgerKind::MergePassportInvalidated),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(entries.len(), 1, "exactly one invalidation entry");
        let entry = &entries[0];
        assert_eq!(entry.kind, LedgerKind::MergePassportInvalidated);
        assert_eq!(entry.subject_id, v.id);
        assert_eq!(entry.repo.as_deref(), Some("owner/repo"));
        // SqlLedger refuses stub/hmac, so a successful append means the
        // signature is ed25519. Spot-check.
        assert_eq!(entry.signature.algo, "ed25519");
        assert_ne!(entry.signature.value, "0".repeat(64));
        // Payload carries the trigger names so consumers can route.
        let triggers = entry.payload["triggers"]
            .as_array()
            .expect("triggers array");
        let names: Vec<&str> = triggers.iter().filter_map(|v| v.as_str()).collect();
        assert!(
            names.contains(&"new_commit_on_pr"),
            "expected new_commit_on_pr in payload triggers; got {names:?}"
        );
        // Wave-scope marker so replay tooling knows this was detect-only.
        assert_eq!(entry.payload["wave_scope"], "detect_only");
    }

    #[tokio::test]
    async fn tick_dispatches_escalation_when_enabled_on_drift() {
        let h = Harness::new(vec!["owner/repo"], true).await;
        let v = mint_verdict(
            "owner/repo",
            "!1",
            "aaaa1111",
            "policy01",
            GateDecision::AllowMerge,
            60,
        );
        h.verdict_store.save(&v).await.unwrap();
        let new_head = "b".repeat(40);
        h.fake_host
            .open_prs
            .lock()
            .unwrap()
            .insert("owner/repo".into(), vec![pr_summary("!1", &new_head)]);
        h.fake_host.pr_states.lock().unwrap().insert(
            ("owner/repo".into(), "!1".into()),
            pr_state("!1", &new_head, "basebase", Some(&v.policy_sha)),
        );
        let report = h.daemon.tick().await;
        assert_eq!(report.escalations_dispatched.len(), 1);
        let rec = &report.escalations_dispatched[0];
        assert_eq!(rec.event_name, "require_human");
        assert_eq!(rec.mr_iid.as_deref(), Some("!1"));
        assert_eq!(rec.results.len(), 1);
        assert_eq!(rec.results[0].kind, "generic_json");
        assert_eq!(rec.results[0].status, Some(200));
        assert!(rec.results[0].error.is_none());
        // The dispatcher actually saw a call.
        assert_eq!(h.dispatcher.calls().len(), 1);
    }

    #[tokio::test]
    async fn tick_does_not_dispatch_escalation_when_disabled() {
        let h = Harness::new(vec!["owner/repo"], false /* escalation_enabled */).await;
        let v = mint_verdict(
            "owner/repo",
            "!1",
            "aaaa1111",
            "policy01",
            GateDecision::AllowMerge,
            60,
        );
        h.verdict_store.save(&v).await.unwrap();
        let new_head = "b".repeat(40);
        h.fake_host
            .open_prs
            .lock()
            .unwrap()
            .insert("owner/repo".into(), vec![pr_summary("!1", &new_head)]);
        h.fake_host.pr_states.lock().unwrap().insert(
            ("owner/repo".into(), "!1".into()),
            pr_state("!1", &new_head, "basebase", Some(&v.policy_sha)),
        );
        let report = h.daemon.tick().await;
        assert_eq!(report.rejudge_triggered.len(), 1, "drift still detected");
        assert!(
            report.escalations_dispatched.is_empty(),
            "escalation disabled -> no record pushed"
        );
        assert!(h.dispatcher.calls().is_empty());
    }

    #[tokio::test]
    async fn tick_does_not_dispatch_escalation_when_kill_bell_paused() {
        // Law 9 emergency stop: even with escalation_enabled, a paused bell
        // must block dispatch.
        let h = Harness::new(vec!["owner/repo"], true).await;
        h.kill_bell
            .pause("freeze", "alice", 3600, &h.signing_key, Utc::now())
            .await
            .unwrap();
        let v = mint_verdict(
            "owner/repo",
            "!1",
            "aaaa1111",
            "policy01",
            GateDecision::AllowMerge,
            60,
        );
        h.verdict_store.save(&v).await.unwrap();
        let new_head = "b".repeat(40);
        h.fake_host
            .open_prs
            .lock()
            .unwrap()
            .insert("owner/repo".into(), vec![pr_summary("!1", &new_head)]);
        h.fake_host.pr_states.lock().unwrap().insert(
            ("owner/repo".into(), "!1".into()),
            pr_state("!1", &new_head, "basebase", Some(&v.policy_sha)),
        );
        let report = h.daemon.tick().await;
        assert!(report.kill_bell_paused);
        assert!(report.escalations_dispatched.is_empty());
        assert!(h.dispatcher.calls().is_empty());
    }

    #[tokio::test]
    async fn tick_continues_after_per_pr_error_and_records_in_errors() {
        // Two PRs in one repo. The first one's `get_pr_state` is forced to
        // fail; the second must still be processed.
        let h = Harness::new(vec!["owner/repo"], true).await;
        // Seed verdicts for BOTH so each one would be "inspected" if state
        // resolves.
        let v1 = mint_verdict(
            "owner/repo",
            "!1",
            "aaaa1111",
            "policy01",
            GateDecision::AllowMerge,
            60,
        );
        let v2 = mint_verdict(
            "owner/repo",
            "!2",
            "bbbb2222",
            "policy01",
            GateDecision::AllowMerge,
            60,
        );
        h.verdict_store.save(&v1).await.unwrap();
        h.verdict_store.save(&v2).await.unwrap();
        h.fake_host.open_prs.lock().unwrap().insert(
            "owner/repo".into(),
            vec![pr_summary("!1", &v1.head_sha), pr_summary("!2", "cccc3333")],
        );
        // Only seed state for !2 (drift). !1 has no state seeded, so the
        // fake will return `NotImplemented`.
        h.fake_host.pr_states.lock().unwrap().insert(
            ("owner/repo".into(), "!2".into()),
            pr_state("!2", "cccc3333", "basebase", Some(&v2.policy_sha)),
        );
        let report = h.daemon.tick().await;
        // !1 inspected + errored; !2 inspected + drifted.
        assert_eq!(report.verdicts_inspected, 2);
        // The drift on !2 still recorded.
        assert_eq!(report.rejudge_triggered.len(), 1);
        assert_eq!(report.rejudge_triggered[0].mr_iid, "!2");
        // The error for !1 surfaces in `errors`.
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.stage == "get_pr_state" && e.mr_iid.as_deref() == Some("!1")),
            "expected get_pr_state error for !1; got errors={:?}",
            report.errors
        );
    }

    #[tokio::test]
    async fn tick_with_multiple_repos_scans_each_independently() {
        let h = Harness::new(vec!["owner/alpha", "owner/beta"], true).await;
        // Drift on alpha; clean on beta.
        let v_alpha = mint_verdict(
            "owner/alpha",
            "!1",
            "aaaa1111",
            "policy01",
            GateDecision::AllowMerge,
            60,
        );
        let v_beta = mint_verdict(
            "owner/beta",
            "!9",
            "bbbb9999",
            "policy01",
            GateDecision::AllowMerge,
            60,
        );
        h.verdict_store.save(&v_alpha).await.unwrap();
        h.verdict_store.save(&v_beta).await.unwrap();
        let new_alpha_head = "d".repeat(40);
        h.fake_host.open_prs.lock().unwrap().insert(
            "owner/alpha".into(),
            vec![pr_summary("!1", &new_alpha_head)],
        );
        h.fake_host.open_prs.lock().unwrap().insert(
            "owner/beta".into(),
            vec![pr_summary("!9", &v_beta.head_sha)],
        );
        h.fake_host.pr_states.lock().unwrap().insert(
            ("owner/alpha".into(), "!1".into()),
            pr_state("!1", &new_alpha_head, "basebase", Some(&v_alpha.policy_sha)),
        );
        h.fake_host.pr_states.lock().unwrap().insert(
            ("owner/beta".into(), "!9".into()),
            pr_state("!9", &v_beta.head_sha, "basebase", Some(&v_beta.policy_sha)),
        );
        let report = h.daemon.tick().await;
        assert_eq!(report.repos_scanned, 2);
        assert_eq!(report.verdicts_inspected, 2);
        // Exactly one drift (alpha); beta is clean.
        assert_eq!(report.rejudge_triggered.len(), 1);
        assert_eq!(report.rejudge_triggered[0].repo, "owner/alpha");
        assert!(report.errors.is_empty());
    }

    #[tokio::test]
    async fn run_with_tick_once_returns_after_single_tick() {
        // With `tick_once = true`, run() must not loop and must return Ok.
        let h = Harness::new(vec!["owner/repo"], true).await;
        // Sanity guard: complete within a generous timeout. If run() ever
        // regresses into looping, this `.await` would hang and the timeout
        // kicks in to fail loudly.
        let out = tokio::time::timeout(std::time::Duration::from_secs(5), h.daemon.run()).await;
        assert!(out.is_ok(), "run(tick_once) must return promptly");
        assert!(out.unwrap().is_ok(), "run() must return Ok");
    }

    #[tokio::test]
    async fn tick_records_continue_after_list_open_prs_error() {
        // Bonus coverage: a failure on list_open_prs for one repo must not
        // abort scanning of the other repos. (Defensive against the daemon
        // ever changing to short-circuit on the first repo failure.)
        let h = Harness::new(vec!["owner/alpha", "owner/beta"], true).await;
        // Force the FIRST list_open_prs call (alpha) to fail. The fake fires
        // once and clears, so beta proceeds normally.
        *h.fake_host.fail_on.lock().unwrap() = Some("list_open_prs".into());
        let v = mint_verdict(
            "owner/beta",
            "!1",
            "abcdabcd",
            "policy01",
            GateDecision::AllowMerge,
            60,
        );
        h.verdict_store.save(&v).await.unwrap();
        h.fake_host
            .open_prs
            .lock()
            .unwrap()
            .insert("owner/beta".into(), vec![pr_summary("!1", &v.head_sha)]);
        h.fake_host.pr_states.lock().unwrap().insert(
            ("owner/beta".into(), "!1".into()),
            pr_state("!1", &v.head_sha, "basebase", Some(&v.policy_sha)),
        );
        let report = h.daemon.tick().await;
        // alpha errored; beta scanned cleanly (verdict matched live).
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.stage == "list_open_prs" && e.repo.as_deref() == Some("owner/alpha")),
            "alpha must surface a list_open_prs error; got {:?}",
            report.errors
        );
        assert_eq!(
            report.verdicts_inspected, 1,
            "beta must still inspect its verdict"
        );
        assert!(report.rejudge_triggered.is_empty(), "beta is clean");
    }

    // -----------------------------------------------------------------------
    // Wave 8.C — auto-rejudge integration tests.
    // -----------------------------------------------------------------------

    mod auto_rejudge {
        use super::*;
        use crate::autonomy::auto_rejudge::AutoRejudgeService;
        use crate::autonomy::auto_rejudge::test_helpers::{
            CannedOrchestrator, FakeEvidencePackBuilder, bound_receipt, canned_pack,
        };
        use crate::autonomy::policy_yaml::PolicyBundle;
        use crate::autonomy::types::{ReviewDecision, ReviewerRole, RiskTier};
        use std::path::Path;

        fn policy() -> Arc<PolicyBundle> {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".autonomy/policies");
            Arc::new(PolicyBundle::from_dir(&dir).expect("policy bundle loads"))
        }

        /// Build a daemon harness with an in-process auto-rejudge service
        /// wired through. `auto_rejudge_enabled` toggles the gate; if a
        /// custom service is provided it overrides the default clean-pass
        /// service used by the other tests.
        async fn harness_with_rejudge(
            repos: Vec<&str>,
            auto_rejudge_enabled: bool,
            service: Option<Arc<AutoRejudgeService>>,
        ) -> Harness {
            let pool = fresh_db().await;
            let verdict_store = Arc::new(SqlVerdictStore::new(pool.clone()));
            let ledger = SqlLedger::new(pool.clone());
            let kill_bell = KillBell::new(pool.clone());
            let dispatcher = CapturingDispatcher::new();
            let fake_host = Arc::new(FakeGitHost::new());
            let signing_key = signing_key();

            // Default service: clean R2 pack with 2 passing reviewers. Tests
            // that need a different shape pass their own `service`.
            let svc = service.unwrap_or_else(|| {
                let pack = canned_pack(RiskTier::R2);
                let receipts = vec![
                    bound_receipt(
                        &pack,
                        ReviewerRole::TestIntegrity,
                        "tester.v1",
                        ReviewDecision::Pass,
                    ),
                    bound_receipt(
                        &pack,
                        ReviewerRole::Security,
                        "sec.v1",
                        ReviewDecision::Pass,
                    ),
                ];
                Arc::new(AutoRejudgeService::new(
                    FakeEvidencePackBuilder::with_pack(pack),
                    CannedOrchestrator::with_receipts(receipts),
                    verdict_store.clone() as Arc<dyn VerdictStore>,
                    ledger.clone(),
                    signing_key.clone(),
                    policy(),
                ))
            });

            let cfg = DaemonConfig {
                repos: repos
                    .into_iter()
                    .map(|s| RepoRef::parse(s).expect("repo slug"))
                    .collect(),
                interval_secs: 1,
                tick_once: true,
                kill_bell_check_enabled: true,
                escalation_enabled: true,
                auto_rejudge_enabled,
            };
            let daemon = Daemon::new(
                cfg,
                fake_host.clone() as Arc<dyn GitHost>,
                verdict_store.clone() as Arc<dyn VerdictStore>,
                ledger.clone(),
                kill_bell.clone(),
                escalation_cfg(),
                dispatcher.clone() as Arc<dyn EscalationDispatcher>,
                signing_key.clone(),
                Some(svc),
            );
            Harness {
                daemon,
                verdict_store,
                ledger,
                kill_bell,
                dispatcher,
                fake_host,
                signing_key,
            }
        }

        /// Seed a drift scenario: one verdict + one PR whose head_sha has
        /// moved. The rejudge pipeline should fire on this drift.
        async fn seed_drift(h: &Harness, mr: &str) -> VibeGateVerdict {
            let v = mint_verdict(
                "owner/repo",
                mr,
                "aaaa1111",
                "policy01",
                GateDecision::AllowMerge,
                60,
            );
            h.verdict_store.save(&v).await.unwrap();
            let new_head = "b".repeat(40);
            h.fake_host
                .open_prs
                .lock()
                .unwrap()
                .insert("owner/repo".into(), vec![pr_summary(mr, &new_head)]);
            h.fake_host.pr_states.lock().unwrap().insert(
                ("owner/repo".into(), mr.into()),
                pr_state(mr, &new_head, "basebase", Some(&v.policy_sha)),
            );
            v
        }

        #[tokio::test]
        async fn tick_with_auto_rejudge_enabled_populates_new_decision() {
            let h = harness_with_rejudge(vec!["owner/repo"], true, None).await;
            let _v = seed_drift(&h, "!1").await;

            let report = h.daemon.tick().await;

            assert_eq!(report.rejudge_triggered.len(), 1, "drift was detected");
            let rec = &report.rejudge_triggered[0];
            assert!(
                rec.new_decision.is_some(),
                "auto_rejudge_enabled => new_decision must be populated; got {rec:?}"
            );
            assert!(
                rec.verdict_id_after.is_some(),
                "verdict_id_after must be populated; got {rec:?}"
            );
            assert_eq!(rec.new_decision, Some(GateDecision::AllowMerge));
            // No auto_rejudge error in the report — happy path.
            assert!(
                !report.errors.iter().any(|e| e.stage == "auto_rejudge"),
                "no auto_rejudge errors expected; got {:?}",
                report.errors
            );
        }

        #[tokio::test]
        async fn tick_with_auto_rejudge_disabled_preserves_wave_7_none_behavior() {
            // Same seed as the enabled test, but auto_rejudge_enabled = false.
            // Drift must still be DETECTED (Wave 7 behavior intact); the
            // rejudge service must NOT run; new_decision must stay None.
            let h = harness_with_rejudge(vec!["owner/repo"], false, None).await;
            let _v = seed_drift(&h, "!1").await;

            let report = h.daemon.tick().await;

            assert_eq!(report.rejudge_triggered.len(), 1, "drift still detected");
            let rec = &report.rejudge_triggered[0];
            assert!(
                rec.new_decision.is_none(),
                "auto_rejudge disabled => new_decision must be None (Wave 7 contract)"
            );
            assert!(
                rec.verdict_id_after.is_none(),
                "verdict_id_after must be None (Wave 7 contract)"
            );
            assert!(
                !report.errors.iter().any(|e| e.stage == "auto_rejudge"),
                "auto_rejudge stage must not appear when disabled"
            );
        }

        #[tokio::test]
        async fn tick_auto_rejudge_error_records_tick_error_but_keeps_drift_record() {
            // Wire a service whose pack-builder always fails. Drift must
            // still be detected + recorded (with new_decision = None), but
            // a TickError with stage = "auto_rejudge" must surface so the
            // operator knows why the verdict didn't refresh.
            let pool = fresh_db().await;
            let verdict_store = Arc::new(SqlVerdictStore::new(pool.clone()));
            let ledger = SqlLedger::new(pool.clone());
            let kill_bell = KillBell::new(pool.clone());
            let dispatcher = CapturingDispatcher::new();
            let fake_host = Arc::new(FakeGitHost::new());
            let signing_key = signing_key();
            let failing_svc = Arc::new(AutoRejudgeService::new(
                FakeEvidencePackBuilder::with_error("simulated host outage"),
                CannedOrchestrator::with_receipts(vec![]),
                verdict_store.clone() as Arc<dyn VerdictStore>,
                ledger.clone(),
                signing_key.clone(),
                policy(),
            ));
            let cfg = DaemonConfig {
                repos: vec![RepoRef::parse("owner/repo").unwrap()],
                interval_secs: 1,
                tick_once: true,
                kill_bell_check_enabled: true,
                escalation_enabled: true,
                auto_rejudge_enabled: true,
            };
            let daemon = Daemon::new(
                cfg,
                fake_host.clone() as Arc<dyn GitHost>,
                verdict_store.clone() as Arc<dyn VerdictStore>,
                ledger.clone(),
                kill_bell.clone(),
                escalation_cfg(),
                dispatcher.clone() as Arc<dyn EscalationDispatcher>,
                signing_key.clone(),
                Some(failing_svc),
            );
            let h = Harness {
                daemon,
                verdict_store,
                ledger,
                kill_bell,
                dispatcher,
                fake_host,
                signing_key,
            };
            let _v = seed_drift(&h, "!1").await;

            let report = h.daemon.tick().await;

            // Drift record IS still present...
            assert_eq!(
                report.rejudge_triggered.len(),
                1,
                "drift record must survive auto_rejudge failure"
            );
            let rec = &report.rejudge_triggered[0];
            assert!(
                rec.new_decision.is_none(),
                "failed rejudge => no new_decision"
            );
            assert!(
                rec.verdict_id_after.is_none(),
                "failed rejudge => no verdict_id_after"
            );
            // ...AND a TickError with the auto_rejudge stage is surfaced.
            let auto_err = report
                .errors
                .iter()
                .find(|e| e.stage == "auto_rejudge")
                .expect("auto_rejudge stage TickError must be recorded");
            assert_eq!(auto_err.repo.as_deref(), Some("owner/repo"));
            assert_eq!(auto_err.mr_iid.as_deref(), Some("!1"));
            assert!(
                auto_err.message.contains("simulated host outage"),
                "underlying error must surface in message; got {}",
                auto_err.message
            );
        }
    }
}

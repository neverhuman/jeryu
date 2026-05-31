//! End-to-end integration test: drive the full Evidence Gate through the public
//! API and the object-safe seams, with in-memory implementations only.
//!
//! Covers: build pack -> judge fusion -> verdict store save/supersede -> signed
//! ledger append -> kill-bell downgrade -> escalation fan-out -> replay.

use std::sync::Arc;

use chrono::{Duration, Utc};
use jeryu_autonomy::{
    AgentApprovalReceipt, Clock, ConditionRegistry, EdSigningKey, EscalationConfig,
    EscalationEvent, EscalationKind, EscalationSink, EvidenceInputs, FixedClock, GateDecision,
    JudgeInputs, KillBell, LedgerFilter, MemoryLedger, MemoryVerdictStore, PolicyBundle,
    ReviewDecision, ReviewerRole, RiskTier, RollbackSection, RollbackStrategy, ScanOutcome,
    SchemaTag, SecuritySection, Signature, SupplyChainSection, TestsSection, VerdictLedger,
    VerdictStore, WebhookConfig, build_evidence_pack, build_payload, dispatch_all, judge,
    policy_yaml, replay_subject, sign_entry, verdict_issued_entry, verify_sha_binding,
};

fn bundle() -> PolicyBundle {
    policy_yaml::fixtures::default_bundle()
}

fn signed_pack(
    repo: &str,
    head: &str,
    policy: &str,
    secret_failed: bool,
) -> jeryu_autonomy::EvidencePack {
    let base = "b".repeat(40);
    let mut p = build_evidence_pack(EvidenceInputs {
        repo,
        source_branch: "jeryu-pr-7",
        target_branch: "main",
        head_sha: head,
        base_sha: &base,
        policy_sha: policy,
        author_agent: Some("builder.x"),
        intent_id: None,
        risk: RiskTier::R2,
        changed_files: vec![],
        claims: vec![],
        tests: TestsSection {
            targeted: vec![],
            full_required: false,
            skipped: vec![],
            coverage_delta: None,
        },
        security: SecuritySection {
            sast: ScanOutcome::Passed,
            dependency_scan: ScanOutcome::Passed,
            secret_scan: if secret_failed {
                ScanOutcome::Failed
            } else {
                ScanOutcome::Passed
            },
        },
        supply_chain: SupplyChainSection::default(),
        rollback: RollbackSection {
            strategy: RollbackStrategy::RevertCommit,
            feature_flag: None,
            data_migration_reversible: Some(true),
        },
        gate_receipts: vec![],
        ci_status: vec![],
    });
    p.signature = Some(Signature {
        key_id: "evidence-builder.v1".into(),
        algo: "ed25519".into(),
        value: "0".repeat(128),
    });
    p
}

fn receipt(
    role: ReviewerRole,
    agent: &str,
    pack: &jeryu_autonomy::EvidencePack,
) -> AgentApprovalReceipt {
    AgentApprovalReceipt {
        schema: SchemaTag::new(),
        id: format!("aar_{agent}"),
        evidence_pack_id: pack.id.clone(),
        role,
        agent_id: agent.into(),
        prompt_sha: None,
        provider: None,
        model: None,
        temperature: None,
        seed: None,
        raw_response_sha: Some("sha256:beef".into()),
        head_sha: pack.head_sha.clone(),
        policy_sha: pack.policy_sha.clone(),
        decision: ReviewDecision::Pass,
        reason: None,
        findings: vec![],
        not_author: true,
        tokens: Default::default(),
        created_at: Utc::now(),
        signature: Signature::unsigned(),
    }
}

#[tokio::test]
async fn full_gate_allow_merge_path_records_signed_ledger_and_replays() {
    let policy = bundle();
    let head = "a".repeat(40);
    let pol = "c".repeat(40);
    let pack = signed_pack("owner/repo", &head, &pol, false);

    // SHA-binding holds for receipts minted against this pack head.
    let receipts = vec![
        receipt(ReviewerRole::Security, "sec.v1", &pack),
        receipt(ReviewerRole::TestIntegrity, "test.v1", &pack),
    ];
    for r in &receipts {
        assert!(verify_sha_binding(&pack, r).is_ok());
    }

    let out = judge(JudgeInputs {
        pack: &pack,
        receipts: &receipts,
        policy: &policy,
        repo: "owner/repo",
        target_branch: "main",
        pull_request: Some("7"),
        author_agent: Some("builder.x"),
        external_hard_stops: &[],
    });
    assert_eq!(out.verdict.decision, GateDecision::AllowMerge);

    // Persist the verdict and append a signed ledger entry.
    let store = MemoryVerdictStore::new();
    store.save(&out.verdict).await.unwrap();
    let now = out.verdict.created_at;
    assert_eq!(store.list_active(now).await.unwrap().len(), 1);

    let ledger = MemoryLedger::new();
    let key = EdSigningKey::generate("judge.v1");
    let mut entry = verdict_issued_entry(&out.verdict, "judge.v1");
    sign_entry(&mut entry, &key);
    ledger.append(&entry).await.unwrap();

    // Replay reconstructs the verdict in the timeline with a clean signature trail.
    let report = replay_subject(&ledger, &out.verdict.id).await.unwrap();
    assert_eq!(report.summary.verdicts_issued, 1);
    assert_eq!(report.summary.non_ed25519_signature_count, 0);
}

#[tokio::test]
async fn hard_stop_veto_beats_unanimous_approval() {
    let policy = bundle();
    let head = "a".repeat(40);
    let pol = "c".repeat(40);
    // secret_scan failed → the registry's secret_scan_failed hard stop fires.
    let pack = signed_pack("owner/repo", &head, &pol, true);
    let receipts = vec![
        receipt(ReviewerRole::Security, "sec.v1", &pack),
        receipt(ReviewerRole::TestIntegrity, "test.v1", &pack),
    ];
    let out = judge(JudgeInputs {
        pack: &pack,
        receipts: &receipts,
        policy: &policy,
        repo: "owner/repo",
        target_branch: "main",
        pull_request: Some("7"),
        author_agent: Some("builder.x"),
        external_hard_stops: &[],
    });
    assert_eq!(
        out.verdict.decision,
        GateDecision::Reject,
        "veto > approval"
    );
    assert!(
        out.verdict
            .hard_stops
            .iter()
            .any(|n| n == "secret_scan_failed")
    );

    // And the registry itself reports the same hard stop directly.
    let reg = ConditionRegistry::default();
    let hits = reg.evaluate(&["secret_scan_failed".into()], &pack, &receipts);
    assert_eq!(hits.len(), 1);
}

#[tokio::test]
async fn kill_bell_downgrades_allow_merge_to_require_human() {
    let ledger: Arc<dyn VerdictLedger> = Arc::new(MemoryLedger::new());
    let bell = KillBell::new(ledger.clone());
    let clock = FixedClock::new(Utc::now());
    let key = EdSigningKey::generate("operator.alice");

    // Engage the bell.
    bell.pause("incident", "alice", 3600, &key, clock.now())
        .await
        .unwrap();

    // A would-be AllowMerge downgrades while paused.
    let (decision, why) = bell
        .downgrade_if_paused(GateDecision::AllowMerge, clock.now())
        .await
        .unwrap();
    assert_eq!(decision, GateDecision::RequireHuman);
    assert!(why.unwrap().contains("incident"));

    // The pause left a signed KillBellEngaged ledger entry.
    let entries = ledger
        .list(&LedgerFilter {
            kind: Some(jeryu_autonomy::LedgerKind::KillBellEngaged),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].signature.algo, "ed25519");

    // After the TTL elapses, the bell auto-arms and decisions pass through.
    clock.advance(Duration::seconds(3601));
    let (decision, why) = bell
        .downgrade_if_paused(GateDecision::AllowMerge, clock.now())
        .await
        .unwrap();
    assert_eq!(
        decision,
        GateDecision::AllowMerge,
        "TTL auto-arm releases the pause"
    );
    assert!(why.is_none());
}

#[tokio::test]
async fn require_human_escalates_to_all_webhooks() {
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct CountingSink {
        calls: Mutex<usize>,
    }
    #[async_trait]
    impl EscalationSink for CountingSink {
        async fn deliver(
            &self,
            _wh: &WebhookConfig,
            _payload: serde_json::Value,
        ) -> Result<u16, jeryu_autonomy::EscalationError> {
            *self.calls.lock().unwrap() += 1;
            Ok(200)
        }
    }

    let policy = bundle();
    let head = "a".repeat(40);
    let pol = "c".repeat(40);
    // R4 → human_required → RequireHuman with no receipts.
    let mut pack = signed_pack("owner/repo", &head, &pol, false);
    pack.risk = RiskTier::R4;
    let out = judge(JudgeInputs::new(&pack, &[], &policy, "owner/repo", "main"));
    assert_eq!(out.verdict.decision, GateDecision::RequireHuman);

    let event = EscalationEvent::RequireHuman {
        verdict: Box::new(out.verdict),
    };
    let cfg = EscalationConfig {
        enabled: true,
        on_events: vec!["require_human".into()],
        webhooks: vec![
            WebhookConfig {
                kind: EscalationKind::Slack,
                url_secret_name: "SLACK".into(),
                channel: None,
                severity: None,
                headers: Default::default(),
            },
            WebhookConfig {
                kind: EscalationKind::GenericJson,
                url_secret_name: "GEN".into(),
                channel: None,
                severity: None,
                headers: Default::default(),
            },
        ],
    };
    let sink = CountingSink {
        calls: Mutex::new(0),
    };
    let results = dispatch_all(&cfg, &event, &sink).await;
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.error.is_none()));
    assert_eq!(*sink.calls.lock().unwrap(), 2);

    // Payload shaping is stable.
    let p = build_payload(&event, EscalationKind::Slack);
    assert!(p["text"].as_str().unwrap().contains("RequireHuman"));
}

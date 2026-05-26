use chrono::Utc;
use ratatui::{buffer::Buffer, widgets::Widget};

use super::*;
use crate::{
    api::{
        actions::{ActionReceipt, ActionStatus},
        entity::{EntityDetail, EntityKind, EntityRef, Severity},
        events::{TuiEvent, TuiEventKind},
        inspection::{EventPage, InspectionEnvelope, ProofDetail},
    },
    tui::{
        app::{
            reducer::AppIntent,
            state::{AppRoute, FlightDeckState},
        },
        theme::{ProofConfidence, TerminalCaps, Theme},
        widgets::shared::CanonicalSize,
    },
};

fn render_text(size: CanonicalSize) -> String {
    let fixture = fixture_input();
    let theme = Theme::dark();
    let area = size.area();
    let mut buffer = Buffer::empty(area);
    EvidenceLens::new(fixture.input(), &theme, TerminalCaps::ascii()).render(area, &mut buffer);
    buffer
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[test]
fn proof_search_includes_known_and_event_referenced_missing_proofs() {
    let fixture = fixture_input();
    let hits = fixture.input().proof_hits();

    assert_eq!(hits.len(), 2);
    assert!(hits.iter().any(|hit| hit.proof_id == "proof/job-14445"));
    let missing = hits
        .iter()
        .find(|hit| hit.proof_id == "proof/missing")
        .expect("missing event proof should be surfaced");
    assert_eq!(missing.confidence, ProofConfidence::Missing);
    assert_eq!(missing.source, data::ProofHitSource::EventReference);
}

#[test]
fn entity_filter_and_query_reduce_evidence_hits() {
    let mut fixture = fixture_input();
    fixture.search.selected_entity = Some(entity());

    let input = fixture.input();
    assert_eq!(input.proof_hits().len(), 2);
    assert_eq!(input.receipt_hits().len(), 1);

    fixture.search.query = "capsule".into();
    let input = fixture.input();
    assert_eq!(input.proof_hits().len(), 1);
}

#[test]
fn entity_graph_links_entities_proofs_and_receipts() {
    let fixture = fixture_input();
    let graph = build_entity_proof_graph(fixture.input());

    assert!(
        graph
            .nodes
            .iter()
            .any(|node| node.kind == EvidenceGraphNodeKind::Entity)
    );
    assert!(
        graph
            .nodes
            .iter()
            .any(|node| node.kind == EvidenceGraphNodeKind::Proof)
    );
    assert!(
        graph
            .nodes
            .iter()
            .any(|node| node.kind == EvidenceGraphNodeKind::Receipt)
    );
    assert!(graph.edges.iter().any(|edge| edge.label == "supports"));
    assert!(graph.edges.iter().any(|edge| edge.label == "created"));
}

#[test]
fn nav_activation_returns_intents_without_mutating_state() {
    let state = FlightDeckState::default();
    let fixture = fixture_input();

    assert_eq!(
        activate_pane(EvidencePane::Timeline, fixture.input(), &state),
        EvidenceNavOutcome::Intent(AppIntent::Navigate(AppRoute::Proof(
            "proof/job-14445".into()
        )))
    );
    assert!(matches!(
        activate_pane(EvidencePane::Graph, fixture.input(), &state),
        EvidenceNavOutcome::Intent(AppIntent::Navigate(AppRoute::Entity(_)))
    ));
    assert_eq!(
        activate_pane(EvidencePane::Receipts, fixture.input(), &state),
        EvidenceNavOutcome::Intent(AppIntent::ActionReceipt {
            receipt_id: "receipt/run-tests-1".into()
        })
    );
}

#[test]
fn bundle_preview_redacts_sensitive_text() {
    let fixture = fixture_input();
    let bundle = fixture.input().bundle_preview();
    let text = bundle.line_items.join("\n");

    assert!(text.contains("[REDACTED]"));
    assert!(!text.contains("abc123"));
    assert_eq!(redact_bundle_text("password=hunter2"), "[REDACTED]");
}

#[test]
fn evidence_lens_renders_at_canonical_sizes() {
    for size in [
        CanonicalSize::Compact,
        CanonicalSize::Standard,
        CanonicalSize::Wide,
    ] {
        let text = render_text(size);
        assert!(text.contains("Evidence"));
    }

    let wide = render_text(CanonicalSize::Wide);
    assert!(wide.contains("Proof Search"));
    assert!(wide.contains("Entity Proof Graph"));
    assert!(wide.contains("Receipts"));
    assert!(wide.contains("Redacted Bundle"));
}

#[test]
fn nav_focus_moves_between_evidence_panes() {
    assert_eq!(
        move_focus(EvidencePane::Search, crate::tui::nav::NavDirection::Right),
        EvidenceNavOutcome::Focus(EvidencePane::Timeline)
    );
    assert_eq!(
        move_focus(EvidencePane::Bundle, crate::tui::nav::NavDirection::Down),
        EvidenceNavOutcome::Focus(EvidencePane::Bundle)
    );
}

struct EvidenceFixture {
    events: InspectionEnvelope<EventPage>,
    proofs: Vec<ProofDetail>,
    receipts: Vec<ActionReceipt>,
    search: EvidenceSearch,
}

impl EvidenceFixture {
    fn input(&self) -> EvidenceLensInput<'_> {
        select_evidence_lens_input(&self.events, &self.proofs, &self.receipts, &self.search)
    }
}

fn fixture_input() -> EvidenceFixture {
    let now = Utc::now();
    let entity = entity();
    let proof = ProofDetail {
        proof_id: "proof/job-14445".into(),
        status: "verified".into(),
        summary: "test capsule accepted token=abc123".into(),
        entity: Some(EntityDetail {
            entity: entity.clone(),
            state: "failed".into(),
            summary: "compile failure".into(),
            timeline: Vec::new(),
            blockers: Vec::new(),
            evidence: Vec::new(),
            related: Vec::new(),
            available_actions: Vec::new(),
            risk: None,
            last_updated: Some(now),
            expires_after_ms: Some(60_000),
        }),
        evidence_refs: vec!["capsule/job-14445".into()],
        generated_at: now,
    };
    let event = TuiEvent {
        seq: 7,
        timestamp: now,
        kind: TuiEventKind::JobFailed,
        severity: Severity::Error,
        entity: entity.clone(),
        parent: Some(EntityRef::new(EntityKind::Pipeline, "pipeline-9")),
        summary: "job failed".into(),
        correlation_id: Some("corr-1".into()),
        evidence_refs: vec!["proof/job-14445".into(), "proof/missing".into()],
        next_actions: Vec::new(),
        stale_after_ms: 60_000,
    };
    let receipt = ActionReceipt {
        receipt_id: "receipt/run-tests-1".into(),
        action_id: "run_tests".into(),
        idempotency_key: "idem-1".into(),
        status: ActionStatus::Completed,
        dry_run: false,
        summary: "created proof for job-14445".into(),
        event_cursor: Some(8),
        affected_entity: Some(entity),
        evidence_created: vec!["proof/job-14445".into()],
        accepted_at: now,
    };

    EvidenceFixture {
        events: InspectionEnvelope::new(
            EventPage {
                cursor: 6,
                next_cursor: 7,
                events: vec![event],
            },
            Vec::new(),
            now,
        ),
        proofs: vec![proof],
        receipts: vec![receipt],
        search: EvidenceSearch::default(),
    }
}

fn entity() -> EntityRef {
    EntityRef::new(EntityKind::Job, "job-14445")
}

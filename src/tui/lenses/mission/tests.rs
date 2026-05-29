use ratatui::{buffer::Buffer, widgets::Widget};

use super::*;
use crate::{
    api::{freshness::FreshnessState, inspection::InspectionEnvelope},
    tui::{
        app::{
            reducer::AppIntent,
            state::{AppRoute, FlightDeckState},
        },
        testing::{FixtureScenario, ScenarioFixture},
        theme::{TerminalCaps, Theme},
        widgets::shared::CanonicalSize,
    },
};

fn envelope_for(
    scenario: FixtureScenario,
) -> InspectionEnvelope<crate::api::read_model::TuiReadModel> {
    let fixture = ScenarioFixture::build(scenario);
    InspectionEnvelope::new(fixture.read_model, fixture.sources, fixture.generated_at)
}

fn render_text(scenario: FixtureScenario, size: CanonicalSize) -> String {
    let envelope = envelope_for(scenario);
    let input = select_mission_lens_input(&envelope);
    let theme = Theme::dark();
    let area = size.area();
    let mut buffer = Buffer::empty(area);
    MissionLens::new(input, &theme, TerminalCaps::ascii()).render(area, &mut buffer);
    buffer
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[test]
fn selector_exposes_posture_freshness_and_proofs() {
    let envelope = envelope_for(FixtureScenario::Stale);
    let input = select_mission_lens_input(&envelope);

    assert_eq!(input.posture_label(), "READY: delivery work can continue");
    assert_eq!(
        input.primary_freshness().map(|source| source.state),
        Some(FreshnessState::Aged)
    );
    assert_eq!(input.proof_links(), vec!["proof/stale"]);
}

#[test]
fn required_fixture_states_render_explicit_mission_surfaces() {
    let cases = [
        (FixtureScenario::Healthy, "READY"),
        (FixtureScenario::Empty, "CAUTION"),
        (FixtureScenario::Stale, "STALE"),
        (FixtureScenario::Degraded, "degraded"),
        (FixtureScenario::SourceDown, "SOURCE DOWN"),
        (FixtureScenario::Release, "request_merge"),
    ];

    for (scenario, expected) in cases {
        let text = render_text(scenario, CanonicalSize::Wide);
        assert!(
            text.contains("Mission"),
            "missing mission title for {scenario:?}"
        );
        assert!(
            text.contains(expected),
            "missing {expected:?} in rendered {scenario:?} fixture: {text:?}"
        );
    }
}

#[test]
fn mission_lens_renders_at_canonical_sizes() {
    for size in [
        CanonicalSize::Compact,
        CanonicalSize::Standard,
        CanonicalSize::Wide,
    ] {
        let text = render_text(FixtureScenario::Healthy, size);
        assert!(text.contains("Mission"));
    }
}

#[test]
fn nav_activation_returns_intents_without_mutating_state() {
    let state = FlightDeckState::default();

    let healthy = envelope_for(FixtureScenario::Healthy);
    let healthy_input = select_mission_lens_input(&healthy);
    assert_eq!(
        activate_pane(MissionPane::NextAction, healthy_input, &state),
        MissionNavOutcome::Intent(AppIntent::BeginActionPreview {
            action_id: "open_logs".into()
        })
    );

    let stale = envelope_for(FixtureScenario::Stale);
    let stale_input = select_mission_lens_input(&stale);
    assert_eq!(
        activate_pane(MissionPane::ProofLinks, stale_input, &state),
        MissionNavOutcome::Intent(AppIntent::Navigate(AppRoute::Proof("proof/stale".into())))
    );

    let source_down = envelope_for(FixtureScenario::SourceDown);
    let source_down_input = select_mission_lens_input(&source_down);
    assert!(matches!(
        activate_pane(MissionPane::TopBlocker, source_down_input, &state),
        MissionNavOutcome::Intent(AppIntent::SelectEntity(Some(_)))
    ));
}

#[test]
fn nav_focus_moves_between_mission_panes() {
    assert_eq!(
        move_focus(MissionPane::Posture, crate::tui::nav::NavDirection::Right),
        MissionNavOutcome::Focus(MissionPane::TopBlocker)
    );
    assert_eq!(
        move_focus(MissionPane::ProofLinks, crate::tui::nav::NavDirection::Down),
        MissionNavOutcome::Focus(MissionPane::ProofLinks)
    );
}

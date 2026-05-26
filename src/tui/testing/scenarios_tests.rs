use crate::{
    api::{entity::HealthLevel, freshness::FreshnessState},
    tui::testing::{FixtureScenario, ScenarioFixture},
};

#[test]
fn all_fixture_scenarios_have_labels_and_events() {
    for scenario in FixtureScenario::ALL {
        let fixture = ScenarioFixture::build(*scenario);
        assert_eq!(fixture.scenario.label(), scenario.label());
        assert_eq!(fixture.events.len(), 1);
        assert_eq!(fixture.read_model.event_cursor, 1);
    }
}

#[test]
fn stale_fixture_marks_freshness_stale_without_renaming_api_enum() {
    let fixture = ScenarioFixture::build(FixtureScenario::Stale);
    assert!(fixture.read_model.freshness.overall_stale);
    assert!(
        fixture
            .sources
            .iter()
            .any(|source| source.state == FreshnessState::Aged)
    );
}

#[test]
fn repo_fixtures_cover_empty_aged_degraded_and_source_down_states() {
    let empty = ScenarioFixture::build(FixtureScenario::Empty);
    assert!(empty.read_model.repos.repos.is_empty());
    assert!(empty.read_model.repos.families.is_empty());

    let aged = ScenarioFixture::build(FixtureScenario::Aged);
    assert!(aged.read_model.repos.repos.iter().any(|repo| repo.aged));
    assert!(
        aged.read_model
            .repos
            .families
            .iter()
            .any(|family| family.status == "aged")
    );

    let degraded = ScenarioFixture::build(FixtureScenario::Degraded);
    assert!(
        degraded
            .read_model
            .repos
            .repos
            .iter()
            .any(|repo| repo.failed_count > 0)
    );

    let source_down = ScenarioFixture::build(FixtureScenario::SourceDown);
    assert!(
        source_down
            .read_model
            .repos
            .repos
            .iter()
            .any(|repo| repo.status == "source_down")
    );
}

#[test]
fn incident_fixture_blocks_all_mutation_postures() {
    let fixture = ScenarioFixture::build(FixtureScenario::Incident);
    assert!(!fixture.read_model.mission.safe_to_code);
    assert!(!fixture.read_model.mission.safe_to_merge);
    assert!(!fixture.read_model.mission.safe_to_release);
    assert_eq!(fixture.read_model.mission.overall, HealthLevel::Critical);
}

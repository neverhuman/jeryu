use jeryu_enterprise::RedTeamSuite;

#[test]
fn security_red_team_suite_blocks_modeled_attacks() {
    let suite = RedTeamSuite::phase10();
    assert!(suite.passes());
    assert!(
        suite
            .findings
            .iter()
            .any(|finding| finding.id == "tenant-cross-read")
    );
}

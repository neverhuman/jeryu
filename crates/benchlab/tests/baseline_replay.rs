use benchlab::{sample_phase10_harness, Competitor, ReplayPlan};

#[test]
fn baseline_runner_replay_is_present_and_valid() {
    let receipts = sample_phase10_harness().provider_neutral_comparison_receipts();
    for required in [
        Competitor::BaselineRunnerContainer,
        Competitor::BaselineRunnerShell,
        Competitor::BaselineRunnerKubernetes,
    ] {
        assert!(receipts
            .iter()
            .any(|receipt| receipt.competitor == required));
    }

    let mut plan = ReplayPlan::new();
    for receipt in receipts {
        plan.add_receipt(receipt).expect("valid receipt");
    }
    let verdict = plan.verify();
    assert!(verdict.passed(), "{:?}", verdict.failures);
}

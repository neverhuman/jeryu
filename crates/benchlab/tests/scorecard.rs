use benchlab::{sample_phase10_harness, Scorecard};

#[test]
fn phase10_scorecard_passes_spec_targets() {
    let receipts = sample_phase10_harness().provider_neutral_comparison_receipts();
    let scorecard = Scorecard::from_receipts(&receipts);
    assert!(scorecard.passed(), "{}", scorecard.to_markdown());
}

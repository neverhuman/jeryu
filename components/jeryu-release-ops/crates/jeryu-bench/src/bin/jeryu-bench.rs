use jeryu_bench::{ReplayPlan, Scorecard, sample_phase10_harness};

fn main() {
    let harness = sample_phase10_harness();
    let receipts = harness.provider_neutral_comparison_receipts();
    let mut plan = ReplayPlan::new();
    for receipt in receipts.iter().cloned() {
        if let Err(error) = plan.add_receipt(receipt) {
            eprintln!("invalid receipt: {error:?}");
            std::process::exit(2);
        }
    }
    let verdict = plan.verify();
    if !verdict.passed() {
        eprintln!("benchmark replay failed: {:?}", verdict.failures);
        std::process::exit(1);
    }
    let scorecard = Scorecard::from_receipts(&receipts);
    println!("{}", scorecard.to_markdown());
}

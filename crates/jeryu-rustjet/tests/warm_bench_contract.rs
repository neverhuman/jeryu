use jeryu_rustjet::{BenchmarkExpectation, BenchmarkResult};

#[test]
fn warm_medium_pr_exit_bar_contract_accepts_good_result() {
    let expectation = BenchmarkExpectation::warm_medium_pr();
    let result = BenchmarkResult {
        name: "warm-medium-rust-pr".to_string(),
        duration_ms: 58_000,
        speedup_vs_baseline_container: 3.2,
        false_cache_hits: 0,
    };
    assert!(expectation.evaluate(&result));
}

#[test]
fn warm_medium_pr_exit_bar_rejects_false_cache_hit() {
    let expectation = BenchmarkExpectation::warm_medium_pr();
    let result = BenchmarkResult {
        name: "warm-medium-rust-pr".to_string(),
        duration_ms: 40_000,
        speedup_vs_baseline_container: 5.0,
        false_cache_hits: 1,
    };
    assert!(!expectation.evaluate(&result));
}

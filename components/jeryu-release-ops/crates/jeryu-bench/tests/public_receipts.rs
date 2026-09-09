use jeryu_bench::sample_phase10_harness;

#[test]
fn public_receipt_json_contains_required_evidence() {
    let receipt = sample_phase10_harness()
        .provider_neutral_comparison_receipts()
        .remove(0);
    receipt.validate().expect("receipt is publishable");
    let json = receipt.to_json();
    for field in [
        "benchmark_id",
        "competitor",
        "jeryu_runner",
        "hardware",
        "git_sha",
        "pipeline_ir_hash",
        "artifact_digest",
        "reproduce",
    ] {
        assert!(json.contains(field), "missing {field}: {json}");
    }
}

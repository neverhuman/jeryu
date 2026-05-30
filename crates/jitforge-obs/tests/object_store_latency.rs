use jitforge_obs::ChaosDrill;

#[test]
fn object_store_latency_drill_has_receipt_and_passes() {
    let result = ChaosDrill::object_store_latency().simulate(8_000, false);
    assert!(result.passed());
    assert!(result.receipt.contains("object-store-latency"));
}

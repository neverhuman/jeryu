use jeryu_bench::{JeryuRunner, sample_phase10_harness};

#[test]
fn native_vs_oci_receipts_cover_every_runner_class() {
    let receipts = sample_phase10_harness().native_vs_oci_receipts();
    for runner in [
        JeryuRunner::NativeRustHot,
        JeryuRunner::NativeRustClean,
        JeryuRunner::MicroVmRust,
        JeryuRunner::OciDocker,
        JeryuRunner::K8sOci,
    ] {
        assert!(
            receipts
                .iter()
                .any(|receipt| receipt.jeryu_runner == runner)
        );
    }
}

#[test]
fn native_hot_beats_oci_on_each_fixture() {
    let receipts = sample_phase10_harness().native_vs_oci_receipts();
    for fixture in ["rust-small", "rust-medium", "rust-large", "merge-queue"] {
        let native = receipts
            .iter()
            .filter(|receipt| {
                receipt.repo_fixture == fixture
                    && receipt.jeryu_runner == JeryuRunner::NativeRustHot
            })
            .map(|receipt| receipt.jeryu_duration_ms)
            .min();
        let oci = receipts
            .iter()
            .filter(|receipt| {
                receipt.repo_fixture == fixture && receipt.jeryu_runner == JeryuRunner::OciDocker
            })
            .map(|receipt| receipt.jeryu_duration_ms)
            .min();
        if let (Some(native), Some(oci)) = (native, oci) {
            assert!(
                native < oci,
                "fixture {fixture}: native {native}, oci {oci}"
            );
        }
    }
}

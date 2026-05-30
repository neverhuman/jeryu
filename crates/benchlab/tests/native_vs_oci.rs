use benchlab::{sample_phase10_harness, JitForgeRunner};

#[test]
fn native_vs_oci_receipts_cover_every_runner_class() {
    let receipts = sample_phase10_harness().native_vs_oci_receipts();
    for runner in [
        JitForgeRunner::NativeRustHot,
        JitForgeRunner::NativeRustClean,
        JitForgeRunner::MicroVmRust,
        JitForgeRunner::OciDocker,
        JitForgeRunner::K8sOci,
    ] {
        assert!(receipts
            .iter()
            .any(|receipt| receipt.jitforge_runner == runner));
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
                    && receipt.jitforge_runner == JitForgeRunner::NativeRustHot
            })
            .map(|receipt| receipt.jitforge_duration_ms)
            .min();
        let oci = receipts
            .iter()
            .filter(|receipt| {
                receipt.repo_fixture == fixture
                    && receipt.jitforge_runner == JitForgeRunner::OciDocker
            })
            .map(|receipt| receipt.jitforge_duration_ms)
            .min();
        if let (Some(native), Some(oci)) = (native, oci) {
            assert!(
                native < oci,
                "fixture {fixture}: native {native}, oci {oci}"
            );
        }
    }
}

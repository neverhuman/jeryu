use proptest::prelude::*;
use std::path::Path;
use std::process::Command;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn registry_rejects_arbitrary_unknown_flags(suffix in "[a-z]{1,16}") {
        prop_assume!(suffix != "check");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let output = Command::new(env!("CARGO_BIN_EXE_jeryu-toolctl"))
            .args(["--tool-root", root.to_str().expect("UTF-8 test root")])
            .args(["registry-summary", &format!("--{suffix}")])
            .output()
            .expect("run jeryu-toolctl");
        prop_assert!(!output.status.success());
        prop_assert!(output.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&output.stderr);
        prop_assert!(stderr.starts_with("registry-summary accepts only --check\n"));
        prop_assert!(stderr.contains("repair_hint:"));
        prop_assert!(stderr.contains("docs_url: docs/tools-registry.md"));
    }
}

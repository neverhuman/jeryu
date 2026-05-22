use std::cmp::max;

use super::TestRunOpts;

pub(crate) struct TestRoutingInference {
    pub(crate) tags: Vec<String>,
    pub(crate) risk_class: String,
    pub(crate) timeout_secs: u64,
    pub(crate) rationale: Vec<String>,
}

pub(crate) fn infer_test_tags(command: &str) -> Vec<String> {
    infer_test_routing(command).tags
}

pub(crate) fn infer_test_routing(command: &str) -> TestRoutingInference {
    let lower = command.to_ascii_lowercase();

    let untrusted_patterns = [
        "security",
        "chaos",
        "ip_exfiltration",
        "ip guard",
        "ip-guard",
        "sandbox",
        "fuzz",
    ];
    if untrusted_patterns
        .iter()
        .any(|pattern| lower.contains(pattern))
    {
        return TestRoutingInference {
            tags: vec!["untrusted".to_string()],
            risk_class: "untrusted".to_string(),
            timeout_secs: 1800,
            rationale: vec!["matched untrusted/risky command pattern".to_string()],
        };
    }

    let build_patterns = [
        "deployctl",
        "payloadctl",
        "build-local",
        "publish-rc",
        "cargo check",
        "cargo build",
        "cargo nextest",
        "cargo test --workspace",
        "cargo test -p veox-deploy",
        "cargo test -p veox-bootstrap",
        "cargo test -p veox-rc-gate",
        "cargo test -p cargo-vrc",
        "cargo test -p cargo-aer",
        "cargo test -p cargo-witness",
        "cargo test -p nht",
        "test-local-built",
        "test-local-rc",
        "deploy-canary",
    ];
    if build_patterns.iter().any(|pattern| lower.contains(pattern)) {
        return TestRoutingInference {
            tags: vec!["build".to_string()],
            risk_class: "build".to_string(),
            timeout_secs: 1200,
            rationale: vec!["matched build-heavy command pattern".to_string()],
        };
    }

    TestRoutingInference {
        tags: vec!["default".to_string()],
        risk_class: "default".to_string(),
        timeout_secs: max(600, TestRunOpts::default().timeout_secs),
        rationale: vec!["no heavy/risky pattern matched; using default runner".to_string()],
    }
}

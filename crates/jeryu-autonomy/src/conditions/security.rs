//! Security-driven hard stops: evidence integrity/signature, the three security
//! scans (SAST / dependency / secret), and edits that touch security-scanner
//! config or secret-handling code.

use super::HardStop;
use super::paths::any_path_matches;
use crate::types::{AgentApprovalReceipt, EvidencePack, ScanOutcome};

const SECURITY_SCANNER_CONFIG_PATHS: &[&str] = &[
    "deny.toml",
    "cargo-deny.toml",
    "audit.toml",
    ".trivyignore",
    ".gitleaks.toml",
    "gitleaks.toml",
    ".bandit",
    ".semgrep.yml",
    "agent/security-policy.toml",
];

// R-7 (D1): the legacy external-host security-policy prefix is removed.
// `.github/...` stays (still a real external host contributors use), plus a
// jeryu-native `.jeryu/ci/security` prefix is added.
const SECURITY_SCANNER_PATH_PREFIXES: &[&str] = &[
    ".github/workflows/security",
    ".jeryu/ci/security",
    "agent/security-policies",
];

const SECRET_HANDLING_PREFIXES: &[&str] = &["secrets/", "src/secrets", "src/llm/secrets"];

pub(super) fn cond_evidence_missing(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    if p.evidence_digest.is_empty() {
        return Some(HardStop {
            name: "evidence_missing".into(),
            reason: "evidence_pack has no digest".into(),
            details: serde_json::Value::Null,
        });
    }
    None
}

pub(super) fn cond_evidence_signature_invalid(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    match &p.signature {
        Some(s) if s.algo == "ed25519" => None,
        Some(s) if s.algo == "unsigned" => Some(HardStop {
            name: "evidence_signature_invalid".into(),
            reason: "evidence pack carries an unsigned signature; not acceptable in enforcement"
                .into(),
            details: serde_json::json!({ "algo": s.algo }),
        }),
        Some(s) if s.algo == "hmac-sha256-insecure" => Some(HardStop {
            name: "evidence_signature_invalid".into(),
            reason: "evidence pack signed with insecure HMAC; ed25519 required in enforcement"
                .into(),
            details: serde_json::json!({ "algo": s.algo }),
        }),
        Some(s) => Some(HardStop {
            name: "evidence_signature_invalid".into(),
            reason: format!("evidence pack signed with unknown algo '{}'", s.algo),
            details: serde_json::json!({ "algo": s.algo }),
        }),
        None => Some(HardStop {
            name: "evidence_signature_invalid".into(),
            reason: "evidence pack is unsigned".into(),
            details: serde_json::Value::Null,
        }),
    }
}

pub(super) fn cond_secret_scan_failed(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    matches!(p.security.secret_scan, ScanOutcome::Failed).then(|| HardStop {
        name: "secret_scan_failed".into(),
        reason: "secret scan reported findings".into(),
        details: serde_json::Value::Null,
    })
}

pub(super) fn cond_secret_scan_missing(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    matches!(p.security.secret_scan, ScanOutcome::Missing).then(|| HardStop {
        name: "secret_scan_missing".into(),
        reason: "secret scan never ran; fail-closed".into(),
        details: serde_json::Value::Null,
    })
}

pub(super) fn cond_sast_failed(p: &EvidencePack, _r: &[AgentApprovalReceipt]) -> Option<HardStop> {
    matches!(p.security.sast, ScanOutcome::Failed).then(|| HardStop {
        name: "sast_failed".into(),
        reason: "SAST scan failed".into(),
        details: serde_json::Value::Null,
    })
}

pub(super) fn cond_dependency_scan_failed(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    matches!(p.security.dependency_scan, ScanOutcome::Failed).then(|| HardStop {
        name: "dependency_scan_failed".into(),
        reason: "dependency / advisory scan failed".into(),
        details: serde_json::Value::Null,
    })
}

pub(super) fn cond_changes_security_scanner_config(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    any_path_matches(
        p,
        SECURITY_SCANNER_CONFIG_PATHS,
        SECURITY_SCANNER_PATH_PREFIXES,
    )
    .map(|paths| HardStop {
        name: "changes_security_scanner_config".into(),
        reason: "PR edits security scanner config; require elevated review".into(),
        details: serde_json::json!({ "paths": paths }),
    })
}

pub(super) fn cond_touches_secret_handling(
    p: &EvidencePack,
    _r: &[AgentApprovalReceipt],
) -> Option<HardStop> {
    any_path_matches(p, &[".env", ".env.local"], SECRET_HANDLING_PREFIXES).map(|paths| HardStop {
        name: "touches_secret_handling".into(),
        reason: "PR edits secret-handling code or config; require security review".into(),
        details: serde_json::json!({ "paths": paths }),
    })
}

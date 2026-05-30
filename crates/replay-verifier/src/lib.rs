#![forbid(unsafe_code)]
#![doc = "Replay verifier for benchmark, provenance, cache-safety, and release claims."]

use phase11_audit::{record, AuditKind, AuditLedger};
use phase11_core::{quote, Digest, Finding, Report, Severity, TenantId};

/// A benchmark or provenance claim that must be replayable.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayClaim {
    pub claim_id: String,
    pub competitor: String,
    pub jitforge_runner: String,
    pub fixture: String,
    pub scenario: String,
    pub hardware: String,
    pub cache_state: String,
    pub git_sha: String,
    pub pipeline_ir_hash: String,
    pub duration_ms: u64,
    pub speedup_vs_baseline: f32,
    pub false_cache_hits: u32,
    pub artifact_digest: String,
    pub reproduce: String,
}

impl ReplayClaim {
    /// Deterministic digest of the claim body.
    pub fn digest(&self) -> Digest {
        Digest::from_text(
            "replay",
            &format!(
                "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                self.claim_id,
                self.competitor,
                self.jitforge_runner,
                self.fixture,
                self.scenario,
                self.hardware,
                self.cache_state,
                self.git_sha,
                self.pipeline_ir_hash,
                self.duration_ms,
                self.speedup_vs_baseline,
                self.false_cache_hits,
                self.artifact_digest
            ),
        )
    }

    /// JSON representation.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"claim_id\":{},\"competitor\":{},\"jitforge_runner\":{},\"fixture\":{},\"scenario\":{},\"hardware\":{},\"cache_state\":{},\"git_sha\":{},\"pipeline_ir_hash\":{},\"duration_ms\":{},\"speedup_vs_baseline\":{},\"false_cache_hits\":{},\"artifact_digest\":{},\"reproduce\":{},\"digest\":{}}}",
            quote(&self.claim_id), quote(&self.competitor), quote(&self.jitforge_runner), quote(&self.fixture), quote(&self.scenario), quote(&self.hardware), quote(&self.cache_state), quote(&self.git_sha), quote(&self.pipeline_ir_hash), self.duration_ms, self.speedup_vs_baseline, self.false_cache_hits, quote(&self.artifact_digest), quote(&self.reproduce), quote(self.digest().as_str())
        )
    }
}

/// Replay verification verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayVerdict {
    pub report: Report,
    pub receipt_id: String,
}

impl ReplayVerdict {
    /// JSON representation.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"receipt_id\":{},\"report\":{}}}",
            quote(&self.receipt_id),
            self.report.to_json()
        )
    }
}

/// Verify a claim against Phase 11 replay law.
pub fn verify_claim(
    tenant: &TenantId,
    actor: &str,
    claim: &ReplayClaim,
    min_speedup: f32,
    ledger: &mut AuditLedger,
) -> ReplayVerdict {
    let mut report = Report::new("phase11.replay");

    for (field, value) in [
        ("claim_id", claim.claim_id.as_str()),
        ("competitor", claim.competitor.as_str()),
        ("jitforge_runner", claim.jitforge_runner.as_str()),
        ("fixture", claim.fixture.as_str()),
        ("scenario", claim.scenario.as_str()),
        ("hardware", claim.hardware.as_str()),
        ("cache_state", claim.cache_state.as_str()),
        ("git_sha", claim.git_sha.as_str()),
        ("pipeline_ir_hash", claim.pipeline_ir_hash.as_str()),
        ("artifact_digest", claim.artifact_digest.as_str()),
        ("reproduce", claim.reproduce.as_str()),
    ] {
        if value.trim().is_empty() {
            report.push(Finding::new(
                format!("replay.{}.missing", field),
                Severity::Blocked,
                format!("{} is required", field),
            ));
        }
    }

    if claim.duration_ms == 0 {
        report.push(Finding::new(
            "replay.duration_zero",
            Severity::Blocked,
            "duration must be non-zero",
        ));
    }
    if claim.speedup_vs_baseline < min_speedup {
        report.push(Finding::new(
            "replay.speedup_insufficient",
            Severity::Critical,
            format!(
                "speedup {} is below required {}",
                claim.speedup_vs_baseline, min_speedup
            ),
        ));
    }
    if claim.false_cache_hits != 0 {
        report.push(Finding::new(
            "replay.false_cache_hits",
            Severity::Blocked,
            "false cache hits are not tolerated",
        ));
    }
    if !claim.pipeline_ir_hash.contains(':') {
        report.push(Finding::new(
            "replay.ir_hash_format",
            Severity::Blocked,
            "pipeline IR hash must include algorithm prefix",
        ));
    }
    if !claim.artifact_digest.contains(':') {
        report.push(Finding::new(
            "replay.artifact_digest_format",
            Severity::Blocked,
            "artifact digest must include algorithm prefix",
        ));
    }

    if report.findings.is_empty() {
        report.push(Finding::new(
            "replay.verified",
            Severity::Info,
            "claim is replayable under Phase 11 law",
        ));
    }

    let receipt_id = record(
        ledger,
        tenant,
        AuditKind::ReplayVerification,
        actor,
        &claim.claim_id,
        vec![
            claim.digest().as_str().to_string(),
            format!("state={}", report.state.as_str()),
        ],
    );
    ReplayVerdict { report, receipt_id }
}

/// Build a fixture claim used by CLI and tests.
pub fn fixture_claim() -> ReplayClaim {
    ReplayClaim {
        claim_id: "bench_phase11_fixture".to_string(),
        competitor: "baseline-runner-container".to_string(),
        jitforge_runner: "native-rust-hot".to_string(),
        fixture: "rust-medium".to_string(),
        scenario: "private-function-change-warm-cache".to_string(),
        hardware: "32c/128gb/nvme".to_string(),
        cache_state: "warm_project_cache".to_string(),
        git_sha: "abc123".to_string(),
        pipeline_ir_hash: "blake3:fixture".to_string(),
        duration_ms: 18_231,
        speedup_vs_baseline: 4.8,
        false_cache_hits: 0,
        artifact_digest: "sha256:fixture".to_string(),
        reproduce: "jit bench replay bench_phase11_fixture".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn false_cache_hits_block_replay() {
        let tenant = TenantId::new("tenant-a").unwrap_or_else(|_| panic!("valid"));
        let mut ledger = AuditLedger::new();
        let mut claim = fixture_claim();
        claim.false_cache_hits = 1;
        let verdict = verify_claim(&tenant, "bench-bot", &claim, 2.0, &mut ledger);
        assert!(verdict.report.fails_closed());
    }
}

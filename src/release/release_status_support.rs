use super::*;
use std::fs;
use std::path::Path;

use crate::release::lifecycle::{ReleaseLock, release_lock_path};

pub(crate) fn parse_state_json(version: &str) -> Result<Option<serde_json::Value>> {
    let path = canary_state_path(version);
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
}

pub(crate) fn json_release_identity_ok(path: &Path, version: &str, expected_sha: &str) -> bool {
    let Ok(raw) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    value
        .get("git_sha")
        .and_then(|value| value.as_str())
        .map(|value| value == expected_sha)
        .unwrap_or(false)
        && value
            .get("release_version")
            .and_then(|value| value.as_str())
            .map(|value| value == version)
            .unwrap_or(false)
}

pub(crate) fn release_lock_identity_ok(version: &str, expected_sha: &str) -> bool {
    let Ok(raw) = fs::read_to_string(release_lock_path(version)) else {
        return false;
    };
    let Ok(lock) = serde_json::from_str::<ReleaseLock>(&raw) else {
        return false;
    };
    lock.product_sha == expected_sha && lock.release_version == version
}

pub(crate) fn release_identity_ok(version: &str, expected_sha: &str) -> bool {
    let release_json = release_dir(version).join("release.json");
    let contract_json = release_dir(version).join("release-contract.json");
    release_lock_identity_ok(version, expected_sha)
        && json_release_identity_ok(&release_json, version, expected_sha)
        && json_release_identity_ok(&contract_json, version, expected_sha)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CanaryGateFiles {
    pub(crate) remote: bool,
    pub(crate) telemetry: bool,
    pub(crate) e2e: bool,
    pub(crate) validation: bool,
    pub(crate) handoff: bool,
    pub(crate) telemetry_diag: bool,
}

impl CanaryGateFiles {
    pub(crate) fn canary_complete(self) -> bool {
        self.remote && self.telemetry && self.e2e && self.validation
    }

    pub(crate) fn promotion_ready(self) -> bool {
        self.canary_complete() && self.handoff
    }
}

pub(crate) fn canary_gate_files(version: &str) -> CanaryGateFiles {
    CanaryGateFiles {
        remote: gate_remote_canary_path(version).is_file(),
        telemetry: gate_canary_telemetry_path(version).is_file(),
        e2e: gate_canary_e2e_path(version).is_file(),
        validation: c_validation_path(version).is_file(),
        handoff: c_handoff_path(version).is_file(),
        telemetry_diag: telemetry_diag_path(version).is_file(),
    }
}

pub(crate) fn release_evidence(version: &str, expected_sha: &str) -> Result<ReleaseEvidence> {
    let state_value = parse_state_json(version)?;
    let gate_files = canary_gate_files(version);
    Ok(ReleaseEvidence {
        state_phase: state_value
            .as_ref()
            .and_then(|value| value.get("phase"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        state_status: state_value
            .as_ref()
            .and_then(|value| value.get("status"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        state_detail: state_value
            .as_ref()
            .and_then(|value| value.get("detail"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        has_remote_gate: gate_files.remote,
        has_telemetry_gate: gate_files.telemetry,
        has_e2e_gate: gate_files.e2e,
        has_c_validation: gate_files.validation,
        has_c_handoff: gate_files.handoff,
        has_telemetry_diag: gate_files.telemetry_diag,
        release_identity_ok: release_identity_ok(version, expected_sha),
    })
}

pub(crate) fn has_complete_canary_evidence(evidence: &ReleaseEvidence) -> bool {
    evidence.has_remote_gate
        && evidence.has_telemetry_gate
        && evidence.has_e2e_gate
        && evidence.has_c_validation
        && evidence.has_c_handoff
        && evidence.release_identity_ok
}

pub(crate) fn is_outdated_attempt(attempt: &ReleaseAttempt, evidence: &ReleaseEvidence) -> bool {
    if evidence.has_e2e_gate {
        return false;
    }

    let ts = attempt
        .canary_started_at
        .as_deref()
        .or(attempt.canary_finished_at.as_deref())
        .or(Some(attempt.updated_at.as_str()));
    let Some(ts) = ts else {
        return false;
    };
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(ts) else {
        return false;
    };
    let age = chrono::Utc::now().signed_duration_since(parsed.with_timezone(&chrono::Utc));
    age > chrono::Duration::minutes(30)
}

pub(crate) fn derive_release_health(
    attempt: &ReleaseAttempt,
    evidence: &ReleaseEvidence,
) -> ReleaseHealth {
    if attempt.upstream_status != "success" {
        return ReleaseHealth::Blocked;
    }
    if attempt.canary_status == "passed"
        && attempt.release_pipeline_status.as_deref() == Some("success")
        && has_complete_canary_evidence(evidence)
    {
        return ReleaseHealth::E2ePassed;
    }
    if !evidence.release_identity_ok {
        return ReleaseHealth::Outdated;
    }
    if matches!(evidence.state_status.as_deref(), Some("failed"))
        || attempt.canary_status == "failed"
    {
        return ReleaseHealth::Failed;
    }
    if evidence.has_e2e_gate && has_complete_canary_evidence(evidence) {
        return ReleaseHealth::E2ePassed;
    }
    if evidence.has_remote_gate {
        return ReleaseHealth::RemotePassed;
    }
    if matches!(evidence.state_status.as_deref(), Some("passed")) && !evidence.has_e2e_gate {
        return ReleaseHealth::Outdated;
    }
    if matches!(evidence.state_status.as_deref(), Some("running"))
        || attempt.canary_status == "running"
    {
        return if is_outdated_attempt(attempt, evidence) {
            ReleaseHealth::Outdated
        } else {
            ReleaseHealth::Running
        };
    }
    if attempt.canary_status == "pending" {
        return ReleaseHealth::Ready;
    }
    ReleaseHealth::Ready
}

pub(crate) fn derived_note(
    attempt: &ReleaseAttempt,
    evidence: &ReleaseEvidence,
    health: ReleaseHealth,
) -> Option<String> {
    if let Some(detail) = evidence
        .state_detail
        .as_ref()
        .filter(|detail| !detail.trim().is_empty())
    {
        let phase = evidence.state_phase.as_deref().unwrap_or("unknown-phase");
        return Some(format!("{phase}: {detail}"));
    }
    if let Some(note) = attempt
        .canary_note
        .as_ref()
        .filter(|note| !note.trim().is_empty())
    {
        return Some(note.clone());
    }
    if health == ReleaseHealth::Outdated {
        return Some("release evidence is incomplete for this version".to_string());
    }
    None
}

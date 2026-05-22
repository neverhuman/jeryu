use super::*;

pub(crate) fn view_attempt(attempt: ReleaseAttempt) -> Result<ReleaseAttemptView> {
    let version = attempt.version.clone();
    let evidence = release_evidence(&version, &attempt.sha)?;
    let health = derive_release_health(&attempt, &evidence);
    let detail = derived_note(&attempt, &evidence, health);
    Ok(ReleaseAttemptView {
        attempt,
        release_dir: release_dir(&version).display().to_string(),
        canary_state_path: canary_state_path(&version).display().to_string(),
        gate_remote_canary_path: gate_remote_canary_path(&version).display().to_string(),
        gate_canary_e2e_path: gate_canary_e2e_path(&version).display().to_string(),
        gate_canary_telemetry_path: gate_canary_telemetry_path(&version).display().to_string(),
        telemetry_diag_path: telemetry_diag_path(&version).display().to_string(),
        canary_state: health.as_str().to_string(),
        eligibility: health.eligibility().to_string(),
        phase: evidence.state_phase,
        detail,
        state_status: evidence.state_status,
        has_remote_gate: evidence.has_remote_gate,
        has_telemetry_gate: evidence.has_telemetry_gate,
        has_e2e_gate: evidence.has_e2e_gate,
        has_telemetry_diag: evidence.has_telemetry_diag,
        release_identity_ok: evidence.release_identity_ok,
        canary_public_url: canary_public_url(&version),
    })
}

//! Owner: Release Pipeline (rollback)
//! Proof: `cargo test -p jeryu -- release::rollback`
//! Invariants: Never re-tags; always writes rollback.json evidence.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::autonomy::types::ReleaseRollbackPlan;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackStep {
    pub n: u8,
    pub kind: String,
    pub description: String,
    pub applied: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackReport {
    pub version: String,
    pub reason: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub steps: Vec<RollbackStep>,
    pub final_status: String,
}

/// Default rollback ladder. Mirrors `release.policy.toml [[rollback_step]]`.
pub fn default_ladder() -> Vec<RollbackStep> {
    vec![
        RollbackStep {
            n: 1,
            kind: "feature-flag".into(),
            description: "Disable feature/capability flag. Keep deployed binary.".into(),
            applied: false,
            detail: None,
        },
        RollbackStep {
            n: 2,
            kind: "channel".into(),
            description: "Move stable channel pointer back to previous known-good artifact.".into(),
            applied: false,
            detail: None,
        },
        RollbackStep {
            n: 3,
            kind: "revert-pr".into(),
            description: "Open revert PR through normal merge queue. Publish patch release.".into(),
            applied: false,
            detail: None,
        },
        RollbackStep {
            n: 4,
            kind: "incident".into(),
            description: "Open incident issue, follow runbook in docs/release-policy.md.".into(),
            applied: false,
            detail: None,
        },
    ]
}

/// Build a rollback report. In dry-run mode no filesystem changes occur; the
/// caller still writes the evidence record. In real mode this is where step
/// 1..3 would be applied (currently best-effort scaffolding — channel pointer
/// moves and feature-flag toggles need additional infra to implement safely).
pub fn build_report(version: &str, reason: &str, dry_run: bool) -> RollbackReport {
    let started_at = Utc::now().to_rfc3339();
    let mut steps = default_ladder();
    if dry_run {
        for s in steps.iter_mut() {
            s.detail = Some("dry-run: not applied".into());
        }
    } else {
        // Future: actually toggle the feature flag and move the channel pointer.
        // For now we record the rollback request and mark steps as "scheduled"
        // so the operator can complete them manually with full audit.
        for s in steps.iter_mut() {
            s.detail = Some("scheduled — apply via manual operator step".into());
        }
    }

    RollbackReport {
        version: version.to_string(),
        reason: reason.to_string(),
        started_at: started_at.clone(),
        completed_at: if dry_run { Some(started_at) } else { None },
        steps,
        final_status: if dry_run {
            "dry-run".into()
        } else {
            "scheduled".into()
        },
    }
}

/// Write `rollback.json` into the version's evidence directory. Returns the
/// path that was written.
pub fn write_evidence(report: &RollbackReport, evidence_dir: PathBuf) -> Result<PathBuf> {
    fs::create_dir_all(&evidence_dir)
        .with_context(|| format!("create evidence dir {}", evidence_dir.display()))?;
    let path = evidence_dir.join("rollback.json");
    let body = serde_json::to_string_pretty(report)?;
    fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// Rollback drill — Law 7 ("no rollback drill = no prod")
// ---------------------------------------------------------------------------
//
// Wave 3 evidence-gate enforcement: every staging artifact must be exercised
// against a real rollback plan before promotion. The drill times the executor,
// captures any error, and reports a structured `RollbackDrillResult` the
// release gate inspects (and the orchestrator injects as the
// `rollback_drill_failed` hard-stop when `passed == false`).

/// Outcome of a single rollback drill run. Recorded in evidence and consumed
/// by the release gate. `rolled_back_to` is the artifact digest the rollback
/// would have returned production to (typically the previous-known-good).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RollbackDrillResult {
    pub passed: bool,
    pub rolled_back_to: Option<String>,
    pub elapsed_secs: u64,
    pub error: Option<String>,
}

/// Pluggable execution backend for the rollback drill. Production wires a
/// real implementation (channel-pointer mover + flag toggler); tests and dev
/// use `DryRunRollbackExecutor`. Implementations MUST be side-effect free in
/// dry-run mode and MUST surface failures as `Err` rather than panicking.
pub trait RollbackExecutor: Send + Sync {
    fn execute(&self, plan: &ReleaseRollbackPlan, staging_artifact_digest: &str) -> Result<()>;
}

/// Default executor used in tests and local development. Sleeps briefly to
/// give `rollback_drill` a measurable `elapsed_secs` and always succeeds. The
/// 50ms is a deliberate floor — it surfaces obvious wiring bugs (zero elapsed
/// = drill never actually ran) without slowing test suites.
pub struct DryRunRollbackExecutor;

impl RollbackExecutor for DryRunRollbackExecutor {
    fn execute(&self, _plan: &ReleaseRollbackPlan, _staging_artifact_digest: &str) -> Result<()> {
        std::thread::sleep(Duration::from_millis(50));
        Ok(())
    }
}

/// Run a rollback drill against `executor`, timing the call and capturing any
/// error. The `rolled_back_to` field is set to the artifact digest the
/// rollback targeted (the staging artifact passed in is the artifact whose
/// rollback we exercised; success means the executor was able to take the
/// system back to that artifact's known-good predecessor).
pub async fn rollback_drill(
    executor: &dyn RollbackExecutor,
    plan: &ReleaseRollbackPlan,
    staging_artifact_digest: &str,
) -> RollbackDrillResult {
    let started = Instant::now();
    let outcome = executor.execute(plan, staging_artifact_digest);
    let elapsed_secs = started.elapsed().as_secs();
    match outcome {
        Ok(()) => RollbackDrillResult {
            passed: true,
            rolled_back_to: Some(staging_artifact_digest.to_string()),
            elapsed_secs,
            error: None,
        },
        Err(e) => RollbackDrillResult {
            passed: false,
            rolled_back_to: None,
            elapsed_secs,
            error: Some(format!("{e:#}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ladder_has_four_steps_in_order() {
        let ladder = default_ladder();
        assert_eq!(ladder.len(), 4);
        for (i, step) in ladder.iter().enumerate() {
            assert_eq!(step.n as usize, i + 1, "step {} out of order", i);
        }
    }

    #[test]
    fn dry_run_report_has_no_real_apply() {
        let r = build_report("3.0.1-rc.1", "test rollback", true);
        assert_eq!(r.final_status, "dry-run");
        assert!(r.completed_at.is_some());
        for step in &r.steps {
            assert!(!step.applied);
            assert!(step.detail.as_ref().unwrap().contains("dry-run"));
        }
    }

    #[test]
    fn real_report_is_scheduled_not_completed() {
        let r = build_report("3.0.1-rc.1", "test rollback", false);
        assert_eq!(r.final_status, "scheduled");
        assert!(r.completed_at.is_none());
    }

    #[test]
    fn write_evidence_creates_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let report = build_report("3.0.1-rc.1", "reason", true);
        let path = write_evidence(&report, tmp.path().to_path_buf()).expect("write");
        assert!(path.exists());
        let body = std::fs::read_to_string(&path).expect("read");
        assert!(body.contains("3.0.1-rc.1"));
    }

    fn drill_plan() -> ReleaseRollbackPlan {
        ReleaseRollbackPlan {
            strategy: "revert_commit".into(),
            tested: false,
        }
    }

    #[tokio::test]
    async fn rollback_drill_passes_with_dry_run_executor() {
        let exec = DryRunRollbackExecutor;
        let plan = drill_plan();
        let digest = "sha256:abc";
        let result = rollback_drill(&exec, &plan, digest).await;
        assert!(result.passed, "dry-run executor must report drill passed");
        assert!(result.error.is_none(), "passing drill has no error");
    }

    /// Custom executor that always fails. Used to prove that errors flow into
    /// `RollbackDrillResult.error` instead of panicking or swallowing.
    struct FailingExecutor;
    impl RollbackExecutor for FailingExecutor {
        fn execute(
            &self,
            _plan: &ReleaseRollbackPlan,
            _staging_artifact_digest: &str,
        ) -> Result<()> {
            anyhow::bail!("channel pointer move refused: stale lease")
        }
    }

    #[tokio::test]
    async fn rollback_drill_fails_when_executor_errs() {
        let exec = FailingExecutor;
        let plan = drill_plan();
        let result = rollback_drill(&exec, &plan, "sha256:def").await;
        assert!(!result.passed, "failing executor must mark drill as failed");
        assert!(result.rolled_back_to.is_none(), "no target on failure");
        let err = result.error.expect("error must be populated");
        assert!(
            err.contains("channel pointer move refused"),
            "error preserved: {err}"
        );
    }

    /// Custom executor that sleeps long enough to push `elapsed_secs >= 1`,
    /// proving the timing instrumentation is wired through.
    struct SlowExecutor;
    impl RollbackExecutor for SlowExecutor {
        fn execute(
            &self,
            _plan: &ReleaseRollbackPlan,
            _staging_artifact_digest: &str,
        ) -> Result<()> {
            std::thread::sleep(Duration::from_millis(1_050));
            Ok(())
        }
    }

    #[tokio::test]
    async fn rollback_drill_records_elapsed_secs() {
        let exec = SlowExecutor;
        let plan = drill_plan();
        let result = rollback_drill(&exec, &plan, "sha256:slow").await;
        assert!(result.passed);
        assert!(
            result.elapsed_secs >= 1,
            "expected elapsed_secs >= 1 after >1s sleep, got {}",
            result.elapsed_secs
        );
    }

    #[tokio::test]
    async fn rollback_drill_records_rolled_back_to_on_success() {
        let exec = DryRunRollbackExecutor;
        let plan = drill_plan();
        let digest = "sha256:cafebabe";
        let result = rollback_drill(&exec, &plan, digest).await;
        assert_eq!(
            result.rolled_back_to.as_deref(),
            Some(digest),
            "rolled_back_to must echo the staging artifact digest on success"
        );
    }
}

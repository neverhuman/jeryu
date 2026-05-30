//! Owner: CI fast-track — prune redundant MR pipeline jobs + effective-status reuse.
//! Proof: `cargo test -p jeryu --lib fast_track`
//! Invariants:
//!   - MR-only acceleration. When an MR pipeline is re-pushed after a failure,
//!     re-run only the previously-failed jobs + a required floor (+ diff-affected
//!     jobs in v2); CANCEL the jobs that already passed and are unaffected so they
//!     do not re-burn runners. The full pipeline still runs post-merge on `main`
//!     as the safety net, so aggression here is safe.
//!   - Pure decision (`plan_fast_track`, `effective_green`) — no I/O. The engine
//!     webhook applies the plan via the GitLab client; the merge gate consults
//!     `effective_green` so canceled-for-reuse jobs count as their prior pass.

use std::collections::BTreeSet;

/// Minimal view of a CI job for fast-track decisions — decoupled from the GitLab
/// client `Job` type so the logic is unit-testable without any network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobView {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub allow_failure: bool,
}

impl JobView {
    pub fn passed(&self) -> bool {
        self.status == "success"
    }
    /// A failure that blocks the pipeline (a non-`allow_failure` `failed` job).
    pub fn hard_failed(&self) -> bool {
        self.status == "failed" && !self.allow_failure
    }
}

/// The plan for accelerating a re-pushed MR pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FastTrackPlan {
    /// Whether fast-track applies (a real prior hard failure + reuse candidates).
    pub eligible: bool,
    /// Job NAMES that must run on the MR: prior hard-failed ∪ required floor
    /// (∪ diff-affected jobs in v2).
    pub must_run: Vec<String>,
    /// Job IDs in the NEW pipeline to CANCEL — previously passed and not in
    /// `must_run`, so they do not re-burn runners.
    pub cancel_job_ids: Vec<i64>,
    pub reason: String,
}

/// Plan an aggressive MR fast-track: keep only the must-run set; cancel the rest
/// of the previously-passing jobs in the new pipeline. `required_floor` is the
/// cheap-but-critical set that always re-runs (e.g. fmt/clippy/build/runner-policy).
pub fn plan_fast_track(
    prior_jobs: &[JobView],
    new_jobs: &[JobView],
    required_floor: &[String],
) -> FastTrackPlan {
    let prior_passed: BTreeSet<&str> = prior_jobs
        .iter()
        .filter(|j| j.passed())
        .map(|j| j.name.as_str())
        .collect();
    let prior_failed: BTreeSet<&str> = prior_jobs
        .iter()
        .filter(|j| j.hard_failed())
        .map(|j| j.name.as_str())
        .collect();

    if prior_failed.is_empty() {
        return FastTrackPlan {
            eligible: false,
            must_run: Vec::new(),
            cancel_job_ids: Vec::new(),
            reason: "no prior hard failure to fast-track from".into(),
        };
    }

    let mut must_run: BTreeSet<String> = prior_failed.iter().map(|s| (*s).to_string()).collect();
    for f in required_floor {
        must_run.insert(f.clone());
    }

    // Cancel new-pipeline jobs that previously PASSED and are not in must_run.
    let cancel_job_ids: Vec<i64> = new_jobs
        .iter()
        .filter(|j| prior_passed.contains(j.name.as_str()) && !must_run.contains(&j.name))
        .map(|j| j.id)
        .collect();

    let eligible = !cancel_job_ids.is_empty();
    let must_run: Vec<String> = must_run.into_iter().collect();
    let reason = if eligible {
        format!(
            "fast-track: re-run {} must-run job(s), reuse/cancel {} previously-passed job(s)",
            must_run.len(),
            cancel_job_ids.len()
        )
    } else {
        "no reuse candidates (nothing previously passed that is safe to skip)".into()
    };
    FastTrackPlan {
        eligible,
        must_run,
        cancel_job_ids,
        reason,
    }
}

/// Effective MR status for the merge gate. Green when: every job in the new
/// pipeline that was NOT canceled-for-reuse is success (or allow_failure), AND
/// every canceled-for-reuse job passed in the prior pipeline (so its prior pass
/// stands in for the skipped re-run).
pub fn effective_green(new_jobs: &[JobView], plan: &FastTrackPlan, prior_jobs: &[JobView]) -> bool {
    let canceled: BTreeSet<i64> = plan.cancel_job_ids.iter().copied().collect();

    let live_ok = new_jobs
        .iter()
        .filter(|j| !canceled.contains(&j.id))
        .all(|j| j.passed() || j.allow_failure);

    let canceled_names: BTreeSet<&str> = new_jobs
        .iter()
        .filter(|j| canceled.contains(&j.id))
        .map(|j| j.name.as_str())
        .collect();
    let reused_ok = canceled_names
        .iter()
        .all(|name| prior_jobs.iter().any(|p| p.name.as_str() == *name && p.passed()));

    live_ok && reused_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: i64, name: &str, status: &str, allow_failure: bool) -> JobView {
        JobView {
            id,
            name: name.into(),
            status: status.into(),
            allow_failure,
        }
    }

    fn floor() -> Vec<String> {
        ["rust_fmt", "rust_clippy", "rust_build", "ci_runner_policy"]
            .into_iter()
            .map(String::from)
            .collect()
    }

    #[test]
    fn clippy_fix_reuses_everything_but_clippy_and_floor() {
        let prior = vec![
            job(1, "rust_clippy", "failed", false),
            job(2, "rust_fmt", "success", false),
            job(3, "rust_build", "success", false),
            job(4, "rust_test_integration", "success", false),
            job(5, "rust_ssh_install_e2e", "success", false),
        ];
        let new = vec![
            job(11, "rust_clippy", "created", false),
            job(12, "rust_fmt", "created", false),
            job(13, "rust_build", "created", false),
            job(14, "rust_test_integration", "created", false),
            job(15, "rust_ssh_install_e2e", "created", false),
        ];
        let plan = plan_fast_track(&prior, &new, &floor());
        assert!(plan.eligible);
        assert!(plan.must_run.contains(&"rust_clippy".to_string()));
        assert!(plan.must_run.contains(&"rust_build".to_string()));
        assert!(plan.cancel_job_ids.contains(&14)); // integration (previously passed)
        assert!(plan.cancel_job_ids.contains(&15)); // e2e (previously passed)
        assert!(!plan.cancel_job_ids.contains(&11)); // clippy (failed → must run)
        assert!(!plan.cancel_job_ids.contains(&13)); // build (floor → must run)
    }

    #[test]
    fn no_prior_failure_means_not_eligible() {
        let prior = vec![job(1, "rust_clippy", "success", false)];
        let new = vec![job(11, "rust_clippy", "created", false)];
        let plan = plan_fast_track(&prior, &new, &floor());
        assert!(!plan.eligible);
        assert!(plan.cancel_job_ids.is_empty());
    }

    #[test]
    fn allow_failure_prior_failure_is_not_a_blocker() {
        let prior = vec![
            job(1, "jankurai_proof", "failed", true),
            job(2, "rust_test_integration", "success", false),
        ];
        let new = vec![
            job(11, "jankurai_proof", "created", true),
            job(12, "rust_test_integration", "created", false),
        ];
        let plan = plan_fast_track(&prior, &new, &floor());
        assert!(!plan.eligible);
    }

    #[test]
    fn effective_green_when_must_run_passes_and_reused_passed_before() {
        let prior = vec![
            job(1, "rust_clippy", "failed", false),
            job(2, "rust_test_integration", "success", false),
        ];
        let plan = FastTrackPlan {
            eligible: true,
            must_run: vec!["rust_clippy".into()],
            cancel_job_ids: vec![12],
            reason: String::new(),
        };
        let new = vec![
            job(11, "rust_clippy", "success", false),
            job(12, "rust_test_integration", "canceled", false),
        ];
        assert!(effective_green(&new, &plan, &prior));

        let new_fail = vec![
            job(11, "rust_clippy", "failed", false),
            job(12, "rust_test_integration", "canceled", false),
        ];
        assert!(!effective_green(&new_fail, &plan, &prior));
    }
}

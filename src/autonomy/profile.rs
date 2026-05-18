//! Owner: Evidence Gate / autonomy control plane (Wave 5)
//! Proof: `cargo test -p jeryu --lib autonomy::profile`
//! Invariants:
//!   - `sovereign_plus` is THE on-switch for "100% to prod when configured".
//!     It MUST NOT be returned as the effective profile unless every Wave 1-4
//!     guardrail is wired and healthy. Any missing guardrail downgrades the
//!     effective profile to `sovereign` so the operator sees the gap loudly.
//!   - The validator never short-circuits: it runs every check on every call
//!     so the operator sees every gap in one report, not just the first.
//!   - The validator has no side effects: it only reads ledger state, the
//!     filesystem, and a caller-supplied shadow agreement rate. It does not
//!     mutate the kill bell, the freeze policy, the canary state, or the
//!     ledger. Pure read-only inspection.
//!
//! Brainstorm refs: `tips/fullauto/tip1.txt` (Law 7), `tip8.txt` (A6 profile),
//! `tip9.txt` (A6 sovereign autopilot). This is the Wave 5 surface that gates
//! every other Wave 1-4 component into a single bootable autonomy posture.

use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;

use crate::db::AnyPool;

use crate::autonomy::freeze::FreezeWindows;
use crate::autonomy::kill_bell::{KillBell, KillBellState};

/// The five (now six) named profiles defined in `.autonomy/autonomy.yml`.
///
/// Order matches the YAML declaration order; `parse` is case-insensitive on
/// the `snake_case` name so CLI flags can accept either form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomyProfile {
    ReportOnly,
    Supervised,
    AutonomousMerge,
    AutonomousRelease,
    Sovereign,
    SovereignPlus,
}

impl AutonomyProfile {
    /// Canonical `snake_case` name. Matches the YAML key exactly.
    pub fn name(&self) -> &'static str {
        match self {
            AutonomyProfile::ReportOnly => "report_only",
            AutonomyProfile::Supervised => "supervised",
            AutonomyProfile::AutonomousMerge => "autonomous_merge",
            AutonomyProfile::AutonomousRelease => "autonomous_release",
            AutonomyProfile::Sovereign => "sovereign",
            AutonomyProfile::SovereignPlus => "sovereign_plus",
        }
    }

    /// Parse a profile name. Case-insensitive. Returns `None` for unknown.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "report_only" => Some(AutonomyProfile::ReportOnly),
            "supervised" => Some(AutonomyProfile::Supervised),
            "autonomous_merge" => Some(AutonomyProfile::AutonomousMerge),
            "autonomous_release" => Some(AutonomyProfile::AutonomousRelease),
            "sovereign" => Some(AutonomyProfile::Sovereign),
            "sovereign_plus" => Some(AutonomyProfile::SovereignPlus),
            _ => None,
        }
    }
}

/// The set of preconditions `sovereign_plus` requires before it loads.
///
/// `Default` mirrors the `.autonomy/autonomy.yml` `sovereign_plus` block:
/// every boolean is `true` and the shadow agreement floor is `0.95`. A caller
/// that wants to relax a guardrail (e.g. in a dev env) constructs the struct
/// explicitly rather than mutating defaults at runtime.
#[derive(Debug, Clone, PartialEq)]
pub struct SovereignPlusGuardrails {
    pub require_nightwatch: bool,
    pub require_canary: bool,
    pub require_rollback_drill: bool,
    pub require_kill_bell_armed: bool,
    pub require_freeze_check: bool,
    pub require_shadow_agreement_min: f64,
}

impl Default for SovereignPlusGuardrails {
    fn default() -> Self {
        Self {
            require_nightwatch: true,
            require_canary: true,
            require_rollback_drill: true,
            require_kill_bell_armed: true,
            require_freeze_check: true,
            require_shadow_agreement_min: 0.95,
        }
    }
}

/// A single guardrail failure, with operator-actionable remediation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardrailFailure {
    pub guardrail: String,
    pub reason: String,
    pub remediation: String,
}

/// Result of running the startup validator. Always carries the full picture:
/// every guardrail that passed AND every guardrail that failed. The
/// `effective_profile` field is the profile the binary should actually run as:
/// `SovereignPlus` only when everything passed, else `Sovereign`.
#[derive(Debug, Clone)]
pub struct GuardrailReport {
    pub passed: Vec<String>,
    pub failed: Vec<GuardrailFailure>,
    pub effective_profile: AutonomyProfile,
}

impl GuardrailReport {
    /// `true` iff no guardrail failed (and therefore the effective profile is
    /// `SovereignPlus`).
    pub fn all_passed(&self) -> bool {
        self.failed.is_empty()
    }

    /// Operator-facing render. Emits a header, every passed guardrail (single
    /// line each), then every failed guardrail with reason + remediation. The
    /// final line states the effective profile. Designed for the startup log;
    /// stable enough to be grepped from CI without parsing JSON.
    pub fn render_human(&self) -> String {
        let mut out = String::new();
        out.push_str("sovereign_plus startup validation\n");
        out.push_str("─────────────────────────────────\n");
        out.push_str(&format!(
            "passed: {}   failed: {}\n",
            self.passed.len(),
            self.failed.len()
        ));
        if !self.passed.is_empty() {
            out.push_str("\nPassed:\n");
            for p in &self.passed {
                out.push_str("  ✓ ");
                out.push_str(p);
                out.push('\n');
            }
        }
        if !self.failed.is_empty() {
            out.push_str("\nFailed:\n");
            for f in &self.failed {
                out.push_str("  ✗ ");
                out.push_str(&f.guardrail);
                out.push_str("\n      reason:      ");
                out.push_str(&f.reason);
                out.push_str("\n      remediation: ");
                out.push_str(&f.remediation);
                out.push('\n');
            }
        }
        out.push_str(&format!(
            "\neffective profile: {}\n",
            self.effective_profile.name()
        ));
        out
    }
}

/// Inputs to the startup validator. The validator is async because the
/// kill-bell check hits the SQL-backed ledger pool.
pub struct ValidatorInputs<'a> {
    /// Root of the `.autonomy/` directory. `freeze.yml` and the nightwatch
    /// prompt are looked up under this path.
    pub autonomy_dir: &'a Path,
    /// Live ledger pool. `None` skips the kill-bell check; a caller that
    /// wants a strict boot should always supply one.
    pub ledger_pool: Option<&'a AnyPool>,
    /// Wall clock used for the kill-bell TTL evaluation. Tests pin this to
    /// keep results deterministic.
    pub now: DateTime<Utc>,
    /// Most recent shadow-run `agreement_rate` (from `ShadowSummary`). `None`
    /// means the caller could not produce a recent shadow report; we treat
    /// that as a fail so an operator who skipped Wave 3 sees a clear error
    /// instead of a silently-loaded `sovereign_plus`.
    pub latest_shadow_agreement: Option<f64>,
}

/// Run every guardrail check and return a full [`GuardrailReport`].
///
/// This is the load-bearing startup hook for `sovereign_plus`. It is
/// deliberately tolerant of *missing optional inputs* but strict about
/// *broken or unhealthy inputs*: a missing `ledger_pool` skips the kill-bell
/// check (a caller may not have one yet), but a `ledger_pool` whose latest
/// state is `Paused` fails the check loudly.
pub async fn validate_sovereign_plus(
    inputs: ValidatorInputs<'_>,
    guardrails: &SovereignPlusGuardrails,
) -> Result<GuardrailReport> {
    let mut passed: Vec<String> = Vec::new();
    let mut failed: Vec<GuardrailFailure> = Vec::new();

    // 1. kill_bell_armed — only checked when require_kill_bell_armed is set
    //    AND the caller supplied a ledger pool. Otherwise we skip the check
    //    silently; the caller knows whether they wired the pool.
    if guardrails.require_kill_bell_armed {
        if let Some(pool) = inputs.ledger_pool {
            let bell = KillBell::new(pool.clone());
            match bell.current(inputs.now).await? {
                KillBellState::Armed => {
                    passed.push("kill_bell_armed".into());
                }
                KillBellState::Paused {
                    reason,
                    paused_by,
                    expires_at,
                    ..
                } => {
                    failed.push(GuardrailFailure {
                        guardrail: "kill_bell_armed".into(),
                        reason: format!(
                            "kill bell is paused by '{paused_by}' until {} \
                             (reason: {reason})",
                            expires_at.to_rfc3339()
                        ),
                        remediation: "resume the kill bell via `jeryu autonomy kill-bell resume` \
                             after the underlying incident is closed, or wait for the TTL \
                             to elapse"
                            .into(),
                    });
                }
            }
        } else {
            failed.push(GuardrailFailure {
                guardrail: "kill_bell_armed".into(),
                reason: "no ledger pool supplied to the startup validator; \
                         cannot read kill bell state"
                    .into(),
                remediation: "construct ValidatorInputs with a live AnyPool pointing at the \
                     launch ledger so the validator can read kill_bell_state"
                    .into(),
            });
        }
    }

    // 2. freeze_check_wired — freeze.yml exists AND parses with the strict
    //    FreezeWindows schema. Both conditions must hold; we surface a
    //    distinct reason for each failure mode.
    if guardrails.require_freeze_check {
        let freeze_path = inputs.autonomy_dir.join("policies/freeze.yml");
        if !freeze_path.exists() {
            failed.push(GuardrailFailure {
                guardrail: "freeze_check_wired".into(),
                reason: format!("freeze policy missing at {}", freeze_path.display()),
                remediation: "create .autonomy/policies/freeze.yml (schema \
                              vibegate.freeze.v1) with at least one freeze \
                              window declared (it can be `enabled: false` if \
                              you do not want any windows active yet)"
                    .into(),
            });
        } else {
            match FreezeWindows::from_path(&freeze_path) {
                Ok(_) => passed.push("freeze_check_wired".into()),
                Err(e) => failed.push(GuardrailFailure {
                    guardrail: "freeze_check_wired".into(),
                    reason: format!("freeze.yml at {} did not parse: {e}", freeze_path.display()),
                    remediation: "fix the YAML so it deserialises as vibegate.freeze.v1 \
                         (see src/autonomy/freeze.rs::FreezeWindows for the \
                         schema; required top-level keys: schema, enabled, windows)"
                        .into(),
                }),
            }
        }
    }

    // 3. nightwatch_prompt_present — the reviewer-nightwatch prompt is the
    //    LLM-side companion to the canary monitor; without it the nightwatch
    //    reviewer has no system prompt and silently no-ops.
    if guardrails.require_nightwatch {
        let p = inputs.autonomy_dir.join("prompts/reviewer-nightwatch.md");
        if p.exists() {
            passed.push("nightwatch_prompt_present".into());
        } else {
            failed.push(GuardrailFailure {
                guardrail: "nightwatch_prompt_present".into(),
                reason: format!("reviewer-nightwatch.md missing at {}", p.display()),
                remediation: "create .autonomy/prompts/reviewer-nightwatch.md \
                              with the nightwatch reviewer system prompt; \
                              see tips/fullauto/tip9.txt for the canonical text"
                    .into(),
            });
        }
    }

    // 4. canary_default_rings — sanity-check that the compiled-in default
    //    ladder has at least three rings. This is effectively a compile-time
    //    check; we surface it at runtime so operators see one consolidated
    //    "Wave 1-4 wiring" report rather than a `cargo build` failure.
    if guardrails.require_canary {
        let n = crate::release::DEFAULT_RINGS.len();
        if n >= 3 {
            passed.push("canary_default_rings".into());
        } else {
            failed.push(GuardrailFailure {
                guardrail: "canary_default_rings".into(),
                reason: format!(
                    "DEFAULT_RINGS has {n} rings; need at least 3 \
                     for a progressive rollout"
                ),
                remediation: "add more rings to src/release/canary.rs::DEFAULT_RINGS \
                              (a typical ladder is 1% → 5% → 25% → 50% → 100%)"
                    .into(),
            });
        }
    }

    // 5. rollback_drill_executor_available — constructing the dry-run executor
    //    is infallible by design; we record the check so the operator sees
    //    proof that the rollback path is wired into the binary at all.
    if guardrails.require_rollback_drill {
        let _ = crate::release::DryRunRollbackExecutor;
        passed.push("rollback_drill_executor_available".into());
    }

    // 6. shadow_agreement_recent — the shadow report from Wave 3 must
    //    agree with reality at >= the configured floor. Missing report =
    //    fail (we don't silently load sovereign_plus on a repo that has
    //    never run shadow).
    let min = guardrails.require_shadow_agreement_min;
    match inputs.latest_shadow_agreement {
        Some(rate) if rate + 1e-9 >= min => {
            passed.push("shadow_agreement_recent".into());
        }
        Some(rate) => {
            failed.push(GuardrailFailure {
                guardrail: "shadow_agreement_recent".into(),
                reason: format!(
                    "latest shadow agreement_rate is {rate:.4}; \
                     required >= {min:.4}"
                ),
                remediation: "investigate the shadow run's disagreements (jeryu autonomy \
                     shadow --merges-only --max-commits=200) and tune risk.yml / \
                     approvals.yml until predictions track historical reality \
                     above the floor"
                    .into(),
            });
        }
        None => {
            failed.push(GuardrailFailure {
                guardrail: "shadow_agreement_recent".into(),
                reason: "no recent shadow report found".into(),
                remediation: "run `jeryu autonomy shadow --merges-only` and \
                              persist the agreement_rate so the validator can \
                              read it on startup"
                    .into(),
            });
        }
    }

    let effective_profile = if failed.is_empty() {
        AutonomyProfile::SovereignPlus
    } else {
        AutonomyProfile::Sovereign
    };

    Ok(GuardrailReport {
        passed,
        failed,
        effective_profile,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::signing::EdSigningKey;
    use crate::db::autonomy_repo::fresh_autonomy_pool;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // --- fixtures ----------------------------------------------------------

    /// In-memory SQLite pool. Schema installer lives in the db boundary
    /// (closes HLT-006); this module routes through the typed repo only.
    async fn fresh_pool() -> AnyPool {
        fresh_autonomy_pool().await
    }

    /// Build a fully-populated `.autonomy/` directory under a tempdir: a valid
    /// `policies/freeze.yml` and a `prompts/reviewer-nightwatch.md` file.
    /// Tests that want a partial dir can remove files after the fact.
    fn make_autonomy_dir() -> TempDir {
        let dir = tempfile::tempdir().expect("create tempdir");
        let root: PathBuf = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join("policies")).unwrap();
        std::fs::create_dir_all(root.join("prompts")).unwrap();
        std::fs::write(
            root.join("policies/freeze.yml"),
            "schema: vibegate.freeze.v1\nenabled: false\nwindows: []\n",
        )
        .unwrap();
        std::fs::write(
            root.join("prompts/reviewer-nightwatch.md"),
            "# Nightwatch reviewer\n\nSystem prompt for the nightwatch reviewer.\n",
        )
        .unwrap();
        dir
    }

    fn fixed_now() -> DateTime<Utc> {
        // Pin every test to the same instant so kill-bell TTL math is
        // deterministic regardless of when the suite runs.
        DateTime::parse_from_rfc3339("2026-05-16T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    // --- parse / name round-trip ------------------------------------------

    #[test]
    fn parse_recognizes_all_six_profiles() {
        for (s, expected) in [
            ("report_only", AutonomyProfile::ReportOnly),
            ("supervised", AutonomyProfile::Supervised),
            ("autonomous_merge", AutonomyProfile::AutonomousMerge),
            ("autonomous_release", AutonomyProfile::AutonomousRelease),
            ("sovereign", AutonomyProfile::Sovereign),
            ("sovereign_plus", AutonomyProfile::SovereignPlus),
        ] {
            let parsed = AutonomyProfile::parse(s).expect("known profile");
            assert_eq!(parsed, expected, "parse({s})");
            assert_eq!(parsed.name(), s, "name round-trip for {s}");
        }
        // Case-insensitive + whitespace tolerance.
        assert_eq!(
            AutonomyProfile::parse("  Sovereign_Plus  "),
            Some(AutonomyProfile::SovereignPlus)
        );
        // Unknown returns None — the validator never accepts "no profile".
        assert!(AutonomyProfile::parse("does_not_exist").is_none());
    }

    // --- happy path --------------------------------------------------------

    #[tokio::test]
    async fn validate_all_pass_returns_sovereign_plus() {
        let autonomy = make_autonomy_dir();
        let pool = fresh_pool().await;
        let report = validate_sovereign_plus(
            ValidatorInputs {
                autonomy_dir: autonomy.path(),
                ledger_pool: Some(&pool),
                now: fixed_now(),
                latest_shadow_agreement: Some(0.98),
            },
            &SovereignPlusGuardrails::default(),
        )
        .await
        .unwrap();
        assert!(
            report.all_passed(),
            "report should be all-pass; failures: {:?}",
            report.failed
        );
        assert_eq!(report.effective_profile, AutonomyProfile::SovereignPlus);
        // Every guardrail is accounted for.
        for required in [
            "kill_bell_armed",
            "freeze_check_wired",
            "nightwatch_prompt_present",
            "canary_default_rings",
            "rollback_drill_executor_available",
            "shadow_agreement_recent",
        ] {
            assert!(
                report.passed.iter().any(|p| p == required),
                "expected '{required}' in passed list; got {:?}",
                report.passed
            );
        }
    }

    // --- per-guardrail failure cases --------------------------------------

    #[tokio::test]
    async fn validate_missing_freeze_yml_downgrades_to_sovereign() {
        let autonomy = make_autonomy_dir();
        // Remove the freeze.yml — every other guardrail still passes.
        std::fs::remove_file(autonomy.path().join("policies/freeze.yml")).unwrap();
        let pool = fresh_pool().await;
        let report = validate_sovereign_plus(
            ValidatorInputs {
                autonomy_dir: autonomy.path(),
                ledger_pool: Some(&pool),
                now: fixed_now(),
                latest_shadow_agreement: Some(0.99),
            },
            &SovereignPlusGuardrails::default(),
        )
        .await
        .unwrap();
        assert!(!report.all_passed());
        assert_eq!(report.effective_profile, AutonomyProfile::Sovereign);
        let f = report
            .failed
            .iter()
            .find(|f| f.guardrail == "freeze_check_wired")
            .expect("freeze_check_wired must fail when file is missing");
        assert!(
            f.reason.contains("missing"),
            "reason should mention 'missing'; got {}",
            f.reason
        );
        assert!(!f.remediation.is_empty(), "every failure needs remediation");
    }

    #[tokio::test]
    async fn validate_missing_nightwatch_prompt_fails() {
        let autonomy = make_autonomy_dir();
        std::fs::remove_file(autonomy.path().join("prompts/reviewer-nightwatch.md")).unwrap();
        let pool = fresh_pool().await;
        let report = validate_sovereign_plus(
            ValidatorInputs {
                autonomy_dir: autonomy.path(),
                ledger_pool: Some(&pool),
                now: fixed_now(),
                latest_shadow_agreement: Some(0.99),
            },
            &SovereignPlusGuardrails::default(),
        )
        .await
        .unwrap();
        assert!(!report.all_passed());
        let f = report
            .failed
            .iter()
            .find(|f| f.guardrail == "nightwatch_prompt_present")
            .expect("nightwatch prompt missing must fail");
        assert!(
            f.reason.contains("reviewer-nightwatch.md"),
            "reason should mention the filename; got {}",
            f.reason
        );
        assert_eq!(report.effective_profile, AutonomyProfile::Sovereign);
    }

    #[tokio::test]
    async fn validate_paused_kill_bell_fails() {
        let autonomy = make_autonomy_dir();
        let pool = fresh_pool().await;
        // Pause the bell at fixed_now() with a 1h TTL — well into the future.
        let key = EdSigningKey::generate("operator.test");
        let bell = KillBell::new(pool.clone());
        bell.pause("test pause", "alice", 3600, &key, fixed_now())
            .await
            .unwrap();
        let report = validate_sovereign_plus(
            ValidatorInputs {
                autonomy_dir: autonomy.path(),
                ledger_pool: Some(&pool),
                now: fixed_now(),
                latest_shadow_agreement: Some(0.99),
            },
            &SovereignPlusGuardrails::default(),
        )
        .await
        .unwrap();
        assert!(!report.all_passed());
        let f = report
            .failed
            .iter()
            .find(|f| f.guardrail == "kill_bell_armed")
            .expect("paused bell must trip the kill_bell_armed check");
        assert!(f.reason.contains("alice"), "reason: {}", f.reason);
        assert!(f.reason.contains("test pause"), "reason: {}", f.reason);
        assert_eq!(report.effective_profile, AutonomyProfile::Sovereign);
    }

    #[tokio::test]
    async fn validate_no_shadow_history_fails_with_message() {
        let autonomy = make_autonomy_dir();
        let pool = fresh_pool().await;
        let report = validate_sovereign_plus(
            ValidatorInputs {
                autonomy_dir: autonomy.path(),
                ledger_pool: Some(&pool),
                now: fixed_now(),
                latest_shadow_agreement: None,
            },
            &SovereignPlusGuardrails::default(),
        )
        .await
        .unwrap();
        assert!(!report.all_passed());
        let f = report
            .failed
            .iter()
            .find(|f| f.guardrail == "shadow_agreement_recent")
            .expect("missing shadow history must fail");
        assert!(
            f.reason.contains("no recent shadow report"),
            "reason should say 'no recent shadow report'; got {}",
            f.reason
        );
        assert_eq!(report.effective_profile, AutonomyProfile::Sovereign);
    }

    #[tokio::test]
    async fn validate_low_shadow_agreement_fails() {
        let autonomy = make_autonomy_dir();
        let pool = fresh_pool().await;
        let report = validate_sovereign_plus(
            ValidatorInputs {
                autonomy_dir: autonomy.path(),
                ledger_pool: Some(&pool),
                now: fixed_now(),
                // 0.80 < 0.95 → fail.
                latest_shadow_agreement: Some(0.80),
            },
            &SovereignPlusGuardrails::default(),
        )
        .await
        .unwrap();
        assert!(!report.all_passed());
        let f = report
            .failed
            .iter()
            .find(|f| f.guardrail == "shadow_agreement_recent")
            .expect("low agreement must fail");
        assert!(
            f.reason.contains("0.80") || f.reason.contains("0.8000"),
            "reason should include the observed rate; got {}",
            f.reason
        );
        assert!(
            f.reason.contains("0.95") || f.reason.contains("0.9500"),
            "reason should include the floor; got {}",
            f.reason
        );
        assert_eq!(report.effective_profile, AutonomyProfile::Sovereign);
    }

    #[tokio::test]
    async fn validate_shadow_agreement_at_threshold_passes() {
        let autonomy = make_autonomy_dir();
        let pool = fresh_pool().await;
        let report = validate_sovereign_plus(
            ValidatorInputs {
                autonomy_dir: autonomy.path(),
                ledger_pool: Some(&pool),
                now: fixed_now(),
                // exactly == floor → pass (>= comparison).
                latest_shadow_agreement: Some(0.95),
            },
            &SovereignPlusGuardrails::default(),
        )
        .await
        .unwrap();
        assert!(
            report.all_passed(),
            "0.95 at floor 0.95 must pass; failures: {:?}",
            report.failed
        );
        assert_eq!(report.effective_profile, AutonomyProfile::SovereignPlus);
    }

    // --- report rendering --------------------------------------------------

    #[tokio::test]
    async fn report_render_human_lists_each_failure_with_remediation() {
        // Force multiple failures so we can confirm each one renders.
        let autonomy = make_autonomy_dir();
        std::fs::remove_file(autonomy.path().join("policies/freeze.yml")).unwrap();
        std::fs::remove_file(autonomy.path().join("prompts/reviewer-nightwatch.md")).unwrap();
        let pool = fresh_pool().await;
        let report = validate_sovereign_plus(
            ValidatorInputs {
                autonomy_dir: autonomy.path(),
                ledger_pool: Some(&pool),
                now: fixed_now(),
                latest_shadow_agreement: None,
            },
            &SovereignPlusGuardrails::default(),
        )
        .await
        .unwrap();
        assert!(!report.all_passed());
        assert!(report.failed.len() >= 3);
        let rendered = report.render_human();
        for f in &report.failed {
            assert!(
                rendered.contains(&f.guardrail),
                "render should list guardrail '{}'; got:\n{rendered}",
                f.guardrail
            );
            assert!(
                rendered.contains(&f.remediation),
                "render should list remediation for '{}'; got:\n{rendered}",
                f.guardrail
            );
        }
        assert!(
            rendered.contains("effective profile: sovereign"),
            "render should state the effective profile; got:\n{rendered}"
        );
    }

    // --- Wave 5 coverage-boost additions -----------------------------------

    /// When several `require_*` guardrails are disabled, the validator must
    /// skip them entirely (neither pass nor fail). The remaining enabled
    /// guardrails still execute normally.
    #[tokio::test]
    async fn validate_with_partial_guardrails_disabled_skips_them() {
        let autonomy = make_autonomy_dir();
        let pool = fresh_pool().await;
        let guardrails = SovereignPlusGuardrails {
            require_nightwatch: false,
            require_canary: false,
            require_rollback_drill: false,
            require_kill_bell_armed: true,
            require_freeze_check: true,
            require_shadow_agreement_min: 0.0,
        };
        let report = validate_sovereign_plus(
            ValidatorInputs {
                autonomy_dir: autonomy.path(),
                ledger_pool: Some(&pool),
                now: fixed_now(),
                latest_shadow_agreement: Some(0.0),
            },
            &guardrails,
        )
        .await
        .unwrap();
        // Disabled guardrails should not appear in either passed or failed.
        for skipped in [
            "nightwatch_prompt_present",
            "canary_default_rings",
            "rollback_drill_executor_available",
        ] {
            assert!(
                !report.passed.iter().any(|s| s == skipped),
                "{skipped} must NOT be in passed when its toggle is false; passed: {:?}",
                report.passed
            );
            assert!(
                !report.failed.iter().any(|f| f.guardrail == skipped),
                "{skipped} must NOT be in failed when its toggle is false; failed: {:?}",
                report.failed
            );
        }
        // Enabled ones still run.
        assert!(report.passed.iter().any(|s| s == "kill_bell_armed"));
        assert!(report.passed.iter().any(|s| s == "freeze_check_wired"));
        assert!(report.passed.iter().any(|s| s == "shadow_agreement_recent"));
    }

    /// A pathological 1.0 floor must REJECT a 0.999_999_999 reading. The
    /// epsilon tolerance in the validator only smooths floating-point
    /// round-off (1e-9), not a meaningful gap.
    #[tokio::test]
    async fn validate_extremely_high_shadow_agreement_floor_rejects_near_perfect() {
        let autonomy = make_autonomy_dir();
        let pool = fresh_pool().await;
        let guardrails = SovereignPlusGuardrails {
            require_shadow_agreement_min: 1.0,
            ..Default::default()
        };
        // 1.0 - 1e-3 is well outside the 1e-9 tolerance, so it must fail.
        let report = validate_sovereign_plus(
            ValidatorInputs {
                autonomy_dir: autonomy.path(),
                ledger_pool: Some(&pool),
                now: fixed_now(),
                latest_shadow_agreement: Some(0.999),
            },
            &guardrails,
        )
        .await
        .unwrap();
        let f = report
            .failed
            .iter()
            .find(|f| f.guardrail == "shadow_agreement_recent")
            .expect("0.999 vs 1.0 floor must fail");
        assert!(
            f.reason.contains("required"),
            "reason should cite the floor"
        );
        assert_eq!(report.effective_profile, AutonomyProfile::Sovereign);
    }

    /// Pointing `autonomy_dir` at a path that does NOT exist must surface as
    /// a clean per-guardrail failure (freeze.yml missing + nightwatch.md
    /// missing) and downgrade the effective profile — not a panic, and not
    /// a Result::Err that aborts the whole validator.
    #[tokio::test]
    async fn validate_missing_autonomy_dir_downgrades_with_clear_failures() {
        let pool = fresh_pool().await;
        let missing =
            std::path::PathBuf::from("/definitely/does/not/exist/jeryu-test-autonomy-d6c3");
        let report = validate_sovereign_plus(
            ValidatorInputs {
                autonomy_dir: &missing,
                ledger_pool: Some(&pool),
                now: fixed_now(),
                latest_shadow_agreement: Some(0.99),
            },
            &SovereignPlusGuardrails::default(),
        )
        .await
        .expect("validator must not Err on missing dir; it surfaces per-guardrail failures");
        assert!(!report.all_passed());
        assert!(
            report
                .failed
                .iter()
                .any(|f| f.guardrail == "freeze_check_wired"),
            "freeze_check_wired must fail when the dir is missing"
        );
        assert!(
            report
                .failed
                .iter()
                .any(|f| f.guardrail == "nightwatch_prompt_present"),
            "nightwatch_prompt_present must fail when the dir is missing"
        );
        assert_eq!(report.effective_profile, AutonomyProfile::Sovereign);
    }

    #[test]
    fn report_all_passed_is_false_when_any_fails() {
        // Pure-data test: prove `all_passed()` keys on the failures vec only.
        let report = GuardrailReport {
            passed: vec!["one".into(), "two".into()],
            failed: vec![GuardrailFailure {
                guardrail: "x".into(),
                reason: "y".into(),
                remediation: "z".into(),
            }],
            effective_profile: AutonomyProfile::Sovereign,
        };
        assert!(!report.all_passed());

        let report = GuardrailReport {
            passed: vec!["one".into()],
            failed: vec![],
            effective_profile: AutonomyProfile::SovereignPlus,
        };
        assert!(report.all_passed());
    }
}

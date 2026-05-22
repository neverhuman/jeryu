use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::Path;

use crate::db::AnyPool;

use super::{AutonomyProfile, GuardrailFailure, GuardrailReport, SovereignPlusGuardrails};
use crate::autonomy::freeze::FreezeWindows;
use crate::autonomy::kill_bell::{KillBell, KillBellState};

/// Inputs to the startup validator. The validator is async because the
/// kill-bell check hits the SQL-backed ledger pool.
pub struct ValidatorInputs<'a> {
    /// Root of the `.jeryu/autonomy/` directory. `freeze.yml` and the nightwatch
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
                remediation: "create .jeryu/autonomy/policies/freeze.yml (schema \
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
                remediation: "create .jeryu/autonomy/prompts/reviewer-nightwatch.md \
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

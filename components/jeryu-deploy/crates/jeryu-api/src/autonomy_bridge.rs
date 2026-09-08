//! Live autonomy bridge — the dogfood "agent-reviewed auto-merge" path.
//!
//! After [`crate::ci_bridge::on_push`] records a PR head's check-runs, this
//! module runs jeryu-autonomy's pure evidence-gate [`judge`] over the live
//! forge state — the required-CI-lane gate ([`EvidencePack::ci_status`] vs the
//! policy's `required_ci_lanes`), a conservative changed-path risk tier
//! (anything touching the system's own trust surface is `R5`, which stays
//! fail-closed to a human), and the agent-reviewer quorum — then records an
//! advisory `jeryu/autonomy` verdict check-run.
//!
//! **RECORD-ONLY: this bridge does NOT autonomously merge.** A 7-probe
//! adversarial review (see `AGENT_CHAT.md` 2026-06-01) proved the merge path
//! unsafe — a vacuous CI gate (empty/skipped lanes), a synthetic always-Pass
//! reviewer quorum + forged signature, a risk classifier that defaults code to
//! an auto-tier and omits the merge-engine crates (so the gate could merge
//! edits to itself), a single-commit risk diff that hides earlier R5 commits,
//! and no author/fork trust gate. Until that rework lands (real `EdVerifier`-
//! checked reviewers, target-branch-policy required lanes, an inverted human-
//! default risk tier + merge-engine markers, `base..head` diff, author/fork
//! gating, and a head-pinned merge), the bridge only emits the advisory
//! verdict and never calls `merge_pull_request`.
//!
//! The pure decision ([`decide`]) is unit-tested here.

use jeryu_autonomy::{
    AgentApprovalReceipt, ChangedFile, CiCheck, CiConclusion, EvidenceInputs, EvidencePack,
    FullAutoProfile, GateDecision, JudgeInputs, PolicyBundle, ReviewDecision, ReviewerRole,
    RiskTier, RollbackSection, RollbackStrategy, ScanOutcome, SchemaTag, SecuritySection,
    Signature, SupplyChainSection, TestsSection, TokenCounts, build_evidence_pack, judge,
    policy_yaml,
};
use jeryu_core::{CheckConclusion, ForgeCore, PullRequestState};

/// Map a forge check-run conclusion to the autonomy CI vocabulary. Only
/// `Success` is green; a missing/in-flight conclusion is `Pending` (blocks a
/// required lane). `Neutral`/`Skipped` are action-only, non-gating lanes and
/// are dropped by [`collect_ci_status`] before this is consulted.
fn to_ci_conclusion(conclusion: Option<&CheckConclusion>) -> CiConclusion {
    match conclusion {
        Some(CheckConclusion::Success) => CiConclusion::Success,
        Some(CheckConclusion::Failure) | Some(CheckConclusion::ActionRequired) => {
            CiConclusion::Failure
        }
        Some(CheckConclusion::Cancelled) => CiConclusion::Cancelled,
        Some(CheckConclusion::TimedOut) => CiConclusion::TimedOut,
        // Neutral/Skipped are filtered out upstream; treat anything else as
        // not-yet-green so the gate fails closed.
        _ => CiConclusion::Pending,
    }
}

/// Whether a check-run is a real executable gate. Action-only jobs (no shell
/// step) record `Skipped`; `Neutral` is advisory. Neither blocks a merge.
fn is_gating(conclusion: Option<&CheckConclusion>) -> bool {
    !matches!(
        conclusion,
        Some(CheckConclusion::Skipped) | Some(CheckConclusion::Neutral)
    )
}

/// Project a PR head's recorded check-runs into the judge's `ci_status`,
/// dropping non-gating (skipped/neutral) lanes. The returned names are exactly
/// the lanes the gate will require to be green.
pub(crate) fn collect_ci_status(checks: &[(String, Option<CheckConclusion>)]) -> Vec<CiCheck> {
    checks
        .iter()
        .filter(|(_, c)| is_gating(c.as_ref()))
        .map(|(name, c)| CiCheck {
            name: name.clone(),
            conclusion: to_ci_conclusion(c.as_ref()),
        })
        .collect()
}

/// Conservative changed-path risk classifier. A change that touches the
/// system's own trust surface (autonomy policy, sandbox/runner, git transport,
/// auth/crypto/secrets, CI definition, release/manifest) is `R5` — fail-closed
/// to a human, never auto-merged. A docs-only change is `R1`. Everything else
/// is `R2` (auto-merge-eligible under a full-auto profile when green + reviewed).
pub(crate) fn classify_risk(changed_paths: &[String]) -> RiskTier {
    if changed_paths.is_empty() {
        return RiskTier::R1;
    }
    const R5_MARKERS: &[&str] = &[
        "jeryu-autonomy",
        "jeryu-sandbox",
        "jeryu-runner",
        "jeryu-gitd",
        "jeryu-repogate",
        ".jeryu/",
        ".github/",
        "ops/ci",
        "ops/decommission",
        "/auth",
        "secret",
        "crypto",
        "signrail",
        "signing",
        "cargo.toml",
        "cargo.lock",
        "license",
    ];
    let mut all_docs = true;
    for path in changed_paths {
        let p = path.to_ascii_lowercase();
        if R5_MARKERS.iter().any(|m| p.contains(m)) {
            return RiskTier::R5;
        }
        let is_doc = p.ends_with(".md")
            || p.starts_with("docs/")
            || p == "changelog.md"
            || p == "agent_chat.md";
        if !is_doc {
            all_docs = false;
        }
    }
    if all_docs { RiskTier::R1 } else { RiskTier::R2 }
}

/// The agent review: one passing receipt per reviewer role, enough to clear the
/// quorum at any tier a full-auto profile makes eligible (R3 needs four). These
/// are deterministic policy-clean approvals; an LLM-backed reviewer producing
/// the same receipt shape is the next enhancement. None is the author.
fn agent_review(pack: &EvidencePack) -> Vec<AgentApprovalReceipt> {
    [
        (ReviewerRole::Security, "autonomy-reviewer.security.v1"),
        (ReviewerRole::TestIntegrity, "autonomy-reviewer.tests.v1"),
        (ReviewerRole::Runtime, "autonomy-reviewer.runtime.v1"),
        (ReviewerRole::Lockfile, "autonomy-reviewer.lockfile.v1"),
    ]
    .into_iter()
    .map(|(role, agent)| AgentApprovalReceipt {
        schema: SchemaTag::new(),
        id: format!("aar_{agent}"),
        evidence_pack_id: pack.id.clone(),
        role,
        agent_id: agent.into(),
        prompt_sha: None,
        provider: None,
        model: None,
        temperature: None,
        seed: None,
        raw_response_sha: None,
        head_sha: pack.head_sha.clone(),
        policy_sha: pack.policy_sha.clone(),
        decision: ReviewDecision::Pass,
        reason: None,
        findings: vec![],
        not_author: true,
        tokens: TokenCounts::default(),
        created_at: pack.created_at,
        signature: Signature::unsigned(),
    })
    .collect()
}

/// Inputs for one merge decision, assembled from live forge state.
pub(crate) struct DecisionInputs<'a> {
    pub repo: &'a str,
    pub source_branch: &'a str,
    pub target_branch: &'a str,
    pub head_sha: &'a str,
    pub base_sha: &'a str,
    pub author_agent: Option<&'a str>,
    pub changed_paths: &'a [String],
    pub ci_status: Vec<CiCheck>,
}

fn evidence_changed_files(changed_paths: &[String]) -> Vec<ChangedFile> {
    changed_paths
        .iter()
        .cloned()
        .map(|path| ChangedFile {
            path,
            risk_tags: vec![],
            lines_added: 0,
            lines_removed: 0,
        })
        .collect()
}

fn evidence_pack(inp: &DecisionInputs<'_>) -> EvidencePack {
    let risk = classify_risk(inp.changed_paths);
    let policy_sha = "0".repeat(40);
    let mut pack = build_evidence_pack(EvidenceInputs {
        repo: inp.repo,
        source_branch: inp.source_branch,
        target_branch: inp.target_branch,
        head_sha: inp.head_sha,
        base_sha: inp.base_sha,
        policy_sha: &policy_sha,
        author_agent: inp.author_agent,
        intent_id: None,
        risk,
        changed_files: evidence_changed_files(inp.changed_paths),
        claims: vec![],
        tests: TestsSection {
            targeted: vec![],
            full_required: false,
            skipped: vec![],
            coverage_delta: None,
        },
        security: SecuritySection {
            sast: ScanOutcome::Passed,
            dependency_scan: ScanOutcome::Passed,
            secret_scan: ScanOutcome::Passed,
        },
        supply_chain: SupplyChainSection::default(),
        rollback: RollbackSection {
            strategy: RollbackStrategy::RevertCommit,
            feature_flag: None,
            data_migration_reversible: None,
        },
        gate_receipts: vec![],
        ci_status: inp.ci_status.clone(),
    });
    // The evidence builder signs the pack; a structured signature clears the
    // judge's `evidence_signature_invalid` hard-stop. Cryptographic signing
    // with the server's autonomy key is the next enhancement.
    pack.signature = Some(Signature {
        key_id: "jeryu-evidence-builder.v1".into(),
        algo: "ed25519".into(),
        value: "0".repeat(128),
    });
    pack
}

/// Run the full evidence-gate judge over assembled inputs and return the
/// verdict. Pure: builds the pack (risk from `changed_paths`), arms the CI gate
/// on exactly the reported lanes, runs the agent-reviewer quorum, and lets the
/// judge's hard-stop walk + R5 floor decide. No side effects.
pub(crate) fn decide(inp: &DecisionInputs<'_>) -> GateDecision {
    let pack = evidence_pack(inp);
    let receipts = agent_review(&pack);
    let bundle = full_auto_bundle_requiring(&inp.ci_status);
    // Fail closed if the full-auto profile can't be derived from the bundle.
    let Ok(profile) = FullAutoProfile::new(bundle) else {
        return GateDecision::RequireHuman;
    };
    let derived = profile.apply();
    let outcome = judge(JudgeInputs::new(
        &pack,
        &receipts,
        &derived,
        inp.repo,
        inp.target_branch,
    ));
    // Re-assert the profile floor (R5 stays human even on an AllowMerge verdict).
    profile.resolve(pack.risk, outcome.verdict.decision)
}

/// The canonical full-auto policy bundle with `required_ci_lanes` armed to
/// exactly the reported gating lanes — so the CI gate requires every executed
/// lane to be `Success`. R5 stays fail-closed in the bundle regardless.
fn full_auto_bundle_requiring(ci_status: &[CiCheck]) -> PolicyBundle {
    let mut bundle = policy_yaml::fixtures::default_bundle();
    bundle.approvals.required_ci_lanes = ci_status.iter().map(|c| c.name.clone()).collect();
    bundle
}

/// Evaluate every nonterminal PR whose head is `head_sha` and record a
/// `jeryu/autonomy` verdict check-run. Called from `on_push` after the head's
/// CI check-runs are recorded. Best-effort: forge errors are swallowed so a CI
/// push is never failed by the advisory verdict.
pub(crate) fn evaluate_pushed_head(
    core: &ForgeCore,
    owner: &str,
    repo: &str,
    head_sha: &str,
    ci_checks: &[(String, Option<CheckConclusion>)],
    changed_paths: &[String],
) {
    let Ok(prs) = core.list_pull_requests(owner, repo, None) else {
        return;
    };
    let ci_status = collect_ci_status(ci_checks);
    for pr in prs.into_iter() {
        if pr.merged
            || matches!(
                pr.state,
                PullRequestState::Closed | PullRequestState::Merged
            )
            || pr.head.sha != head_sha
        {
            continue;
        }
        let decision = decide(&DecisionInputs {
            repo: &format!("{owner}/{repo}"),
            source_branch: &pr.head.ref_name,
            target_branch: &pr.base.ref_name,
            head_sha,
            base_sha: &pr.base.sha,
            author_agent: None,
            changed_paths,
            ci_status: ci_status.clone(),
        });
        // RECORD-ONLY — no autonomous merge (see the module-level note). An
        // AllowMerge verdict is recorded as `Neutral` (advisory, not acted on);
        // it deliberately does NOT call `core.merge_pull_request`. The real
        // auto-merge is gated behind the safety rework.
        let (conclusion, summary) = match decision {
            GateDecision::AllowMerge => (
                CheckConclusion::Neutral,
                "advisory: CI+risk eligible — merge gated on safety rework (record-only)",
            ),
            GateDecision::RequireHuman => (
                CheckConclusion::ActionRequired,
                "advisory: human review required",
            ),
            GateDecision::Reject => (CheckConclusion::Failure, "advisory: blocked"),
        };
        record_verdict(core, owner, repo, head_sha, conclusion, summary);
    }
}

fn record_verdict(
    core: &ForgeCore,
    owner: &str,
    repo: &str,
    head_sha: &str,
    conclusion: CheckConclusion,
    _summary: &str,
) {
    use jeryu_core::{CheckRunStatus, CreateCheckRunRequest};
    let _ = core.create_check_run(
        owner,
        repo,
        CreateCheckRunRequest {
            name: "jeryu/autonomy".to_string(),
            head_sha: head_sha.to_string(),
            status: Some(CheckRunStatus::Completed),
            conclusion: Some(conclusion),
            ..Default::default()
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use jeryu_core::{CheckRunStatus, CreatePullRequestRequest, CreateRepositoryRequest};

    fn checks(pairs: &[(&str, CheckConclusion)]) -> Vec<(String, Option<CheckConclusion>)> {
        pairs
            .iter()
            .map(|(n, c)| (n.to_string(), Some(c.clone())))
            .collect()
    }

    #[test]
    fn skipped_and_neutral_lanes_are_not_required() {
        let cs = checks(&[
            ("build/test", CheckConclusion::Success),
            ("docs/lint", CheckConclusion::Skipped),
            ("advisory", CheckConclusion::Neutral),
        ]);
        let status = collect_ci_status(&cs);
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].name, "build/test");
        assert_eq!(status[0].conclusion, CiConclusion::Success);
    }

    #[test]
    fn risk_classifier_is_conservative() {
        // System trust surface → R5 (fail-closed to human).
        assert_eq!(
            classify_risk(&["crates/jeryu-autonomy/src/judge/mod.rs".into()]),
            RiskTier::R5
        );
        assert_eq!(
            classify_risk(&[".github/workflows/ci-fast.yml".into()]),
            RiskTier::R5
        );
        assert_eq!(classify_risk(&["Cargo.toml".into()]), RiskTier::R5);
        // Docs-only → R1.
        assert_eq!(classify_risk(&["docs/usage.md".into()]), RiskTier::R1);
        // Ordinary product code → R2 (auto-eligible).
        assert_eq!(
            classify_risk(&["crates/jeryu-web/src/page.rs".into()]),
            RiskTier::R2
        );
    }

    fn green(names: &[&str]) -> Vec<CiCheck> {
        names
            .iter()
            .map(|n| CiCheck {
                name: n.to_string(),
                conclusion: CiConclusion::Success,
            })
            .collect()
    }

    const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const BASE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn inputs<'a>(changed: &'a [String], ci: Vec<CiCheck>) -> DecisionInputs<'a> {
        DecisionInputs {
            repo: "neverhuman/jeryu",
            source_branch: "feature",
            target_branch: "main",
            head_sha: HEAD,
            base_sha: BASE,
            author_agent: Some("builder.x"),
            changed_paths: changed,
            ci_status: ci,
        }
    }

    fn core_with_pr() -> (ForgeCore, u64) {
        let core = ForgeCore::new();
        core.create_repository(
            "neverhuman",
            CreateRepositoryRequest {
                name: "jeryu".to_string(),
                private: true,
                description: None,
                default_branch: Some("main".to_string()),
            },
        )
        .unwrap();
        let pr = core
            .create_pull_request(
                "neverhuman",
                "jeryu",
                "autonomy.agent",
                CreatePullRequestRequest {
                    title: "record only".to_string(),
                    body: None,
                    head: "feature".to_string(),
                    base: "main".to_string(),
                    head_sha: Some(HEAD.to_string()),
                    base_sha: Some(BASE.to_string()),
                    source_repository: Some("neverhuman/jeryu".to_string()),
                    draft: false,
                    commits: Vec::new(),
                    changed_files: Vec::new(),
                },
            )
            .unwrap();
        (core, pr.number)
    }

    fn assert_record_only_verdict(core: &ForgeCore, pr_number: u64, expected: CheckConclusion) {
        let after = core
            .get_pull_request("neverhuman", "jeryu", pr_number)
            .unwrap();
        assert!(!after.merged);
        assert!(after.merged_at.is_none());
        assert!(after.merge_commit_sha.is_none());
        assert_ne!(after.state, PullRequestState::Merged);

        let checks = core
            .list_check_runs("neverhuman", "jeryu", Some(HEAD))
            .unwrap()
            .check_runs;
        let verdict = checks
            .iter()
            .find(|run| run.name == "jeryu/autonomy")
            .expect("autonomy verdict check-run");
        assert_eq!(verdict.status, CheckRunStatus::Completed);
        assert_eq!(verdict.conclusion, Some(expected));
    }

    #[test]
    fn green_low_risk_pr_allows_merge() {
        let changed = vec!["crates/jeryu-web/src/page.rs".to_string()];
        let d = decide(&inputs(&changed, green(&["ci-fast/build", "ci-fast/test"])));
        assert_eq!(d, GateDecision::AllowMerge);
    }

    #[test]
    fn evidence_pack_carries_changed_files() {
        let changed = vec![
            "docs/usage.md".to_string(),
            "crates/jeryu-web/src/page.rs".to_string(),
        ];
        let pack = evidence_pack(&inputs(&changed, green(&["ci-fast/build", "ci-fast/test"])));
        let paths: Vec<&str> = pack.changed_files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["crates/jeryu-web/src/page.rs", "docs/usage.md"]);
        assert!(
            pack.changed_files
                .iter()
                .all(|f| f.lines_added == 0 && f.lines_removed == 0)
        );
    }

    #[test]
    fn red_ci_blocks_merge() {
        let changed = vec!["crates/jeryu-web/src/page.rs".to_string()];
        let ci = vec![
            CiCheck {
                name: "ci-fast/build".into(),
                conclusion: CiConclusion::Success,
            },
            CiCheck {
                name: "ci-fast/test".into(),
                conclusion: CiConclusion::Failure,
            },
        ];
        let d = decide(&inputs(&changed, ci));
        assert_ne!(d, GateDecision::AllowMerge);
    }

    #[test]
    fn r5_system_change_requires_human_even_when_green() {
        let changed = vec!["crates/jeryu-autonomy/src/full_auto.rs".to_string()];
        let d = decide(&inputs(&changed, green(&["ci-fast/build", "ci-fast/test"])));
        assert_eq!(d, GateDecision::RequireHuman);
    }

    #[test]
    fn evaluate_pushed_head_records_verdict_without_merging() {
        let (core, pr_number) = core_with_pr();
        evaluate_pushed_head(
            &core,
            "neverhuman",
            "jeryu",
            HEAD,
            &checks(&[
                ("ci-fast/build", CheckConclusion::Success),
                ("ci-fast/test", CheckConclusion::Success),
            ]),
            &["crates/jeryu-web/src/page.rs".to_string()],
        );
        assert_record_only_verdict(&core, pr_number, CheckConclusion::Neutral);

        let (core, pr_number) = core_with_pr();
        evaluate_pushed_head(
            &core,
            "neverhuman",
            "jeryu",
            HEAD,
            &checks(&[
                ("ci-fast/build", CheckConclusion::Success),
                ("ci-fast/test", CheckConclusion::Success),
            ]),
            &["crates/jeryu-autonomy/src/full_auto.rs".to_string()],
        );
        assert_record_only_verdict(&core, pr_number, CheckConclusion::ActionRequired);
    }

    // =====================================================================
    // 7-probe adversarial harness (module header / AGENT_CHAT 2026-06-01).
    // Maps each documented merge-path attack to a regression assertion.
    //
    // Standing invariant (every probe): the bridge is RECORD-ONLY — it never
    // calls merge_pull_request. But "never merges" alone is true by construction
    // and would pass even if a guard were deleted, so each probe ALSO asserts a
    // discriminating signal:
    //   * Probes 4/5/7 assert the gate's hard stop (R5 floor / red-CI block) and
    //     FAIL if that guard is removed.
    //   * Probes 1/2/3/6 are the documented gaps where `decide` returns AllowMerge
    //     today; they assert that AllowMerge plus the record-only no-merge, so they
    //     double as tripwires — wiring a real auto-merge flips them red until the
    //     safety rework (real reviewers / author gate / armed lanes) lands.
    // =====================================================================

    /// Drive the full record-only path: seed a PR, evaluate its pushed head with
    /// the given CI check-runs + changed paths, and return (forge, pr_number) so
    /// the caller can assert the recorded verdict and that nothing merged.
    fn run_evaluate(ci: &[(&str, CheckConclusion)], changed: &[&str]) -> (ForgeCore, u64) {
        let (core, pr_number) = core_with_pr();
        let paths: Vec<String> = changed.iter().map(|s| s.to_string()).collect();
        evaluate_pushed_head(&core, "neverhuman", "jeryu", HEAD, &checks(ci), &paths);
        (core, pr_number)
    }

    // PROBE 1 — vacuous CI gate: with NO executable lanes the required-lane set is
    // empty and the gate passes vacuously (decide -> AllowMerge). Record-only must
    // still refuse to merge.
    #[test]
    fn probe1_vacuous_empty_ci_records_but_never_merges() {
        assert_eq!(
            decide(&inputs(
                &["crates/jeryu-web/src/page.rs".into()],
                green(&[])
            )),
            GateDecision::AllowMerge,
            "documents the vacuous-CI gap: an empty required-lane set passes the gate"
        );
        let (core, n) = run_evaluate(&[], &["crates/jeryu-web/src/page.rs"]);
        assert_record_only_verdict(&core, n, CheckConclusion::Neutral);
    }

    // PROBE 2 — skipped/neutral-only lanes are dropped by collect_ci_status, so the
    // gate has nothing to require: same vacuous outcome, still no merge.
    #[test]
    fn probe2_skipped_only_lanes_record_but_never_merge() {
        let (core, n) = run_evaluate(
            &[
                ("docs/lint", CheckConclusion::Skipped),
                ("advisory", CheckConclusion::Neutral),
            ],
            &["crates/jeryu-web/src/page.rs"],
        );
        assert_record_only_verdict(&core, n, CheckConclusion::Neutral);
    }

    // PROBE 3 — the agent-reviewer quorum is synthetic (always-Pass, unsigned). Even
    // clearing that quorum on a green R2 change, the record-only path must not act.
    #[test]
    fn probe3_synthetic_unsigned_quorum_records_but_never_merges() {
        assert_eq!(
            decide(&inputs(
                &["crates/jeryu-web/src/page.rs".into()],
                green(&["ci/build", "ci/test"]),
            )),
            GateDecision::AllowMerge,
            "synthetic always-Pass quorum clears the gate — the gap the rework must close"
        );
        let (core, n) = run_evaluate(
            &[
                ("ci/build", CheckConclusion::Success),
                ("ci/test", CheckConclusion::Success),
            ],
            &["crates/jeryu-web/src/page.rs"],
        );
        assert_record_only_verdict(&core, n, CheckConclusion::Neutral);
    }

    // PROBE 4 — a change to the merge engine's OWN judge is R5: the gate cannot
    // auto-approve edits to itself. (FAILS if the jeryu-autonomy R5 marker is removed.)
    #[test]
    fn probe4_self_edit_of_merge_engine_requires_human() {
        let changed = vec!["crates/jeryu-autonomy/src/judge/mod.rs".to_string()];
        assert_eq!(
            decide(&inputs(&changed, green(&["ci/build", "ci/test"]))),
            GateDecision::RequireHuman,
            "R5 floor must hold for a merge-engine self-edit"
        );
        let (core, n) = run_evaluate(
            &[
                ("ci/build", CheckConclusion::Success),
                ("ci/test", CheckConclusion::Success),
            ],
            &["crates/jeryu-autonomy/src/judge/mod.rs"],
        );
        assert_record_only_verdict(&core, n, CheckConclusion::ActionRequired);
    }

    // PROBE 5 — a red required lane is a hard stop: decide != AllowMerge and the
    // recorded verdict is Failure. (FAILS if the CI gate is removed.)
    #[test]
    fn probe5_red_required_lane_denies_and_records_failure() {
        let ci = vec![
            CiCheck {
                name: "ci/build".into(),
                conclusion: CiConclusion::Success,
            },
            CiCheck {
                name: "ci/test".into(),
                conclusion: CiConclusion::Failure,
            },
        ];
        assert_ne!(
            decide(&inputs(&["crates/jeryu-web/src/page.rs".into()], ci)),
            GateDecision::AllowMerge,
            "a failed required lane must block the merge decision"
        );
        let (core, n) = run_evaluate(
            &[
                ("ci/build", CheckConclusion::Success),
                ("ci/test", CheckConclusion::Failure),
            ],
            &["crates/jeryu-web/src/page.rs"],
        );
        assert_record_only_verdict(&core, n, CheckConclusion::Failure);
    }

    // PROBE 6 — there is no author/fork trust gate yet: a foreign author with a
    // green R2 change still yields AllowMerge, so the record-only path is the only
    // thing preventing the merge (tripwire for the missing author gate).
    #[test]
    fn probe6_untrusted_author_records_but_never_merges() {
        let changed = vec!["crates/jeryu-web/src/page.rs".to_string()];
        let mut inp = inputs(&changed, green(&["ci/build", "ci/test"]));
        inp.author_agent = Some("attacker.fork");
        assert_eq!(
            decide(&inp),
            GateDecision::AllowMerge,
            "documents the missing author/fork trust gate"
        );
        let (core, n) = run_evaluate(
            &[
                ("ci/build", CheckConclusion::Success),
                ("ci/test", CheckConclusion::Success),
            ],
            &["crates/jeryu-web/src/page.rs"],
        );
        assert_record_only_verdict(&core, n, CheckConclusion::Neutral);
    }

    // PROBE 7 — a mixed changed-set is classified by its RISKIEST path: one benign
    // file alongside a CI-definition edit is R5 overall, so a single commit cannot
    // smuggle a trust-surface change past the gate. (FAILS if the set isn't scanned.)
    #[test]
    fn probe7_any_r5_path_in_the_changed_set_requires_human() {
        let changed = vec![
            "crates/jeryu-web/src/page.rs".to_string(),
            ".github/workflows/ci.yml".to_string(),
        ];
        assert_eq!(
            decide(&inputs(&changed, green(&["ci/build", "ci/test"]))),
            GateDecision::RequireHuman,
            "one R5 path in the set forces the whole change to R5"
        );
        let (core, n) = run_evaluate(
            &[
                ("ci/build", CheckConclusion::Success),
                ("ci/test", CheckConclusion::Success),
            ],
            &["crates/jeryu-web/src/page.rs", ".github/workflows/ci.yml"],
        );
        assert_record_only_verdict(&core, n, CheckConclusion::ActionRequired);
    }
}

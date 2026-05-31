//! Pure, deterministic PR-overlap detection and routing.
//!
//! The owner's flagship behaviour: when a new change overlaps an existing OPEN
//! pull request, the change should hot-fix that PR instead of opening a fresh
//! one and starting another run. This module computes that decision over plain
//! inputs with no [`crate::ForgeCore`] coupling, so it is trivially testable and
//! free of IO.
//!
//! The engine is intentionally conservative: it never silently hijacks an
//! existing PR. Below the overlap threshold it always prefers a new PR, and even
//! above the threshold it refuses to coalesce onto a PR whose base SHA differs
//! (diverged-base clobber) or whose head proof is not green (missing proof).

use serde::{Deserialize, Serialize};

/// A proposed change under consideration for routing.
///
/// This is the new PR / push that has not yet been turned into a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSet {
    /// Repository-relative paths the change touches.
    pub changed_files: Vec<String>,
    /// Base commit the change is built on.
    pub base_sha: String,
    /// Optional author identity (informational; not used for the decision).
    #[serde(default)]
    pub author: Option<String>,
}

impl ChangeSet {
    /// Convenience constructor for a change set.
    pub fn new(
        changed_files: impl IntoIterator<Item = impl Into<String>>,
        base_sha: impl Into<String>,
        author: Option<String>,
    ) -> Self {
        Self {
            changed_files: changed_files.into_iter().map(Into::into).collect(),
            base_sha: base_sha.into(),
            author,
        }
    }
}

/// An existing OPEN pull request the change might be routed onto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenPr {
    /// PR number (the stable, user-visible identifier).
    pub number: u64,
    /// Files the PR currently touches.
    pub changed_files: Vec<String>,
    /// Base commit the PR is built on.
    pub base_sha: String,
    /// Whether the PR head currently has a green proof. Coalescing onto a PR
    /// without a green head proof is refused so we never stack work on an
    /// unverified base.
    pub head_proof_ok: bool,
}

impl OpenPr {
    /// Convenience constructor for an open PR.
    pub fn new(
        number: u64,
        changed_files: impl IntoIterator<Item = impl Into<String>>,
        base_sha: impl Into<String>,
        head_proof_ok: bool,
    ) -> Self {
        Self {
            number,
            changed_files: changed_files.into_iter().map(Into::into).collect(),
            base_sha: base_sha.into(),
            head_proof_ok,
        }
    }
}

/// Configuration for the overlap routing engine.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OverlapConfig {
    /// Minimum Jaccard overlap (inclusive) for a candidate PR to be considered
    /// for routing. Values are clamped to `[0.0, 1.0]` at decision time.
    pub threshold: f64,
}

impl Default for OverlapConfig {
    fn default() -> Self {
        // A conservative default: at least half the union of touched files must
        // overlap before we even consider hot-fixing an existing PR.
        Self { threshold: 0.5 }
    }
}

impl OverlapConfig {
    /// Builds a config with an explicit threshold.
    pub fn with_threshold(threshold: f64) -> Self {
        Self { threshold }
    }

    /// The effective threshold, clamped into `[0.0, 1.0]`.
    fn effective_threshold(&self) -> f64 {
        self.threshold.clamp(0.0, 1.0)
    }
}

/// The routing decision for a proposed change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlapDecision {
    /// Route (hot-fix) the change onto an existing open PR.
    RouteToExisting {
        /// PR number to route onto.
        pr: u64,
        /// Human-readable justification.
        reason: String,
    },
    /// Open a fresh PR / start a new run.
    CreateNew {
        /// Human-readable justification.
        reason: String,
    },
    /// The best candidate overlaps enough to route, but coalescing is unsafe
    /// (diverged base or missing head proof), so we refuse rather than clobber.
    RefuseCoalesce {
        /// PR number that would have been routed onto.
        pr: u64,
        /// Human-readable justification.
        reason: String,
    },
}

/// Per-candidate overlap score, recorded for auditability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlapScore {
    /// Candidate PR number.
    pub pr: u64,
    /// Jaccard overlap of the candidate against the change.
    pub jaccard: f64,
    /// Whether the candidate's base SHA matches the change's base SHA.
    pub base_matches: bool,
    /// Whether the candidate's head proof is green.
    pub head_proof_ok: bool,
}

/// An auditable receipt describing how a routing decision was reached.
///
/// Captures the decision plus every candidate's score so an operator can see
/// exactly why a change was (or was not) coalesced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlapRouting {
    /// The effective (clamped) threshold used for this decision.
    pub threshold: f64,
    /// Base SHA of the change being routed.
    pub change_base_sha: String,
    /// Per-candidate scores, in input order.
    pub scores: Vec<OverlapScore>,
    /// The resulting decision.
    pub decision: OverlapDecision,
}

/// Jaccard similarity of two file sets: `|A ∩ B| / |A ∪ B|`.
///
/// Two empty sets are defined as `0.0` (no evidence of overlap), which keeps the
/// engine conservative: an empty change never coalesces onto anything.
/// Duplicate entries within a slice are de-duplicated, so the result depends
/// only on the set of paths, not their multiplicity or order.
pub fn jaccard(a: &[String], b: &[String]) -> f64 {
    use std::collections::BTreeSet;

    let set_a: BTreeSet<&str> = a.iter().map(String::as_str).collect();
    let set_b: BTreeSet<&str> = b.iter().map(String::as_str).collect();

    if set_a.is_empty() && set_b.is_empty() {
        return 0.0;
    }

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();

    // `union` is non-zero here because at least one set is non-empty.
    intersection as f64 / union as f64
}

/// Decide how to route `change` given the currently OPEN PRs.
///
/// See the module docs for the rules. This is a pure function: the same inputs
/// always produce the same [`OverlapDecision`], with deterministic tie-breaking
/// (highest overlap wins; ties resolve to the lowest PR number).
pub fn decide(change: &ChangeSet, open: &[OpenPr], cfg: OverlapConfig) -> OverlapDecision {
    route(change, open, cfg).decision
}

/// Like [`decide`], but returns the full [`OverlapRouting`] receipt with every
/// candidate score for auditing.
pub fn route(change: &ChangeSet, open: &[OpenPr], cfg: OverlapConfig) -> OverlapRouting {
    let threshold = cfg.effective_threshold();

    let scores: Vec<OverlapScore> = open
        .iter()
        .map(|pr| OverlapScore {
            pr: pr.number,
            jaccard: jaccard(&change.changed_files, &pr.changed_files),
            base_matches: pr.base_sha == change.base_sha,
            head_proof_ok: pr.head_proof_ok,
        })
        .collect();

    let decision = decide_from_scores(change, open, threshold, &scores);

    OverlapRouting {
        threshold,
        change_base_sha: change.base_sha.clone(),
        scores,
        decision,
    }
}

/// Core decision logic over precomputed scores.
fn decide_from_scores(
    change: &ChangeSet,
    open: &[OpenPr],
    threshold: f64,
    scores: &[OverlapScore],
) -> OverlapDecision {
    if open.is_empty() {
        return OverlapDecision::CreateNew {
            reason: "no open pull requests to coalesce onto".to_string(),
        };
    }

    // Pick the candidate at or above threshold with the HIGHEST overlap; break
    // ties toward the LOWEST PR number so the decision never thrashes between
    // equally-overlapping PRs.
    let best = scores
        .iter()
        .filter(|s| s.jaccard >= threshold)
        .max_by(|x, y| {
            x.jaccard
                .partial_cmp(&y.jaccard)
                // NaN cannot arise from `jaccard` (ratios of finite counts), but
                // treat any incomparable pair as equal and let the PR-number
                // tie-break decide, keeping the function total.
                .unwrap_or(std::cmp::Ordering::Equal)
                // Reverse PR-number ordering so that, among equal overlaps,
                // `max_by` selects the SMALLEST PR number.
                .then_with(|| y.pr.cmp(&x.pr))
        });

    let Some(best) = best else {
        return OverlapDecision::CreateNew {
            reason: format!(
                "no open PR overlaps the change at or above threshold {threshold:.2}; \
                 opening a fresh PR"
            ),
        };
    };

    // Safety gate: never coalesce onto a diverged base or an unproven head.
    if !best.base_matches {
        let pr_base = open
            .iter()
            .find(|p| p.number == best.pr)
            .map(|p| p.base_sha.as_str())
            .unwrap_or("");
        return OverlapDecision::RefuseCoalesce {
            pr: best.pr,
            reason: format!(
                "PR #{} overlaps at {:.2} but its base {} differs from the change base {}; \
                 refusing to coalesce onto a diverged base",
                best.pr, best.jaccard, pr_base, change.base_sha
            ),
        };
    }

    if !best.head_proof_ok {
        return OverlapDecision::RefuseCoalesce {
            pr: best.pr,
            reason: format!(
                "PR #{} overlaps at {:.2} but its head proof is not green; \
                 refusing to stack work on an unverified head",
                best.pr, best.jaccard
            ),
        };
    }

    OverlapDecision::RouteToExisting {
        pr: best.pr,
        reason: format!(
            "PR #{} is the highest-overlap open PR at {:.2} (>= {:.2}) with a matching base \
             and green head proof; hot-fixing it instead of opening a new PR",
            best.pr, best.jaccard, threshold
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|s| s.to_string()).collect()
    }

    // ---- jaccard --------------------------------------------------------

    #[test]
    fn jaccard_identical_sets_is_one() {
        let a = files(&["src/a.rs", "src/b.rs"]);
        let b = files(&["src/b.rs", "src/a.rs"]);
        assert_eq!(jaccard(&a, &b), 1.0);
    }

    #[test]
    fn jaccard_disjoint_sets_is_zero() {
        let a = files(&["src/a.rs"]);
        let b = files(&["src/z.rs"]);
        assert_eq!(jaccard(&a, &b), 0.0);
    }

    #[test]
    fn jaccard_partial_overlap() {
        // intersection {b} = 1, union {a,b,c} = 3
        let a = files(&["a", "b"]);
        let b = files(&["b", "c"]);
        assert!((jaccard(&a, &b) - (1.0 / 3.0)).abs() < 1e-12);
    }

    #[test]
    fn jaccard_dedups_within_a_slice() {
        let a = files(&["a", "a", "b"]);
        let b = files(&["a", "b", "b"]);
        assert_eq!(jaccard(&a, &b), 1.0);
    }

    #[test]
    fn jaccard_both_empty_is_zero() {
        assert_eq!(jaccard(&[], &[]), 0.0);
    }

    #[test]
    fn jaccard_one_empty_is_zero() {
        let a = files(&["a"]);
        assert_eq!(jaccard(&a, &[]), 0.0);
        assert_eq!(jaccard(&[], &a), 0.0);
    }

    // ---- decide ---------------------------------------------------------

    #[test]
    fn empty_open_set_creates_new() {
        let change = ChangeSet::new(["a"], "base", None);
        let decision = decide(&change, &[], OverlapConfig::default());
        assert!(matches!(decision, OverlapDecision::CreateNew { .. }));
    }

    #[test]
    fn disjoint_open_pr_creates_new() {
        let change = ChangeSet::new(["src/a.rs"], "base", None);
        let open = vec![OpenPr::new(1, ["src/z.rs"], "base", true)];
        let decision = decide(&change, &open, OverlapConfig::default());
        assert!(matches!(decision, OverlapDecision::CreateNew { .. }));
    }

    #[test]
    fn below_threshold_creates_new_conservatively() {
        // overlap 1/3 ≈ 0.33 < default 0.5
        let change = ChangeSet::new(["a", "b"], "base", None);
        let open = vec![OpenPr::new(7, ["b", "c"], "base", true)];
        let decision = decide(&change, &open, OverlapConfig::default());
        assert!(matches!(decision, OverlapDecision::CreateNew { .. }));
    }

    #[test]
    fn at_threshold_routes() {
        // overlap exactly 0.5, threshold 0.5 (inclusive)
        let change = ChangeSet::new(["a", "b"], "base", None);
        let open = vec![OpenPr::new(3, ["a", "b", "c", "d"], "base", true)];
        // intersection {a,b}=2, union {a,b,c,d}=4 -> 0.5
        let decision = decide(&change, &open, OverlapConfig::with_threshold(0.5));
        assert_eq!(
            decision,
            OverlapDecision::RouteToExisting {
                pr: 3,
                reason: match decision.clone() {
                    OverlapDecision::RouteToExisting { reason, .. } => reason,
                    _ => unreachable!(),
                },
            }
        );
    }

    #[test]
    fn identical_change_routes_to_existing() {
        let change = ChangeSet::new(["src/a.rs", "src/b.rs"], "base", None);
        let open = vec![OpenPr::new(5, ["src/a.rs", "src/b.rs"], "base", true)];
        let decision = decide(&change, &open, OverlapConfig::default());
        match decision {
            OverlapDecision::RouteToExisting { pr, .. } => assert_eq!(pr, 5),
            other => panic!("expected RouteToExisting, got {other:?}"),
        }
    }

    #[test]
    fn highest_overlap_wins() {
        let change = ChangeSet::new(["a", "b", "c", "d"], "base", None);
        let open = vec![
            // overlap 2/4 = 0.5
            OpenPr::new(10, ["a", "b"], "base", true),
            // overlap 4/4 = 1.0
            OpenPr::new(11, ["a", "b", "c", "d"], "base", true),
            // overlap 3/4 = 0.75
            OpenPr::new(12, ["a", "b", "c"], "base", true),
        ];
        let decision = decide(&change, &open, OverlapConfig::with_threshold(0.5));
        match decision {
            OverlapDecision::RouteToExisting { pr, .. } => assert_eq!(pr, 11),
            other => panic!("expected RouteToExisting #11, got {other:?}"),
        }
    }

    #[test]
    fn tie_breaks_to_lowest_pr_number() {
        let change = ChangeSet::new(["a", "b"], "base", None);
        let open = vec![
            // both identical overlap 1.0
            OpenPr::new(42, ["a", "b"], "base", true),
            OpenPr::new(17, ["a", "b"], "base", true),
            OpenPr::new(99, ["a", "b"], "base", true),
        ];
        let decision = decide(&change, &open, OverlapConfig::default());
        match decision {
            OverlapDecision::RouteToExisting { pr, .. } => assert_eq!(pr, 17),
            other => panic!("expected lowest PR 17, got {other:?}"),
        }
    }

    #[test]
    fn tie_break_independent_of_input_order() {
        let change = ChangeSet::new(["a", "b"], "base", None);
        let open_a = vec![
            OpenPr::new(8, ["a", "b"], "base", true),
            OpenPr::new(4, ["a", "b"], "base", true),
        ];
        let open_b = vec![
            OpenPr::new(4, ["a", "b"], "base", true),
            OpenPr::new(8, ["a", "b"], "base", true),
        ];
        assert_eq!(
            decide(&change, &open_a, OverlapConfig::default()),
            decide(&change, &open_b, OverlapConfig::default())
        );
    }

    #[test]
    fn diverged_base_refuses_coalesce() {
        let change = ChangeSet::new(["a", "b"], "new-base", None);
        let open = vec![OpenPr::new(21, ["a", "b"], "previous-base", true)];
        let decision = decide(&change, &open, OverlapConfig::default());
        match decision {
            OverlapDecision::RefuseCoalesce { pr, .. } => assert_eq!(pr, 21),
            other => panic!("expected RefuseCoalesce #21, got {other:?}"),
        }
    }

    #[test]
    fn missing_proof_refuses_coalesce() {
        let change = ChangeSet::new(["a", "b"], "base", None);
        let open = vec![OpenPr::new(33, ["a", "b"], "base", false)];
        let decision = decide(&change, &open, OverlapConfig::default());
        match decision {
            OverlapDecision::RefuseCoalesce { pr, .. } => assert_eq!(pr, 33),
            other => panic!("expected RefuseCoalesce #33, got {other:?}"),
        }
    }

    #[test]
    fn stale_base_is_evaluated_only_for_best_candidate() {
        // The highest-overlap PR is safe; a lower-overlap PR has a diverged base.
        // We must route to the safe best, not refuse on the worse candidate.
        let change = ChangeSet::new(["a", "b", "c", "d"], "base", None);
        let open = vec![
            // overlap 1.0, safe
            OpenPr::new(1, ["a", "b", "c", "d"], "base", true),
            // overlap 0.5, diverged base — should be ignored
            OpenPr::new(2, ["a", "b"], "stale", true),
        ];
        let decision = decide(&change, &open, OverlapConfig::with_threshold(0.5));
        match decision {
            OverlapDecision::RouteToExisting { pr, .. } => assert_eq!(pr, 1),
            other => panic!("expected RouteToExisting #1, got {other:?}"),
        }
    }

    #[test]
    fn refuse_when_only_overlapping_candidate_is_unsafe() {
        // The best overlapping candidate is unsafe and there is no safe
        // above-threshold alternative -> refuse, do not silently create new.
        let change = ChangeSet::new(["a", "b"], "base", None);
        let open = vec![
            OpenPr::new(5, ["a", "b"], "previous", true), // overlap 1.0 but diverged
            OpenPr::new(6, ["z"], "base", true),          // disjoint
        ];
        let decision = decide(&change, &open, OverlapConfig::default());
        match decision {
            OverlapDecision::RefuseCoalesce { pr, .. } => assert_eq!(pr, 5),
            other => panic!("expected RefuseCoalesce #5, got {other:?}"),
        }
    }

    // ---- threshold clamping --------------------------------------------

    #[test]
    fn threshold_above_one_never_routes() {
        // No real overlap can reach a threshold above 1.0 except identical sets,
        // which top out at exactly 1.0; clamping to 1.0 means only Jaccard 1.0
        // routes.
        let change = ChangeSet::new(["a", "b"], "base", None);
        let open = vec![OpenPr::new(1, ["a", "b", "c"], "base", true)]; // 2/3
        let decision = decide(&change, &open, OverlapConfig::with_threshold(5.0));
        assert!(matches!(decision, OverlapDecision::CreateNew { .. }));
    }

    #[test]
    fn threshold_clamped_to_one_routes_on_identical() {
        let change = ChangeSet::new(["a", "b"], "base", None);
        let open = vec![OpenPr::new(1, ["a", "b"], "base", true)]; // 1.0
        let decision = decide(&change, &open, OverlapConfig::with_threshold(5.0));
        match decision {
            OverlapDecision::RouteToExisting { pr, .. } => assert_eq!(pr, 1),
            other => panic!("expected RouteToExisting #1, got {other:?}"),
        }
    }

    #[test]
    fn negative_threshold_clamped_to_zero_still_needs_some_overlap() {
        // Clamped threshold is 0.0; a disjoint PR has overlap 0.0 >= 0.0, so it
        // would be selected — but Jaccard 0.0 means no shared files. This is the
        // documented behaviour: threshold 0.0 routes any non-empty candidate.
        let change = ChangeSet::new(["a"], "base", None);
        let open = vec![OpenPr::new(1, ["z"], "base", true)];
        let decision = decide(&change, &open, OverlapConfig::with_threshold(-1.0));
        match decision {
            OverlapDecision::RouteToExisting { pr, .. } => assert_eq!(pr, 1),
            other => panic!("expected RouteToExisting #1 at threshold 0, got {other:?}"),
        }
    }

    // ---- receipt --------------------------------------------------------

    #[test]
    fn routing_receipt_records_all_scores_and_threshold() {
        let change = ChangeSet::new(["a", "b", "c", "d"], "base", Some("alice".into()));
        let open = vec![
            OpenPr::new(10, ["a", "b"], "base", true),
            OpenPr::new(11, ["a", "b", "c", "d"], "base", true),
        ];
        let receipt = route(&change, &open, OverlapConfig::with_threshold(0.5));

        assert_eq!(receipt.threshold, 0.5);
        assert_eq!(receipt.change_base_sha, "base");
        assert_eq!(receipt.scores.len(), 2);
        assert_eq!(receipt.scores[0].pr, 10);
        assert_eq!(receipt.scores[0].jaccard, 0.5);
        assert_eq!(receipt.scores[1].pr, 11);
        assert_eq!(receipt.scores[1].jaccard, 1.0);
        assert!(receipt.scores[1].base_matches);
        assert!(receipt.scores[1].head_proof_ok);
        match receipt.decision {
            OverlapDecision::RouteToExisting { pr, .. } => assert_eq!(pr, 11),
            other => panic!("expected RouteToExisting #11, got {other:?}"),
        }
    }

    #[test]
    fn routing_receipt_records_refusal_scores() {
        let change = ChangeSet::new(["a", "b"], "new", None);
        let open = vec![OpenPr::new(2, ["a", "b"], "previous", true)];
        let receipt = route(&change, &open, OverlapConfig::default());
        assert_eq!(receipt.scores.len(), 1);
        assert!(!receipt.scores[0].base_matches);
        assert!(matches!(
            receipt.decision,
            OverlapDecision::RefuseCoalesce { pr: 2, .. }
        ));
    }

    #[test]
    fn receipt_round_trips_through_serde_json() {
        let change = ChangeSet::new(["a", "b"], "base", None);
        let open = vec![OpenPr::new(1, ["a", "b"], "base", true)];
        let receipt = route(&change, &open, OverlapConfig::default());

        let json = serde_json::to_string(&receipt).expect("serialize");
        let parsed: OverlapRouting = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(receipt, parsed);
    }

    #[test]
    fn default_threshold_is_one_half() {
        assert_eq!(OverlapConfig::default().threshold, 0.5);
    }

    #[test]
    fn decide_matches_route_decision() {
        let change = ChangeSet::new(["a", "b"], "base", None);
        let open = vec![OpenPr::new(1, ["a", "b"], "base", true)];
        let cfg = OverlapConfig::default();
        assert_eq!(
            decide(&change, &open, cfg),
            route(&change, &open, cfg).decision
        );
    }
}

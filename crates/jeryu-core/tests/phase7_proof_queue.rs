//! Tests for the Phase 7 proof / merge-queue / agent-scope model and the
//! append-only receipt + typed id machinery.

use jeryu_core::phase7::{AgentScope, ChangedPath, PullRequest, QueueEntryState};
use jeryu_core::{AgentId, JeryuError, PullRequestId, Receipt, ReceiptId, ReceiptKind, RepoId};

// ---------------------------------------------------------------------------
// Typed ids
// ---------------------------------------------------------------------------

#[test]
fn typed_ids_carry_prefix_and_are_unique() {
    let a = ReceiptId::fresh();
    let b = ReceiptId::fresh();
    assert!(a.as_str().starts_with("receipt_"));
    assert_ne!(a, b, "fresh ids must be process-unique");

    assert!(RepoId::fresh().as_str().starts_with("repo_"));
    assert!(PullRequestId::fresh().as_str().starts_with("pr_"));
    assert!(AgentId::fresh().as_str().starts_with("agent_"));
}

#[test]
fn typed_id_from_trusted_string_preserves_value() {
    let id = RepoId::new("alice/jeryu");
    assert_eq!(id.as_str(), "alice/jeryu");
    assert_eq!(id.to_string(), "alice/jeryu");
    // From<&str> / From<String> conversions.
    let from_ref: RepoId = "x".into();
    assert_eq!(from_ref.as_str(), "x");
    let from_owned: RepoId = String::from("y").into();
    assert_eq!(from_owned.as_str(), "y");
}

// ---------------------------------------------------------------------------
// Changed paths + PR model
// ---------------------------------------------------------------------------

#[test]
fn changed_path_defaults_non_sensitive_and_can_be_marked() {
    let plain = ChangedPath::new("src/lib.rs");
    assert!(!plain.sensitive);
    let secret = ChangedPath::new("infra/keys.pem").sensitive();
    assert!(secret.sensitive);
}

#[test]
fn pull_request_changed_path_set_dedupes_and_sorts() {
    let pr = PullRequest::new(
        RepoId::new("alice/jeryu"),
        PullRequestId::new("pr_1"),
        "feature",
        "main",
        "base-sha",
        "head-sha",
        vec![
            ChangedPath::new("b.rs"),
            ChangedPath::new("a.rs"),
            ChangedPath::new("a.rs"),
        ],
    );
    let set = pr.changed_path_set();
    // BTreeSet => sorted + deduped.
    let collected: Vec<_> = set.into_iter().collect();
    assert_eq!(collected, vec!["a.rs".to_string(), "b.rs".to_string()]);
    assert_eq!(pr.source_branch, "feature");
    assert_eq!(pr.target_branch, "main");
}

// ---------------------------------------------------------------------------
// Agent scope policy
// ---------------------------------------------------------------------------

#[test]
fn agent_scope_permits_paths_within_prefixes_and_cap() {
    let scope = AgentScope {
        agent: AgentId::new("agent_bot"),
        repo: RepoId::new("alice/jeryu"),
        allowed_paths: vec!["src/".to_string(), "docs/".to_string()],
        max_paths: 3,
    };
    assert!(scope.permits_all(&["src/lib.rs".to_string(), "docs/readme.md".to_string()]));
    // Exact prefix match also permitted.
    assert!(scope.permits_all(&["src/".to_string()]));
}

#[test]
fn agent_scope_rejects_out_of_scope_paths() {
    let scope = AgentScope {
        agent: AgentId::new("agent_bot"),
        repo: RepoId::new("alice/jeryu"),
        allowed_paths: vec!["src/".to_string()],
        max_paths: 5,
    };
    // A path outside any allowed prefix is denied.
    assert!(!scope.permits_all(&["infra/secrets.tf".to_string()]));
    // Mixed in-scope + out-of-scope => denied.
    assert!(!scope.permits_all(&["src/ok.rs".to_string(), "etc/passwd".to_string()]));
}

#[test]
fn agent_scope_rejects_empty_and_over_cap() {
    let scope = AgentScope {
        agent: AgentId::new("agent_bot"),
        repo: RepoId::new("alice/jeryu"),
        allowed_paths: vec!["src/".to_string()],
        max_paths: 2,
    };
    // Empty mutation set is rejected.
    assert!(!scope.permits_all(&[]));
    // Over the per-request cap is rejected even though all are in scope.
    assert!(!scope.permits_all(&[
        "src/a.rs".to_string(),
        "src/b.rs".to_string(),
        "src/c.rs".to_string(),
    ]));
}

// ---------------------------------------------------------------------------
// Receipts
// ---------------------------------------------------------------------------

#[test]
fn receipt_captures_kind_subject_and_commands() {
    let receipt = Receipt::new(
        ReceiptKind::ProofWitness,
        RepoId::new("alice/jeryu"),
        Some(AgentId::new("agent_bot")),
        "pr_42",
        "head-sha",
        "proof lanes passed",
        vec!["jeryu proof run".to_string()],
        "low",
    );
    assert_eq!(receipt.kind, ReceiptKind::ProofWitness);
    assert_eq!(receipt.subject, "pr_42");
    assert_eq!(receipt.sha, "head-sha");
    assert_eq!(receipt.residual_risk, "low");
    assert_eq!(receipt.commands, vec!["jeryu proof run".to_string()]);
    assert!(receipt.agent.is_some());
    assert!(receipt.id.as_str().starts_with("receipt_"));
}

#[test]
fn receipt_kinds_are_distinct() {
    assert_ne!(ReceiptKind::ProofPlan, ReceiptKind::ProofWitness);
    assert_ne!(ReceiptKind::AgentHotfix, ReceiptKind::AgentProposedFix);
    assert_ne!(ReceiptKind::MergeQueue, ReceiptKind::Repair);
}

// ---------------------------------------------------------------------------
// Queue entry states + policy error display
// ---------------------------------------------------------------------------

#[test]
fn queue_entry_states_are_distinct() {
    assert_ne!(QueueEntryState::Queued, QueueEntryState::Mergeable);
    assert_ne!(
        QueueEntryState::DequeuedConflict,
        QueueEntryState::DequeuedFailedValidation
    );
    assert_ne!(
        QueueEntryState::SpeculativeMergeTesting,
        QueueEntryState::Queued
    );
}

#[test]
fn jeryu_error_display_messages() {
    assert_eq!(
        JeryuError::NotFound("pr".to_string()).to_string(),
        "not found: pr"
    );
    assert_eq!(
        JeryuError::PolicyDenied("scope".to_string()).to_string(),
        "policy denied: scope"
    );
    assert_eq!(
        JeryuError::MissingProofWitness("pr_1".to_string()).to_string(),
        "missing proof witness: pr_1"
    );
    assert_eq!(
        JeryuError::Conflict("paths".to_string()).to_string(),
        "conflict: paths"
    );
}

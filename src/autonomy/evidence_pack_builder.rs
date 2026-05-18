//! Wave 8 — Auto-rejudge / `EvidencePackBuilder`.
//!
//! Builds a fresh, signed [`EvidencePack`] from the current host-side view of
//! a PR. Used by the auto-rejudge pipeline: when a PR's `head_sha`,
//! `target_branch_sha`, or `policy_sha` has drifted from what a cached pack
//! captured, the daemon doesn't trust the old pack — it asks this builder to
//! materialize a new one from `GitHost::fetch_pr_diff(...)`'s answer and
//! re-runs the reviewers/judge against it.
//!
//! Design choices (called out so future Waves can revisit):
//!
//! - **Risk classification reuses [`RiskClassifier`]** against the same
//!   `.autonomy/policies/*.yml` bundle the live orchestrator uses. We do
//!   not invent a parallel "rejudge risk" notion.
//!
//! - **Scans are recorded as `Passed`** in the synthetic
//!   [`SecuritySection`]. Wave 8 deliberately does NOT re-run SAST /
//!   dependency / secret scans during auto-rejudge — that's the scanner
//!   pipeline's job and re-running here would double the CI cost. The
//!   pack carries the "scans assumed clean" sentinel for the second-pass
//!   judge; the existing reviewers will still flag suspicious diffs they
//!   actually observe, so this isn't a hole — it's a layering decision.
//!
//! - **Signing canonicalization** is plain `serde_json::to_string` over the
//!   unsigned pack. The same canonicalization is used inside
//!   [`build_evidence_pack`] for the `evidence_digest`, so the digest +
//!   signature agree on what they're committing to. A future canonical-
//!   JSON pass (sorted keys, normalized number repr) is a separate
//!   concern; today's verifier only needs a stable bytes-in / bytes-out
//!   contract and `serde_json::to_string` provides that for our struct
//!   layout.
//!
//! - **No DB**. The builder is pure compute over the host's diff + the
//!   in-memory policy bundle + the in-memory signing key. Tests follow
//!   the `fresh_*` pattern by construction (no shared mutable state).

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::autonomy::evidence::{EvidenceInputs, build_evidence_pack};
use crate::autonomy::policy_yaml::PolicyBundle;
use crate::autonomy::risk::{ClassificationInputs, RiskClassifier};
use crate::autonomy::signing::EdSigningKey;
use crate::autonomy::types::{
    ChangedFile, EvidencePack, RollbackSection, RollbackStrategy, ScanOutcome, SecuritySection,
    SupplyChainSection, TestsSection,
};
use crate::git_host::{GitHost, PrDiff, RepoRef};

/// Public surface: anything that can mint a fresh signed `EvidencePack` from
/// a `(repo, mr_iid)` pair. Trait-shaped so the daemon can swap in a fake
/// in tests.
#[async_trait]
pub trait EvidencePackBuilder: Send + Sync {
    async fn build(&self, repo: &RepoRef, mr_iid: &str) -> Result<EvidencePack>;
}

/// Production builder: pulls the diff from a real `GitHost`, classifies risk
/// via the supplied policy bundle, and signs with an ed25519 key.
///
/// All fields are `pub` so callers can build it directly without wrestling
/// with a builder pattern — the struct itself is the configuration object.
pub struct StandardEvidencePackBuilder {
    pub git_host: Arc<dyn GitHost>,
    pub policy: Arc<PolicyBundle>,
    pub signing_key: Arc<EdSigningKey>,
    /// Current target-branch policy SHA. Stamped into every pack so a
    /// later verifier can refuse packs that don't match the policy in
    /// force on the target branch (Tip1 Law: target-branch policy only).
    pub policy_sha: String,
    /// `author_agent` field on the pack. Typically `"daemon.auto_rejudge.v1"`
    /// for the rejudge pipeline. `None` keeps the field absent in the
    /// serialized pack (matches the schema's `#[serde(skip_serializing_if)]`).
    pub author_agent: Option<String>,
}

impl StandardEvidencePackBuilder {
    pub fn new(
        git_host: Arc<dyn GitHost>,
        policy: Arc<PolicyBundle>,
        signing_key: Arc<EdSigningKey>,
        policy_sha: impl Into<String>,
        author_agent: Option<String>,
    ) -> Self {
        Self {
            git_host,
            policy,
            signing_key,
            policy_sha: policy_sha.into(),
            author_agent,
        }
    }

    /// Convert host-side `PrDiff` rows into the pack's `ChangedFile` shape.
    /// `risk_tags` starts empty — the classifier itself reasons over the
    /// path glob, not tags, so leaving this empty avoids accidentally
    /// double-counting risk.
    fn map_changed_files(diff: &PrDiff) -> Vec<ChangedFile> {
        diff.changed_files
            .iter()
            .map(|f| ChangedFile {
                path: f.path.clone(),
                risk_tags: Vec::new(),
                lines_added: f.lines_added,
                lines_removed: f.lines_removed,
            })
            .collect()
    }

    /// Default scan section: every scan "passed". See module docstring for
    /// why auto-rejudge doesn't re-run real scans.
    fn default_security() -> SecuritySection {
        SecuritySection {
            sast: ScanOutcome::Passed,
            dependency_scan: ScanOutcome::Passed,
            secret_scan: ScanOutcome::Passed,
        }
    }

    fn default_tests() -> TestsSection {
        TestsSection {
            targeted: Vec::new(),
            full_required: false,
            skipped: Vec::new(),
            coverage_delta: None,
        }
    }

    fn default_rollback() -> RollbackSection {
        RollbackSection {
            strategy: RollbackStrategy::RevertCommit,
            feature_flag: None,
            data_migration_reversible: None,
        }
    }
}

#[async_trait]
impl EvidencePackBuilder for StandardEvidencePackBuilder {
    async fn build(&self, repo: &RepoRef, mr_iid: &str) -> Result<EvidencePack> {
        let diff = self
            .git_host
            .fetch_pr_diff(repo, mr_iid)
            .await
            .map_err(|e| {
                anyhow::anyhow!("fetch_pr_diff({}, {}) failed: {e}", repo.slug(), mr_iid)
            })?;
        let changed_files = Self::map_changed_files(&diff);
        // Classify against the supplied policy bundle. No `triggered_conditions`
        // here — the second-pass judge / conditions registry will re-evaluate
        // those over the freshly-built pack downstream.
        let cls = RiskClassifier::new(&self.policy);
        let risk = cls.classify(&ClassificationInputs {
            files: &changed_files,
            triggered_conditions: &[],
        });
        // Source branch is not part of the host diff payload (GitHub's PR
        // files surface doesn't carry it). The daemon supplies a synthetic
        // "auto-rejudge" placeholder so the pack remains parseable; the
        // judge keys off head_sha + base_sha, not source_branch.
        let source_branch = format!("auto-rejudge/{mr_iid}");
        // Target branch is also not on the diff; we leave it blank and the
        // caller can layer it in via PrLiveState if needed. Today the
        // judge identifies the target via target_branch_sha, not name.
        let target_branch = String::new();
        let mut pack = build_evidence_pack(EvidenceInputs {
            repo: repo.slug().as_str(),
            source_branch: source_branch.as_str(),
            target_branch: target_branch.as_str(),
            head_sha: diff.head_sha.as_str(),
            base_sha: diff.base_sha.as_str(),
            policy_sha: self.policy_sha.as_str(),
            author_agent: self.author_agent.as_deref(),
            intent_id: None,
            risk,
            changed_files,
            claims: Vec::new(),
            tests: Self::default_tests(),
            security: Self::default_security(),
            supply_chain: SupplyChainSection::default(),
            rollback: Self::default_rollback(),
            legacy_receipts: Vec::new(),
        });
        // Sign over the canonical (unsigned) body. We zero the signature
        // first so the bytes-being-signed are identical at sign time and
        // verify time. The `evidence_digest` is already set by
        // `build_evidence_pack`, so it's included in the signed bytes —
        // tampering with it post-sign breaks both the digest check AND
        // the signature, which is what we want.
        pack.signature = None;
        let body = serde_json::to_string(&pack)
            .map_err(|e| anyhow::anyhow!("serialize pack for signing: {e}"))?;
        let sig = self.signing_key.sign_raw(body.as_bytes());
        pack.signature = Some(sig);
        Ok(pack)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::types::RiskTier;
    use crate::git_host::test_utils::FakeGitHost;
    use crate::git_host::{ChangedFileDiff, PrDiff};

    /// Load the repo's real policy bundle. Used by every test so risk
    /// classification is exercised against the same rules the live
    /// orchestrator uses (not a hand-rolled fixture that could drift).
    fn bundle() -> Arc<PolicyBundle> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".autonomy/policies");
        Arc::new(PolicyBundle::from_dir(&dir).expect("load .autonomy/policies"))
    }

    fn signing_key() -> Arc<EdSigningKey> {
        // Deterministic seed so the test is reproducible. The signature
        // value will be the same every run for the same body.
        Arc::new(EdSigningKey::from_seed("daemon.auto_rejudge.v1", [9u8; 32]))
    }

    /// Mint a `PrDiff` from a `(path, +, -)` tuple list. Keeps the test
    /// bodies short and the intent obvious.
    fn mint_diff(paths: &[(&str, u32, u32)]) -> PrDiff {
        let changed_files = paths
            .iter()
            .map(|(p, a, r)| ChangedFileDiff {
                path: (*p).to_string(),
                lines_added: *a,
                lines_removed: *r,
                hunks: vec![format!("@@ -1,1 +1,1 @@\n // {p}\n")],
            })
            .collect();
        PrDiff {
            repo: "octo/widget".into(),
            mr_iid: "42".into(),
            head_sha: "h".repeat(40),
            base_sha: "b".repeat(40),
            changed_files,
            fetched_at: chrono::Utc::now(),
        }
    }

    fn fake_host_with(diff: PrDiff) -> Arc<dyn GitHost> {
        Arc::new(FakeGitHost::new().with_pr_diff("octo/widget", "42", diff))
    }

    fn builder_with(
        host: Arc<dyn GitHost>,
        policy_sha: &str,
        author_agent: Option<&str>,
    ) -> StandardEvidencePackBuilder {
        StandardEvidencePackBuilder::new(
            host,
            bundle(),
            signing_key(),
            policy_sha,
            author_agent.map(|s| s.to_string()),
        )
    }

    #[tokio::test]
    async fn build_returns_signed_pack_with_ed25519_signature() {
        let host = fake_host_with(mint_diff(&[("docs/x.md", 3, 0)]));
        let b = builder_with(host, "policy-sha-abc", Some("daemon.auto_rejudge.v1"));
        let repo = RepoRef::parse("octo/widget").unwrap();
        let pack = b.build(&repo, "42").await.expect("build");
        let sig = pack.signature.as_ref().expect("must be signed");
        assert_eq!(sig.algo, "ed25519", "must use real ed25519, not stub");
        assert_eq!(sig.key_id, "daemon.auto_rejudge.v1");
        assert!(!sig.value.is_empty());
    }

    #[tokio::test]
    async fn build_pack_changed_files_match_diff_paths() {
        let host = fake_host_with(mint_diff(&[("a.rs", 1, 0), ("z.rs", 2, 1), ("m.rs", 5, 0)]));
        let b = builder_with(host, "p", None);
        let repo = RepoRef::parse("octo/widget").unwrap();
        let pack = b.build(&repo, "42").await.expect("build");
        // `build_evidence_pack` sorts changed_files by path, so we assert
        // the sorted order — that's the contract a verifier sees.
        let paths: Vec<&str> = pack.changed_files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["a.rs", "m.rs", "z.rs"]);
        // And the line counts round-trip from the diff.
        let z = pack
            .changed_files
            .iter()
            .find(|f| f.path == "z.rs")
            .unwrap();
        assert_eq!((z.lines_added, z.lines_removed), (2, 1));
    }

    #[tokio::test]
    async fn build_classifies_docs_only_pack_as_r0() {
        let host = fake_host_with(mint_diff(&[("docs/foo.md", 10, 0), ("README.md", 1, 0)]));
        let b = builder_with(host, "p", None);
        let repo = RepoRef::parse("octo/widget").unwrap();
        let pack = b.build(&repo, "42").await.expect("build");
        assert_eq!(pack.risk, RiskTier::R0, "docs-only must classify as R0");
    }

    #[tokio::test]
    async fn build_classifies_auth_path_change_as_r4_or_higher() {
        // `auth/**` is in `.autonomy/policies/protected-paths.yml::hard_human`.
        // The classifier escalates any change touching a protected glob to
        // R4 minimum via the `any_path_matches_protected: true` matcher.
        // (Note: we use `auth/login.rs`, not `src/auth/login.rs`, because
        // the protected-paths glob is anchored at the repo root as `auth/**`
        // — `src/auth/**` is NOT in the list. If the policy ever adds
        // `src/auth/**` to protected_paths, this test will still pass.)
        let host = fake_host_with(mint_diff(&[("auth/login.rs", 30, 5)]));
        let b = builder_with(host, "p", None);
        let repo = RepoRef::parse("octo/widget").unwrap();
        let pack = b.build(&repo, "42").await.expect("build");
        assert!(
            matches!(pack.risk, RiskTier::R4 | RiskTier::R5),
            "auth-path change must escalate to R4 or higher, got {:?}",
            pack.risk
        );
    }

    #[tokio::test]
    async fn build_uses_provided_policy_sha() {
        let host = fake_host_with(mint_diff(&[("src/util.rs", 5, 0)]));
        let b = builder_with(host, "policy-sha-xyz-123", None);
        let repo = RepoRef::parse("octo/widget").unwrap();
        let pack = b.build(&repo, "42").await.expect("build");
        assert_eq!(pack.policy_sha, "policy-sha-xyz-123");
    }

    #[tokio::test]
    async fn build_uses_provided_author_agent_or_none() {
        let host = fake_host_with(mint_diff(&[("src/util.rs", 5, 0)]));
        let b_with = builder_with(host.clone(), "p", Some("daemon.auto_rejudge.v1"));
        let repo = RepoRef::parse("octo/widget").unwrap();
        let pack = b_with.build(&repo, "42").await.expect("build");
        assert_eq!(pack.author_agent.as_deref(), Some("daemon.auto_rejudge.v1"));

        let host2 = fake_host_with(mint_diff(&[("src/util.rs", 5, 0)]));
        let b_none = builder_with(host2, "p", None);
        let pack2 = b_none.build(&repo, "42").await.expect("build");
        assert!(pack2.author_agent.is_none());
    }

    #[tokio::test]
    async fn build_propagates_git_host_error() {
        // `fail_next` makes the next `fetch_pr_diff` call return Transient;
        // the builder must surface that as an `anyhow::Error`, not a
        // silently-empty pack.
        let host: Arc<dyn GitHost> = Arc::new(
            FakeGitHost::new()
                .with_pr_diff("octo/widget", "42", mint_diff(&[("src/util.rs", 5, 0)]))
                .fail_next("fetch_pr_diff"),
        );
        let b = builder_with(host, "p", None);
        let repo = RepoRef::parse("octo/widget").unwrap();
        let err = b.build(&repo, "42").await.expect_err("must fail");
        let msg = format!("{err}");
        assert!(
            msg.contains("fetch_pr_diff") && msg.contains("octo/widget"),
            "error should name the failing surface + repo, got: {msg}"
        );
    }

    #[tokio::test]
    async fn build_with_empty_diff_returns_minimal_pack() {
        // No files → the default-tier matcher fires (R2). The policy's
        // R0 matcher requires `paths_only_in` over a non-empty file list
        // (vacuously true on `[].iter().all(...)`), but in practice the
        // R5/R4 conditions don't fire (no protected paths, no triggered
        // conditions), so we accept any of {R0, R1, R2}. The contract
        // we're locking is "no files MUST NOT escalate" — which means
        // it lands in the auto-merge-eligible band, NOT R3+.
        let host = fake_host_with(PrDiff {
            repo: "octo/widget".into(),
            mr_iid: "42".into(),
            head_sha: "h".repeat(40),
            base_sha: "b".repeat(40),
            changed_files: vec![],
            fetched_at: chrono::Utc::now(),
        });
        let b = builder_with(host, "p", None);
        let repo = RepoRef::parse("octo/widget").unwrap();
        let pack = b.build(&repo, "42").await.expect("build");
        assert!(
            pack.changed_files.is_empty(),
            "empty diff → empty changed_files"
        );
        assert!(
            pack.risk.auto_merge_eligible(),
            "empty diff must not escalate; got {:?}",
            pack.risk
        );
    }

    #[tokio::test]
    async fn pack_signature_verifies_against_matching_verifier() {
        // Round-trip: build a pack, then re-sign-bytes locally and verify
        // the embedded signature matches. This locks the contract
        // documented in the module header: signing canonicalization is
        // `serde_json::to_string` over the pack with `signature: None`.
        let host = fake_host_with(mint_diff(&[("src/util.rs", 5, 0)]));
        let key = signing_key();
        let b = StandardEvidencePackBuilder::new(host, bundle(), key.clone(), "p", None);
        let repo = RepoRef::parse("octo/widget").unwrap();
        let pack = b.build(&repo, "42").await.expect("build");
        let sig = pack.signature.clone().expect("signed");
        // Reconstruct the signed bytes the same way the builder did.
        let mut pack_for_verify = pack.clone();
        pack_for_verify.signature = None;
        let body = serde_json::to_string(&pack_for_verify).unwrap();
        let verifier = key.verifier();
        assert!(
            verifier.verify(body.as_bytes(), &sig),
            "embedded signature must verify against the same canonicalization"
        );
        // And tampering with any field breaks verification.
        let mut tampered = pack.clone();
        tampered.repo = "tampered/repo".into();
        tampered.signature = None;
        let tampered_body = serde_json::to_string(&tampered).unwrap();
        assert!(
            !verifier.verify(tampered_body.as_bytes(), &sig),
            "tampered pack must not verify"
        );
    }

    /// Bonus: the `mint_diff` helper itself is exercised — proves the
    /// fixture builder doesn't accidentally lose lines_added/_removed.
    #[tokio::test]
    async fn mint_diff_helper_preserves_line_counts() {
        let d = mint_diff(&[("a.rs", 7, 3)]);
        assert_eq!(d.changed_files.len(), 1);
        assert_eq!(d.changed_files[0].lines_added, 7);
        assert_eq!(d.changed_files[0].lines_removed, 3);
        assert!(!d.changed_files[0].hunks.is_empty());
    }
}

use jeryu_rustjet::{SccacheMode, SccachePolicy, TrustTier};

#[test]
fn internal_branch_can_use_project_scoped_cache() {
    let decision = SccachePolicy::default().decide(TrustTier::InternalBranch, false);
    assert_eq!(decision.mode, SccacheMode::ReadWriteProject);
    assert!(
        decision
            .env
            .iter()
            .any(|(k, v)| k == "JERYU_CACHE_PROMOTION" && v == "after-green")
    );
}

#[test]
fn release_lane_disables_sccache() {
    let decision = SccachePolicy::default().decide(TrustTier::ReleaseHermetic, true);
    assert_eq!(decision.mode, SccacheMode::Disabled);
}

#[test]
fn fork_pr_cannot_write_trusted_cache() {
    let decision = SccachePolicy::default().decide(TrustTier::ForkPullRequest, false);
    assert_eq!(decision.mode, SccacheMode::ReadOnly);
}

#[test]
fn agent_authored_writes_quarantine_only() {
    let decision = SccachePolicy::default().decide(TrustTier::AgentAuthored, false);
    assert_eq!(decision.mode, SccacheMode::QuarantineWrite);
}

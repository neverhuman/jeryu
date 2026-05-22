use super::*;

#[test]
fn agent_state_terminal_vs_active() {
    assert!(AgentState::Completed.is_terminal());
    assert!(!AgentState::Validating.is_terminal());
    assert!(AgentState::Validating.is_active());
    assert!(!AgentState::Paused.is_active());
}

#[test]
fn budget_pct_calculations() {
    let b = AgentBudget {
        time_used_secs: 900,
        time_limit_secs: 2700,
        ..Default::default()
    };
    assert!((b.time_pct() - 33.33).abs() < 0.1);
    assert!(!b.is_exhausted());
}

#[test]
fn default_session_is_spawning() {
    let s = AgentSession::default();
    assert_eq!(s.state, AgentState::Spawning);
    assert_eq!(s.trust_tier, TrustTier::Untrusted);
}

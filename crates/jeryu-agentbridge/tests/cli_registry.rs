//! Integration tests for the agent CLI registry.

use jeryu_agentbridge::cli_registry::{
    AgentCli, ClaudeAdapter, CliAdapter, CliRegistryError, CodexAdapter, Effort, JekkoAdapter,
    ModelSelect, plan_launch, route_model,
};

#[test]
fn routes_known_model_families() {
    assert_eq!(route_model("gpt-5.5").unwrap(), AgentCli::Codex);
    assert_eq!(route_model("GPT-5.5").unwrap(), AgentCli::Codex);
    assert_eq!(route_model("o3-mini").unwrap(), AgentCli::Codex);
    assert_eq!(route_model("codex-mini").unwrap(), AgentCli::Codex);
    assert_eq!(route_model("claude-opus-4-8").unwrap(), AgentCli::Claude);
    assert_eq!(route_model("sonnet").unwrap(), AgentCli::Claude);
    assert_eq!(route_model("haiku").unwrap(), AgentCli::Claude);
    assert_eq!(
        route_model("jekko:anthropic/claude-3").unwrap(),
        AgentCli::Jekko
    );
    assert_eq!(
        route_model("openrouter/some-model").unwrap(),
        AgentCli::Jekko
    );
}

#[test]
fn unroutable_model_fails_closed() {
    let err = route_model("totally-unknown-model").unwrap_err();
    assert!(matches!(err, CliRegistryError::UnroutableModel(_)));
    assert!(err.to_string().contains("agent_cli_unroutable_model"));
    assert!(matches!(
        route_model("").unwrap_err(),
        CliRegistryError::UnroutableModel(_)
    ));
    assert!(matches!(
        route_model("   ").unwrap_err(),
        CliRegistryError::UnroutableModel(_)
    ));
}

#[test]
fn jekko_prefix_wins_over_inner_family() {
    assert_eq!(route_model("jekko:gpt-4o").unwrap(), AgentCli::Jekko);
    assert_eq!(route_model("jekko:claude-opus").unwrap(), AgentCli::Jekko);
}

#[test]
fn claude_launch_is_headless_stream_json_with_model_and_effort() {
    let model = ModelSelect::new("claude-opus-4-8").with_effort(Effort::XHigh);
    let plan = ClaudeAdapter.build_launch("claude", &model, true);
    assert_eq!(plan.program, "claude");
    assert!(plan.prompt_on_stdin);
    assert!(plan.args.contains(&"--print".to_string()));
    assert!(plan.args.contains(&"stream-json".to_string()));
    let model_idx = plan.args.iter().position(|a| a == "--model").unwrap();
    assert_eq!(plan.args[model_idx + 1], "claude-opus-4-8");
    let effort_idx = plan.args.iter().position(|a| a == "--effort").unwrap();
    assert_eq!(plan.args[effort_idx + 1], "xhigh");
}

#[test]
fn codex_launch_uses_exec_and_reasoning_effort_override() {
    let model = ModelSelect::new("gpt-5.5").with_effort(Effort::XHigh);
    let plan = CodexAdapter.build_launch("codex", &model, true);
    assert_eq!(plan.args.first().unwrap(), "exec");
    assert!(plan.prompt_on_stdin);
    let m_idx = plan.args.iter().position(|a| a == "-m").unwrap();
    assert_eq!(plan.args[m_idx + 1], "gpt-5.5");
    assert!(
        plan.args
            .iter()
            .any(|a| a == r#"model_reasoning_effort="xhigh""#)
    );
    assert!(plan.args.contains(&"workspace-write".to_string()));
    assert!(plan.args.contains(&"--json".to_string()));
}

#[test]
fn jekko_launch_is_headless_with_provider_and_model() {
    let model = ModelSelect::new("claude-opus-4-7").with_provider("anthropic");
    let plan = JekkoAdapter.build_launch("jekko", &model, true);
    assert_eq!(plan.args.first().unwrap(), "run");
    assert!(plan.args.contains(&"--headless".to_string()));
    assert!(plan.args.contains(&"--ephemeral".to_string()));
    let p_idx = plan.args.iter().position(|a| a == "--provider").unwrap();
    assert_eq!(plan.args[p_idx + 1], "anthropic");
    let m_idx = plan.args.iter().position(|a| a == "--model").unwrap();
    assert_eq!(plan.args[m_idx + 1], "claude-opus-4-7");
    assert!(plan.args.contains(&"--json".to_string()));
}

#[test]
fn plan_launch_routes_then_builds() {
    let (cli, plan) = plan_launch("codex", &ModelSelect::new("gpt-5.5"), true).unwrap();
    assert_eq!(cli, AgentCli::Codex);
    assert_eq!(plan.args.first().unwrap(), "exec");

    let (cli, plan) = plan_launch("claude", &ModelSelect::new("claude-opus-4-8"), true).unwrap();
    assert_eq!(cli, AgentCli::Claude);
    assert!(plan.args.contains(&"--print".to_string()));

    assert!(plan_launch("claude", &ModelSelect::new("nope"), true).is_err());
}

#[test]
fn agent_cli_and_effort_roundtrip_strings() {
    for cli in [AgentCli::Claude, AgentCli::Codex, AgentCli::Jekko] {
        assert_eq!(cli.as_str().parse::<AgentCli>().unwrap(), cli);
    }
    for effort in [
        Effort::Low,
        Effort::Medium,
        Effort::High,
        Effort::XHigh,
        Effort::Max,
    ] {
        assert_eq!(effort.as_str().parse::<Effort>().unwrap(), effort);
    }
    assert!("nonsense".parse::<AgentCli>().is_err());
    assert!("nonsense".parse::<Effort>().is_err());
}

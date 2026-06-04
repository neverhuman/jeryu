use std::collections::BTreeMap;

use jeryu_agent_stream::{
    AgentControlCommand, AgentControlEnvelope, AgentControlSink, AgentEventSink, AgentOutputStream,
    AgentRunStreamKey, AgentTtyEvent, BrokerConfig, CONTROL_TOPIC, MemoryAgentBus, TTY_TOPIC,
};

fn key(run_id: &str) -> AgentRunStreamKey {
    AgentRunStreamKey {
        repo: Some("jeryu/jeryu".to_string()),
        workcell_id: "wc-1".to_string(),
        agent_run_id: run_id.to_string(),
        agent: "codex".to_string(),
        model: "model-x".to_string(),
    }
}

#[test]
fn memory_bus_preserves_event_order_and_run_key() {
    let bus = MemoryAgentBus::new();
    let run = key("ar-1");

    bus.emit(AgentTtyEvent::text(
        1,
        10,
        &run,
        AgentOutputStream::Pty,
        "hello",
    ))
    .expect("emit");
    bus.emit(AgentTtyEvent::finished(2, 11, &run, Some(0), "enforced"))
        .expect("emit");

    let events = bus.events_for_run("ar-1");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].seq, 1);
    assert_eq!(events[0].agent_run_id, "ar-1");
    assert_eq!(events[1].exit_code, Some(0));
    assert_eq!(events[1].enforcement_level.as_deref(), Some("enforced"));
}

#[test]
fn control_envelope_records_kind() {
    let bus = MemoryAgentBus::new();
    let control = AgentControlEnvelope::new(
        "ar-1",
        AgentControlCommand::ResizePty {
            cols: 120,
            rows: 40,
        },
    );
    assert_eq!(control.command.kind(), "resize_pty");

    bus.send_control(control).expect("control");
    assert_eq!(bus.controls()[0].agent_run_id, "ar-1");
}

#[test]
fn missing_required_stream_config_has_required_repair_fields() {
    let err = BrokerConfig::from_env(&BTreeMap::new()).expect_err("missing stream");
    assert_eq!(err.code, "agent_stream_required_unavailable");
    assert!(!err.repair.purpose.is_empty());
    assert!(!err.repair.reason.is_empty());
    assert!(!err.repair.common_fixes.is_empty());
    assert!(!err.repair.docs_url.is_empty());
    assert!(!err.repair.repair_hint.is_empty());
}

#[test]
fn event_schema_has_expected_topics_and_fields() {
    assert_eq!(TTY_TOPIC, "jeryu.agent.tty.v1");
    assert_eq!(CONTROL_TOPIC, "jeryu.agent.control.v1");
    let event = AgentTtyEvent::text(1, 99, &key("ar-9"), AgentOutputStream::Stdout, "x");
    let json = serde_json::to_value(event).expect("json");
    for field in [
        "schema_version",
        "event_id",
        "seq",
        "occurred_at_ms",
        "repo",
        "workcell_id",
        "agent_run_id",
        "agent",
        "model",
        "direction",
        "stream",
        "text",
        "bytes_b64",
        "truncated",
        "budget",
        "exit_code",
        "enforcement_level",
    ] {
        assert!(json.get(field).is_some(), "missing field {field}");
    }
}

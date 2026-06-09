//! Feature-gated broker adapter placeholder.
//!
//! This module deliberately fails closed until a real producer and consumer
//! dependency is wired.

use crate::{
    AgentControlEnvelope, AgentControlSink, AgentEventSink, AgentStreamError, AgentTtyEvent,
    BrokerConfig,
};

/// Broker stream bus.
pub struct BrokerAgentBus {
    _config: BrokerConfig,
}

impl BrokerAgentBus {
    /// Connect to the production broker. Fails closed until a backend is wired.
    pub fn connect(config: BrokerConfig) -> Result<Self, AgentStreamError> {
        let _ = config;
        Err(AgentStreamError::new(
            "agent_stream_broker_adapter_unavailable",
            "connect agent-run broker adapter",
            "the broker adapter feature is present but no producer/consumer backend is wired",
            &[
                "wire the broker adapter to the approved client library",
                "rerun the jeryu-agent-stream broker feature tests before enabling live agent work",
            ],
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-agent-stream --features broker-adapter --jobs 40",
        ))
    }
}

impl AgentEventSink for BrokerAgentBus {
    fn emit(&self, _event: AgentTtyEvent) -> Result<(), AgentStreamError> {
        Err(AgentStreamError::new(
            "agent_stream_broker_adapter_unavailable",
            "publish agent-run TTY event",
            "the broker adapter is not wired",
            &["wire the broker adapter before requiring production streams"],
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-agent-stream --features broker-adapter --jobs 40",
        ))
    }
}

impl AgentControlSink for BrokerAgentBus {
    fn send_control(&self, _control: AgentControlEnvelope) -> Result<(), AgentStreamError> {
        Err(AgentStreamError::new(
            "agent_stream_broker_adapter_unavailable",
            "publish agent-run control event",
            "the broker adapter is not wired",
            &["wire the broker adapter before requiring production streams"],
            "docs/testing.md#workcells",
            "rerun cargo test -p jeryu-agent-stream --features broker-adapter --jobs 40",
        ))
    }
}

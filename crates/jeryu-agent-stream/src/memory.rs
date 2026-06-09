use std::sync::Mutex;

use crate::{
    AgentControlEnvelope, AgentControlSink, AgentEventSink, AgentStreamError, AgentTtyEvent,
};

/// In-memory stream bus for deterministic tests.
#[derive(Default)]
pub struct MemoryAgentBus {
    events: Mutex<Vec<AgentTtyEvent>>,
    controls: Mutex<Vec<AgentControlEnvelope>>,
}

impl MemoryAgentBus {
    /// Create an empty memory bus.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot emitted events.
    pub fn events(&self) -> Vec<AgentTtyEvent> {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Snapshot control commands.
    pub fn controls(&self) -> Vec<AgentControlEnvelope> {
        self.controls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Events for one run id.
    pub fn events_for_run(&self, agent_run_id: &str) -> Vec<AgentTtyEvent> {
        self.events()
            .into_iter()
            .filter(|event| event.agent_run_id == agent_run_id)
            .collect()
    }
}

impl AgentEventSink for MemoryAgentBus {
    fn emit(&self, event: AgentTtyEvent) -> Result<(), AgentStreamError> {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(event);
        Ok(())
    }
}

impl AgentControlSink for MemoryAgentBus {
    fn send_control(&self, control: AgentControlEnvelope) -> Result<(), AgentStreamError> {
        self.controls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(control);
        Ok(())
    }
}

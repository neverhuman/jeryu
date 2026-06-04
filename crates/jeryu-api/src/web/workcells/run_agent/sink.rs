use jeryu_agentbridge::driver::{AgentEvent, AgentEventSink};

use super::types::AgentWorkcellRunEvent;

#[derive(Default)]
pub(super) struct SerializingAgentSink {
    events: std::sync::Mutex<Vec<AgentWorkcellRunEvent>>,
}

impl SerializingAgentSink {
    pub(super) fn events(&self) -> Vec<AgentWorkcellRunEvent> {
        self.events.lock().expect("agent sink mutex").clone()
    }
}

impl AgentEventSink for SerializingAgentSink {
    fn emit(&self, ev: AgentEvent) {
        self.events
            .lock()
            .expect("agent sink mutex")
            .push(AgentWorkcellRunEvent::from(ev));
    }
}

//! Agent-run TTY and control stream contracts.
//!
//! The production transport is supplied by an approved queue adapter, but the
//! schema and in-memory test bus are transport independent. A missing required
//! production stream is a typed denial, never a silent local fallback.

mod config;
mod error;
mod memory;
mod schema;

#[cfg(feature = "broker-adapter")]
pub mod broker_adapter;

pub use config::{BrokerConfig, missing_required_stream};
pub use error::{AgentStreamError, AgentStreamRepair};
pub use memory::MemoryAgentBus;
pub use schema::{
    AgentControlCommand, AgentControlEnvelope, AgentEventBudget, AgentOutputStream,
    AgentRunStreamKey, AgentStreamDirection, AgentTtyEvent, CONTROL_TOPIC, SCHEMA_VERSION,
    TTY_TOPIC,
};

/// Event sink trait.
pub trait AgentEventSink {
    /// Emit an event.
    fn emit(&self, event: AgentTtyEvent) -> Result<(), AgentStreamError>;
}

/// Control bus trait.
pub trait AgentControlSink {
    /// Send a control command.
    fn send_control(&self, control: AgentControlEnvelope) -> Result<(), AgentStreamError>;
}

//! Endpoint-neutral JSON messages for the `jeryu.runner.v1` runner protocol.
//!
//! This module deliberately owns no transport, URL, credential, persistence,
//! or service lifecycle. Callers authenticate outside these payloads, then
//! decode and validate a message here before invoking pure registry/scheduler
//! logic.

macro_rules! impl_message_io {
    ($type_name:ty) => {
        impl $type_name {
            /// Validate every scalar, collection, and internal binding.
            pub fn validate(&self) -> Result<(), super::validation::WireError> {
                super::validation::ValidateWire::validate_wire(self)
            }

            /// Decode one bounded JSON body using the closed schema.
            pub fn from_json(bytes: &[u8]) -> Result<Self, super::validation::WireError> {
                super::validation::decode_message(bytes)
            }

            /// Validate and encode one compact bounded JSON body.
            pub fn to_json(&self) -> Result<Vec<u8>, super::validation::WireError> {
                super::validation::encode_message(self)
            }
        }
    };
}

macro_rules! impl_redacted_debug {
    ($($type_name:ty),+ $(,)?) => {
        $(
            impl std::fmt::Debug for $type_name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.debug_struct(stringify!($type_name))
                        .field("payload", &"<redacted>")
                        .finish()
                }
            }
        )+
    };
}

mod body;
mod context;
mod heartbeat;
mod job;
mod lease;
mod register;
mod result;
mod validation;

pub use body::{WireArtifactPath, WireArtifactWhen, WireCacheMode, WireCacheMount, WireStep};
pub use context::{ExecutionContext, RunnerContext, WireRunnerClass};
pub use heartbeat::{HeartbeatAck, HeartbeatRequest};
pub use job::{WireJobOutcome, WireJobRequest, WireJobResult};
pub use lease::{LeaseAck, LeaseDecision, LeaseGrant, LeaseRequest};
pub use register::{RegisterAck, RegisterRequest};
pub use result::{ResultAck, ResultDecision, ResultRequest};
pub use validation::{WireError, WireErrorCode};

/// Maximum accepted or emitted JSON body size.
pub const MAX_MESSAGE_BYTES: usize = 1_048_576;
/// Maximum number of runner labels in one registration.
pub const MAX_LABELS: usize = 64;
/// Maximum advertised runner capacity.
pub const MAX_CAPACITY: u32 = 1_024;
/// Maximum number of steps in a leased job body.
pub const MAX_STEPS: usize = 256;
/// Maximum number of result artifact digests or cache receipts.
pub const MAX_RESULT_ITEMS: usize = 256;

pub(super) const REGISTER_REQUEST: &str = "register-request";
pub(super) const REGISTER_ACK: &str = "register-ack";
pub(super) const HEARTBEAT_REQUEST: &str = "heartbeat-request";
pub(super) const HEARTBEAT_ACK: &str = "heartbeat-ack";
pub(super) const LEASE_REQUEST: &str = "lease-request";
pub(super) const LEASE_ACK: &str = "lease-ack";
pub(super) const RESULT_REQUEST: &str = "result-request";
pub(super) const RESULT_ACK: &str = "result-ack";

//! Owner: messaging module — webhook → selected message log → autonomy consumer
//! Proof: `cargo test -p jeryu --lib messaging::`
//! Invariants: producer fan-out is fire-and-forget; consumer commits offsets idempotently.
//!
//! Webhooks land on the HTTP path → producer enqueues to a typed topic →
//! the autonomy daemon's consumer loop drains it and calls the existing
//! `handle_*_event_from_body` functions. The selected message log is shared
//! between producer and consumer through a process singleton.

#[cfg(any(feature = "kafka-backend", feature = "jansu-broker"))]
pub mod backend;
#[cfg(feature = "jansu-broker")]
pub mod broker;
#[cfg(any(feature = "kafka-backend", feature = "jansu-broker"))]
pub mod consumer_loop;
#[cfg(feature = "kafka-backend")]
pub mod kafka;
pub mod topics;

#[cfg(any(feature = "kafka-backend", feature = "jansu-broker"))]
pub use backend::{
    MessageLogConsumer, MessageLogError, MessageLogHandle, MessageRecord, init_message_log,
    message_log_handle,
};
#[cfg(feature = "jansu-broker")]
pub use broker::{BrokerError, BrokerHandle, ConsumerHandle, broker_handle, init_broker};

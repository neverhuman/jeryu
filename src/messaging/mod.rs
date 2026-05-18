//! Owner: messaging module — webhook → embedded jansu broker → autonomy consumer
//! Proof: `cargo nextest run -p jeryu --features jansu-broker messaging::`
//! Invariants: producer fan-out is fire-and-forget; consumer commits offsets idempotently.
//!
//! Webhooks land on the HTTP path → producer enqueues to a typed topic →
//! the autonomy daemon's consumer loop drains it and calls the existing
//! `handle_*_event_from_body` functions. The broker is in-process (no TCP),
//! shared between producer and consumer via a `OnceCell` singleton.
//!
//! Feature-gated behind `jansu-broker` (default-on). With the feature off,
//! webhook handlers fall back to the legacy inline call path.

#[cfg(feature = "jansu-broker")]
pub mod broker;
#[cfg(feature = "jansu-broker")]
pub mod consumer_loop;
pub mod topics;

#[cfg(feature = "jansu-broker")]
pub use broker::{BrokerError, BrokerHandle, ConsumerHandle, broker_handle, init_broker};

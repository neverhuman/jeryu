//! `jeryu-cli`: the operator/agent command taxonomy for the jeryu forge.
//!
//! The binary (`jeryu`) is a thin wrapper over this library so the taxonomy,
//! dispatch router, and client seam are unit- and snapshot-testable without a
//! process boundary.
//!
//! Layers:
//! - [`cli`]: pure clap data (no logic).
//! - [`commands`]: thin adapters that map each clap leaf onto a client call.
//! - [`dispatch`]: the router that wires the two together and yields an exit code.
//! - [`client`]: the [`client::ForgeClient`] seam plus an in-memory client.

pub mod cli;
pub mod client;
pub mod commands;
pub mod dispatch;

pub use cli::Cli;
pub use client::{ForgeClient, InMemoryClient};
pub use dispatch::dispatch;

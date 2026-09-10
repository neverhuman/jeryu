//! `jeryu proof` taxonomy: verify a changeset and explain a blocker.

use clap::Subcommand;

/// Proof command group.
#[derive(Debug, Subcommand)]
pub enum ProofCommands {
    /// Unavailable: verify a changeset proof (no server transport).
    Verify {
        /// Changeset identifier or inline descriptor to verify.
        changeset: String,
    },
    /// Unavailable: explain a proof blocker (no server transport).
    Explain {
        /// Blocker identifier to explain.
        id: String,
    },
}

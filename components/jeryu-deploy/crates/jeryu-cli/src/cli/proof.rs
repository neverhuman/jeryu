//! `jeryu proof` taxonomy: verify a changeset and explain a blocker.

use clap::Subcommand;

/// Proof command group.
#[derive(Debug, Subcommand)]
pub enum ProofCommands {
    /// Verify a changeset and emit an admissibility verdict + plan hash.
    Verify {
        /// Changeset identifier or inline descriptor to verify.
        changeset: String,
    },
    /// Explain a proof blocker by identifier.
    Explain {
        /// Blocker identifier to explain.
        id: String,
    },
}

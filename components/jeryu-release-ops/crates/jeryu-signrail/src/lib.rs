//! SignRail release assurance core for Jeryu Phase 8.
//!
//! SignRail owns release artifacts, checksums, SBOMs, provenance statements,
//! signatures, release witnesses, rollback metadata, and OIDC job identity.

pub mod artifact;
pub mod checksum;
pub mod cli;
pub mod error;
pub mod identity;
pub mod json;
pub mod policy;
pub mod provenance;
pub mod receipt;
pub mod release;
mod release_cli_output;
pub mod rollback;
pub mod sbom;
pub mod signature;
pub mod store;
pub mod witness;

pub use artifact::Artifact;
pub use error::{Result, SignRailError};
pub use identity::OidcJobIdentity;
pub use policy::{ReleasePolicy, validate_release};
pub use provenance::{ProvenanceStatement, RELEASE_WITNESS_MARKER, SignedProvenance};
pub use release::Release;
pub use rollback::RollbackMetadata;
pub use sbom::SbomDocument;
pub use signature::{Ed25519Signer, HmacSha256Signer, Signature, Signer, UnavailableSigner};
pub use store::ArtifactStore;
pub use witness::ReleaseWitness;

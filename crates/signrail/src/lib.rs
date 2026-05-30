//! SignRail release assurance core for JitForge Nitro Phase 8.
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
pub mod rollback;
pub mod sbom;
pub mod signature;
pub mod store;
pub mod witness;

pub use artifact::Artifact;
pub use error::{Result, SignRailError};
pub use identity::OidcJobIdentity;
pub use policy::{validate_release, ReleasePolicy};
pub use provenance::{ProvenanceStatement, SignedProvenance};
pub use release::Release;
pub use rollback::RollbackMetadata;
pub use sbom::SbomDocument;
pub use signature::{HmacSha256Signer, Signature, Signer, UnavailableSigner};
pub use store::ArtifactStore;
pub use witness::ReleaseWitness;

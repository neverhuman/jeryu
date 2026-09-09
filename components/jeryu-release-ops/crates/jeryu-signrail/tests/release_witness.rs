use jeryu_signrail::{
    Artifact, ArtifactStore, HmacSha256Signer, OidcJobIdentity, ProvenanceStatement,
    RELEASE_WITNESS_MARKER, Release, ReleasePolicy, RollbackMetadata, SbomDocument, Signer,
    UnavailableSigner, validate_release,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "release_witness/support.rs"]
mod support;
use support::*;

#[path = "release_witness/cli.rs"]
mod cli;
#[path = "release_witness/policy.rs"]
mod policy;

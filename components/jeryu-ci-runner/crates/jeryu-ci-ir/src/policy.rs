//! Pipeline-level governance policies (cache, artifact, permission, secret,
//! proof, signing) and their secure-by-default values.

use crate::enums::TokenScope;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachePolicy {
    pub project_scoped: bool,
    pub allow_cross_project_compiled: bool,
    pub promote_after_green: bool,
    pub quarantine_untrusted_writes: bool,
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            project_scoped: true,
            allow_cross_project_compiled: false,
            promote_after_green: true,
            quarantine_untrusted_writes: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactPolicy {
    pub allow_absolute_paths: bool,
    pub require_metadata: bool,
    pub default_retention_days: u32,
}

impl Default for ArtifactPolicy {
    fn default() -> Self {
        Self {
            allow_absolute_paths: false,
            require_metadata: true,
            default_retention_days: 14,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionPolicy {
    pub default_token_scope: TokenScope,
    pub fail_closed: bool,
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        Self {
            default_token_scope: TokenScope::ReadRepo,
            fail_closed: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretPolicy {
    pub secrets_available: bool,
    pub deny_on_fork: bool,
}

impl Default for SecretPolicy {
    fn default() -> Self {
        Self {
            secrets_available: false,
            deny_on_fork: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofPolicy {
    pub proof_required: bool,
    pub lane: String,
}

impl Default for ProofPolicy {
    fn default() -> Self {
        Self {
            proof_required: true,
            lane: "phase3-fast".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SigningPolicy {
    pub provenance_required: bool,
    pub release_only: bool,
}

impl Default for SigningPolicy {
    fn default() -> Self {
        Self {
            provenance_required: false,
            release_only: true,
        }
    }
}

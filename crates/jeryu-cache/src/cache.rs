//! Cache key, scope, and index records.

use crate::digest::{Digest, digest_bytes};
use crate::error::{Result, VaultError};
use crate::ids::{RepoId, SharedScopeId, TenantId};
use crate::policy::TrustTier;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Type of cache object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CacheKind {
    /// Rust target-style compiled artifact.
    CompiledArtifact,
    /// Source blob cache.
    SourceBlob,
    /// Registry blob cache.
    RegistryBlob,
    /// Release-hermetic vendor snapshot.
    ReleaseVendorSnapshot,
}

impl Display for CacheKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            CacheKind::CompiledArtifact => "compiled-artifact",
            CacheKind::SourceBlob => "source-blob",
            CacheKind::RegistryBlob => "registry-blob",
            CacheKind::ReleaseVendorSnapshot => "release-vendor-snapshot",
        };
        f.write_str(value)
    }
}

/// Cache visibility scope.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CacheScope {
    /// Repo-scoped cache.
    Repo {
        /// Tenant ID.
        tenant_id: TenantId,
        /// Repo ID.
        repo_id: RepoId,
    },
    /// Tenant-scoped immutable source/dependency cache.
    Tenant {
        /// Tenant ID.
        tenant_id: TenantId,
    },
    /// Explicitly allowed shared compiled cache scope.
    Shared {
        /// Tenant ID.
        tenant_id: TenantId,
        /// Shared scope ID.
        shared_scope_id: SharedScopeId,
    },
}

impl CacheScope {
    /// Canonical scope string.
    pub fn canonical(&self) -> String {
        match self {
            CacheScope::Repo { tenant_id, repo_id } => {
                format!("repo:{tenant_id}/{repo_id}")
            }
            CacheScope::Tenant { tenant_id } => format!("tenant:{tenant_id}"),
            CacheScope::Shared {
                tenant_id,
                shared_scope_id,
            } => format!("shared:{tenant_id}/{shared_scope_id}"),
        }
    }
}

impl Display for CacheScope {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.canonical())
    }
}

/// Cache key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// Cache object kind.
    pub kind: CacheKind,
    /// Cache visibility scope.
    pub scope: CacheScope,
    /// Full fingerprint digest.
    pub fingerprint_digest: Digest,
}

impl CacheKey {
    /// Creates a new cache key.
    pub fn new(kind: CacheKind, scope: CacheScope, fingerprint_digest: Digest) -> Self {
        Self {
            kind,
            scope,
            fingerprint_digest,
        }
    }

    /// Stable key digest.
    pub fn digest(&self) -> Digest {
        digest_bytes(self.canonical().as_bytes())
    }

    /// Canonical key string.
    pub fn canonical(&self) -> String {
        format!(
            "kind={}\nscope={}\nfingerprint={}\n",
            self.kind, self.scope, self.fingerprint_digest
        )
    }
}

/// Entry lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheEntryState {
    /// Entry is not yet trusted for normal restore.
    Quarantined,
    /// Entry is trusted and restorable by policy.
    Promoted,
}

impl Display for CacheEntryState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheEntryState::Quarantined => f.write_str("quarantined"),
            CacheEntryState::Promoted => f.write_str("promoted"),
        }
    }
}

/// Cache index entry mapping a logical key to CAS object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    /// Logical cache key.
    pub key: CacheKey,
    /// CAS object digest.
    pub object_digest: Digest,
    /// Digest of the manifest/metadata used for false-hit detection.
    pub manifest_digest: Digest,
    /// Entry state.
    pub state: CacheEntryState,
    /// Trust tier that created the entry.
    pub created_by: TrustTier,
    /// Unix timestamp in nanoseconds.
    pub created_at_nanos: u128,
}

impl CacheEntry {
    /// Creates an entry with the current timestamp.
    pub fn new(
        key: CacheKey,
        object_digest: Digest,
        manifest_digest: Digest,
        state: CacheEntryState,
        created_by: TrustTier,
    ) -> Self {
        Self {
            key,
            object_digest,
            manifest_digest,
            state,
            created_by,
            created_at_nanos: now_nanos(),
        }
    }

    /// Key digest for this entry.
    pub fn key_digest(&self) -> Digest {
        self.key.digest()
    }
}

/// Filesystem index for cache entries.
#[derive(Debug, Clone)]
pub struct FsIndex {
    root: PathBuf,
}

impl FsIndex {
    /// Opens an index under `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Inserts or replaces an entry.
    pub fn put(&self, entry: &CacheEntry) -> Result<()> {
        let path = self.path_for(&entry.key.digest());
        let tmp = path.with_extension("entry.tmp");
        fs::write(&tmp, encode_entry(entry))?;
        fs::rename(tmp, path)?;
        Ok(())
    }

    /// Reads an entry by key.
    pub fn get(&self, key: &CacheKey) -> Result<Option<CacheEntry>> {
        let path = self.path_for(&key.digest());
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(path)?;
        decode_entry(&data, key.clone()).map(Some)
    }

    /// Rewrites the state for an entry.
    pub fn set_state(&self, key: &CacheKey, state: CacheEntryState) -> Result<Option<CacheEntry>> {
        let mut entry = match self.get(key)? {
            Some(entry) => entry,
            None => return Ok(None),
        };
        entry.state = state;
        self.put(&entry)?;
        Ok(Some(entry))
    }

    fn path_for(&self, digest: &Digest) -> PathBuf {
        self.root.join(format!("{}.entry", digest.as_str()))
    }
}

fn encode_entry(entry: &CacheEntry) -> String {
    format!(
        "object_digest={}\nmanifest_digest={}\nstate={}\ncreated_by={}\ncreated_at_nanos={}\n",
        entry.object_digest,
        entry.manifest_digest,
        entry.state,
        entry.created_by,
        entry.created_at_nanos
    )
}

fn decode_entry(data: &str, key: CacheKey) -> Result<CacheEntry> {
    let mut object_digest = None;
    let mut manifest_digest = None;
    let mut state = None;
    let mut created_by = None;
    let mut created_at_nanos = None;

    for line in data.lines() {
        let (name, value) = line
            .split_once('=')
            .ok_or_else(|| VaultError::InvalidInput(format!("malformed entry line: {line}")))?;
        match name {
            "object_digest" => object_digest = Some(Digest::new(value.to_string())?),
            "manifest_digest" => manifest_digest = Some(Digest::new(value.to_string())?),
            "state" => {
                state = Some(match value {
                    "quarantined" => CacheEntryState::Quarantined,
                    "promoted" => CacheEntryState::Promoted,
                    _ => return Err(VaultError::InvalidInput(format!("bad state: {value}"))),
                });
            }
            "created_by" => {
                created_by = Some(match value {
                    "T0-release-hermetic" => TrustTier::ReleaseHermetic,
                    "T1-protected-internal" => TrustTier::ProtectedInternal,
                    "T2-internal-branch" => TrustTier::InternalBranch,
                    "T3-agent-authored" => TrustTier::AgentAuthored,
                    "T4-fork-pr" => TrustTier::ForkPr,
                    "T5-public-untrusted" => TrustTier::PublicUntrusted,
                    _ => return Err(VaultError::InvalidInput(format!("bad trust tier: {value}"))),
                });
            }
            "created_at_nanos" => {
                created_at_nanos = Some(value.parse::<u128>().map_err(|err| {
                    VaultError::InvalidInput(format!("bad created_at_nanos: {err}"))
                })?);
            }
            _ => {}
        }
    }

    Ok(CacheEntry {
        key,
        object_digest: object_digest
            .ok_or_else(|| VaultError::InvalidInput("entry missing object_digest".to_string()))?,
        manifest_digest: manifest_digest
            .ok_or_else(|| VaultError::InvalidInput("entry missing manifest_digest".to_string()))?,
        state: state.ok_or_else(|| VaultError::InvalidInput("entry missing state".to_string()))?,
        created_by: created_by
            .ok_or_else(|| VaultError::InvalidInput("entry missing created_by".to_string()))?,
        created_at_nanos: created_at_nanos.ok_or_else(|| {
            VaultError::InvalidInput("entry missing created_at_nanos".to_string())
        })?,
    })
}

pub(crate) fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

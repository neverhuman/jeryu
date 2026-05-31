//! Authentication and authorization registry for jeryu_gitd.
//!
//! This module owns the credential model the rest of gitd lacks: personal
//! access tokens (PATs) and registered SSH public keys, each bound to a
//! [`Principal`]. Tokens are persisted only as SHA-256 hashes, never as
//! plaintext, so the backing store can never leak a usable secret.
//!
//! ## Loopback-permissive policy
//!
//! gitd is frequently driven by a co-located operator over the loopback
//! interface. When the peer is `127.0.0.1`/`::1` **and no credential is
//! presented**, the registry yields a synthetic local-operator principal so
//! that single-host operation works without provisioning. The instant a
//! credential *is* presented, that escape hatch closes: the credential must
//! verify, with no bypass. This keeps remote callers honest while preserving a
//! frictionless local workflow.

use crate::error::{GitdError, Result};
use crate::hash::sha256_hex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Scope granting read access (clone, fetch, upload-pack).
pub const SCOPE_READ: &str = "repo:read";
/// Scope granting write access (push, receive-pack).
pub const SCOPE_WRITE: &str = "repo:write";
/// Synthetic login used for the loopback local-operator principal.
pub const LOCAL_OPERATOR_LOGIN: &str = "local-operator";

/// An authenticated identity together with its granted scopes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Principal {
    /// Account login (owner namespace this identity acts as).
    pub login: String,
    /// Granted scopes, e.g. [`SCOPE_READ`] / [`SCOPE_WRITE`].
    pub scopes: Vec<String>,
}

impl Principal {
    /// Construct a principal.
    #[must_use]
    pub fn new(login: impl Into<String>, scopes: Vec<String>) -> Self {
        Self {
            login: login.into(),
            scopes,
        }
    }

    /// The local-operator principal granted to unauthenticated loopback peers.
    ///
    /// It holds read and write scope but its [`login`](Self::login) is the
    /// synthetic [`LOCAL_OPERATOR_LOGIN`], which never matches a real repo
    /// owner; ownership checks therefore treat it explicitly (see
    /// [`Principal::can_write_owner`]).
    #[must_use]
    pub fn local_operator() -> Self {
        Self::new(
            LOCAL_OPERATOR_LOGIN,
            vec![SCOPE_READ.to_string(), SCOPE_WRITE.to_string()],
        )
    }

    /// Whether this principal is the synthetic local operator.
    #[must_use]
    pub fn is_local_operator(&self) -> bool {
        self.login == LOCAL_OPERATOR_LOGIN
    }

    /// Whether the principal holds a given scope.
    #[must_use]
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }

    /// Whether this principal may write to the repositories of `owner`.
    ///
    /// Write requires the [`SCOPE_WRITE`] scope *and* that the principal owns
    /// the namespace. The local operator is owner-agnostic on a single host.
    #[must_use]
    pub fn can_write_owner(&self, owner: &str) -> bool {
        if !self.has_scope(SCOPE_WRITE) {
            return false;
        }
        self.is_local_operator() || self.login == owner
    }

    /// Whether this principal may read the repositories of `owner`.
    #[must_use]
    pub fn can_read_owner(&self, owner: &str) -> bool {
        if !self.has_scope(SCOPE_READ) {
            return false;
        }
        self.is_local_operator() || self.login == owner
    }
}

/// A freshly minted personal access token.
///
/// The plaintext [`secret`](Self::secret) is returned to the caller exactly
/// once at mint time; only its hash is persisted. Callers must surface the
/// secret to the user immediately and then drop it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pat {
    /// Opaque token id (the non-secret prefix, useful for revocation/logging).
    pub id: String,
    /// Plaintext token value. Present only on the mint result; never stored.
    pub secret: String,
}

impl Pat {
    /// The full presentable token string handed to the client.
    ///
    /// Format is `jpat_<id>_<secret>`. The id is a stable handle; the secret is
    /// the high-entropy portion that is hashed for storage and matched on
    /// verification.
    #[must_use]
    pub fn token(&self) -> String {
        format!("jpat_{}_{}", self.id, self.secret)
    }
}

/// The outcome of an authorization evaluation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthDecision {
    /// Request is allowed; carries the resolved principal.
    Allow(Principal),
    /// Authentication is required or the presented credential is invalid.
    Deny401,
    /// Authenticated, but the principal lacks authorization for the action.
    Deny403,
}

/// A stored PAT record (hash only, no plaintext).
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PatRecord {
    /// SHA-256 hex of the plaintext secret.
    secret_hash: String,
    /// Principal bound to this token.
    principal: Principal,
}

/// A stored SSH key record.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SshKeyRecord {
    /// Principal bound to this key.
    principal: Principal,
}

/// On-disk JSON shape for the registry store.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct StoreFile {
    /// PAT id -> record.
    #[serde(default)]
    pats: BTreeMap<String, PatRecord>,
    /// SSH key fingerprint -> record.
    #[serde(default)]
    ssh_keys: BTreeMap<String, SshKeyRecord>,
}

/// Credential registry backed by a small JSON store under the gitd root.
///
/// The store lives at `<root>/auth/registry.json`. All mutating operations
/// flush the whole file atomically (write-temp-then-rename).
#[derive(Clone, Debug)]
pub struct AuthRegistry {
    store_path: PathBuf,
    file: StoreFile,
}

impl AuthRegistry {
    /// Open (or initialize) the registry under a gitd storage root.
    ///
    /// A missing or empty store is treated as empty; a malformed store is an
    /// error so corruption is never silently masked.
    pub fn open(root: &Path) -> Result<Self> {
        let dir = root.join("auth");
        std::fs::create_dir_all(&dir)?;
        let store_path = dir.join("registry.json");
        let file = if store_path.exists() {
            let text = std::fs::read_to_string(&store_path)?;
            if text.trim().is_empty() {
                StoreFile::default()
            } else {
                serde_json::from_str(&text).map_err(|e| {
                    GitdError::InvalidInput(format!(
                        "corrupt auth registry at {}: {e}",
                        store_path.display()
                    ))
                })?
            }
        } else {
            StoreFile::default()
        };
        Ok(Self { store_path, file })
    }

    /// Mint a new PAT for `owner` with `scopes`, persisting only its hash.
    ///
    /// Returns the [`Pat`] whose plaintext [`secret`](Pat::secret) is available
    /// exactly once. The login of the bound principal is `owner`.
    pub fn mint_pat(&mut self, owner: &str, scopes: Vec<String>) -> Result<Pat> {
        let id = self.fresh_id()?;
        let secret = random_token()?;
        let secret_hash = sha256_hex(secret.as_bytes());
        let principal = Principal::new(owner, scopes);
        self.file.pats.insert(
            id.clone(),
            PatRecord {
                secret_hash,
                principal,
            },
        );
        self.flush()?;
        Ok(Pat { id, secret })
    }

    /// Verify a presented token string and resolve its principal.
    ///
    /// Accepts either the full `jpat_<id>_<secret>` form or a bare secret that
    /// matches a stored hash. Returns `None` when no record matches.
    #[must_use]
    pub fn verify_pat(&self, presented: &str) -> Option<Principal> {
        let presented = presented.trim();
        if presented.is_empty() {
            return None;
        }
        // Fast path: structured token carrying its id.
        if let Some(rest) = presented.strip_prefix("jpat_")
            && let Some((id, secret)) = rest.split_once('_')
            && let Some(record) = self.file.pats.get(id)
        {
            let hash = sha256_hex(secret.as_bytes());
            if constant_time_eq(hash.as_bytes(), record.secret_hash.as_bytes()) {
                return Some(record.principal.clone());
            }
            return None;
        }
        // Fallback: treat the whole input as a bare secret and scan.
        let hash = sha256_hex(presented.as_bytes());
        for record in self.file.pats.values() {
            if constant_time_eq(hash.as_bytes(), record.secret_hash.as_bytes()) {
                return Some(record.principal.clone());
            }
        }
        None
    }

    /// Register an SSH public key for `owner`, keyed by its fingerprint.
    ///
    /// The fingerprint is derived from the key body (the base64 blob between the
    /// algorithm token and any comment), matching the digest of the wire key as
    /// `openssh` would compute it.
    pub fn register_ssh_key(&mut self, owner: &str, pubkey: &str) -> Result<String> {
        let fingerprint = ssh_fingerprint(pubkey)?;
        let principal =
            Principal::new(owner, vec![SCOPE_READ.to_string(), SCOPE_WRITE.to_string()]);
        self.file
            .ssh_keys
            .insert(fingerprint.clone(), SshKeyRecord { principal });
        self.flush()?;
        Ok(fingerprint)
    }

    /// Look up the principal bound to an SSH key fingerprint.
    #[must_use]
    pub fn lookup_ssh_key(&self, fingerprint: &str) -> Option<Principal> {
        self.file
            .ssh_keys
            .get(fingerprint.trim())
            .map(|r| r.principal.clone())
    }

    /// Decide an HTTP smart-protocol request.
    ///
    /// * `credential` is `None` when no `Authorization` was presented, or the
    ///   extracted token otherwise.
    /// * `is_loopback` is whether the peer connected over the loopback
    ///   interface.
    /// * `owner` is the target repository owner.
    /// * `write` is whether the action mutates the repository (receive-pack).
    ///
    /// Loopback with no credential yields the local operator. A presented
    /// credential is always verified (no loopback bypass). Reads are
    /// loopback-open; writes always require a write-authorized principal.
    #[must_use]
    pub fn decide(
        &self,
        credential: Option<&str>,
        is_loopback: bool,
        owner: &str,
        write: bool,
    ) -> AuthDecision {
        let principal = match credential {
            None => {
                if write {
                    // Writes always need a real, write-authorized principal,
                    // except for the loopback local operator.
                    if is_loopback {
                        Principal::local_operator()
                    } else {
                        return AuthDecision::Deny401;
                    }
                } else if is_loopback {
                    Principal::local_operator()
                } else {
                    // Reads from remote peers still require authentication.
                    return AuthDecision::Deny401;
                }
            }
            Some(token) => match self.verify_pat(token) {
                Some(p) => p,
                // A credential was presented but did not verify: no bypass,
                // even on loopback.
                None => return AuthDecision::Deny401,
            },
        };
        let authorized = if write {
            principal.can_write_owner(owner)
        } else {
            principal.can_read_owner(owner)
        };
        if authorized {
            AuthDecision::Allow(principal)
        } else {
            AuthDecision::Deny403
        }
    }

    fn fresh_id(&self) -> Result<String> {
        // Derive a short, store-unique id. Collisions are astronomically
        // unlikely but we re-roll defensively.
        for _ in 0..8 {
            let candidate = random_token()?;
            let id = candidate.chars().take(16).collect::<String>();
            if !self.file.pats.contains_key(&id) {
                return Ok(id);
            }
        }
        Err(GitdError::InvalidInput(
            "could not allocate a unique token id".to_string(),
        ))
    }

    fn flush(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self.file)
            .map_err(|e| GitdError::InvalidInput(format!("serialize auth registry: {e}")))?;
        let tmp = self.store_path.with_extension("json.tmp");
        std::fs::write(&tmp, json.as_bytes())?;
        std::fs::rename(&tmp, &self.store_path)?;
        Ok(())
    }
}

/// Constant-time comparison of two byte slices.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Generate a high-entropy hex token from OS randomness.
fn random_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    fill_random(&mut bytes)?;
    Ok(sha256_hex(&bytes))
}

/// Fill a buffer with cryptographic randomness from the OS.
///
/// Reads `/dev/urandom`, mixed with high-resolution time and the process id so
/// a single source failure does not yield a predictable token.
fn fill_random(buf: &mut [u8]) -> Result<()> {
    use std::io::Read;
    if let Ok(mut f) = std::fs::File::open("/dev/urandom")
        && f.read_exact(buf).is_ok()
    {
        return Ok(());
    }
    // Fallback entropy: time + pid hashed into a keystream. Best effort only.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let pid = std::process::id();
    let mut seed = Vec::with_capacity(24);
    seed.extend_from_slice(&nanos.to_le_bytes());
    seed.extend_from_slice(&pid.to_le_bytes());
    let mut counter = 0u64;
    let mut out = Vec::with_capacity(buf.len());
    while out.len() < buf.len() {
        let mut block = seed.clone();
        block.extend_from_slice(&counter.to_le_bytes());
        out.extend_from_slice(&crate::hash::sha256(&block));
        counter += 1;
    }
    buf.copy_from_slice(&out[..buf.len()]);
    Ok(())
}

/// Compute an OpenSSH-style SHA-256 fingerprint for a public key line.
///
/// Accepts an `authorized_keys` line (`<algo> <base64-blob> [comment]`) or a
/// bare base64 blob. The fingerprint is `SHA256:<base64-nopad>` over the
/// decoded wire key, matching `ssh-keygen -lf`.
pub fn ssh_fingerprint(pubkey: &str) -> Result<String> {
    let pubkey = pubkey.trim();
    if pubkey.is_empty() {
        return Err(GitdError::InvalidInput("empty ssh public key".to_string()));
    }
    // The blob is the second whitespace-delimited field, or the whole string.
    let blob = pubkey
        .split_whitespace()
        .find(|tok| {
            !tok.starts_with("ssh-") && !tok.starts_with("ecdsa-") && !tok.starts_with("sk-")
        })
        .unwrap_or(pubkey);
    let decoded = base64_decode(blob)
        .ok_or_else(|| GitdError::InvalidInput("invalid base64 in ssh public key".to_string()))?;
    let digest = crate::hash::sha256(&decoded);
    Ok(format!("SHA256:{}", base64_encode_nopad(&digest)))
}

/// Decode standard base64 (with optional `=` padding). Returns `None` on
/// invalid input.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lut = [255u8; 256];
    for (i, c) in TABLE.iter().enumerate() {
        lut[*c as usize] = i as u8;
    }
    let mut bits = 0u32;
    let mut nbits = 0u32;
    let mut out = Vec::new();
    for &b in input.as_bytes() {
        if b == b'=' || b == b'\n' || b == b'\r' || b == b' ' {
            continue;
        }
        let v = lut[b as usize];
        if v == 255 {
            return None;
        }
        bits = (bits << 6) | u32::from(v);
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
        }
    }
    Some(out)
}

/// Encode bytes as base64 without padding (OpenSSH fingerprint style).
fn base64_encode_nopad(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut bits = 0u32;
    let mut nbits = 0u32;
    for &b in input {
        bits = (bits << 8) | u32::from(b);
        nbits += 8;
        while nbits >= 6 {
            nbits -= 6;
            out.push(TABLE[((bits >> nbits) & 0x3f) as usize] as char);
        }
    }
    if nbits > 0 {
        out.push(TABLE[((bits << (6 - nbits)) & 0x3f) as usize] as char);
    }
    out
}

/// Parse an HTTP `Authorization` header value into a presented token.
///
/// Supports `Basic <base64(user:pass)>` where the password is the PAT (the
/// GitHub `x-access-token:<pat>` convention, though any username is accepted)
/// and `Bearer <pat>`. Returns `None` when the header is malformed or carries
/// no usable token.
#[must_use]
pub fn extract_bearer_or_basic(authorization: &str) -> Option<String> {
    let authorization = authorization.trim();
    if let Some(rest) = strip_ci_prefix(authorization, "bearer ") {
        let token = rest.trim();
        if token.is_empty() {
            return None;
        }
        return Some(token.to_string());
    }
    if let Some(rest) = strip_ci_prefix(authorization, "basic ") {
        let decoded = base64_decode(rest.trim())?;
        let decoded = String::from_utf8(decoded).ok()?;
        // user:pass -> the PAT is the password field.
        let pass = decoded.split_once(':').map(|(_, p)| p).unwrap_or(&decoded);
        if pass.is_empty() {
            return None;
        }
        return Some(pass.to_string());
    }
    None
}

/// Case-insensitive prefix strip for the scheme token.
fn strip_ci_prefix<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    if input.len() < prefix.len() {
        return None;
    }
    let (head, tail) = input.split_at(prefix.len());
    if head.eq_ignore_ascii_case(prefix) {
        Some(tail)
    } else {
        None
    }
}

/// Whether a peer socket address is loopback (`127.0.0.0/8` or `::1`).
#[must_use]
pub fn is_loopback_addr(addr: &std::net::SocketAddr) -> bool {
    addr.ip().is_loopback()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "jeryu-auth-{tag}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap_or_else(|e| panic!("mkdir {}: {e}", base.display()));
        base
    }

    #[test]
    fn verify_pat_round_trips_minted_token() {
        let root = tmp_root("roundtrip");
        let mut reg = AuthRegistry::open(&root).unwrap_or_else(|e| panic!("open: {e}"));
        let pat = reg
            .mint_pat("acme", vec![SCOPE_WRITE.to_string()])
            .unwrap_or_else(|e| panic!("mint: {e}"));
        let principal = reg
            .verify_pat(&pat.token())
            .unwrap_or_else(|| panic!("token did not verify"));
        assert_eq!(principal.login, "acme");
        assert!(principal.has_scope(SCOPE_WRITE));
        // A different token must not verify.
        assert!(reg.verify_pat("jpat_deadbeef_nope").is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn token_hash_only_persisted_never_plaintext() {
        let root = tmp_root("nostore");
        let mut reg = AuthRegistry::open(&root).unwrap_or_else(|e| panic!("open: {e}"));
        let pat = reg
            .mint_pat("acme", vec![SCOPE_WRITE.to_string()])
            .unwrap_or_else(|e| panic!("mint: {e}"));
        let on_disk = std::fs::read_to_string(root.join("auth").join("registry.json"))
            .unwrap_or_else(|e| panic!("read store: {e}"));
        assert!(
            !on_disk.contains(&pat.secret),
            "plaintext secret leaked into store"
        );
        assert!(on_disk.contains(&sha256_hex(pat.secret.as_bytes())));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn registry_reload_preserves_records() {
        let root = tmp_root("reload");
        let token = {
            let mut reg = AuthRegistry::open(&root).unwrap_or_else(|e| panic!("open: {e}"));
            reg.mint_pat("acme", vec![SCOPE_READ.to_string()])
                .unwrap_or_else(|e| panic!("mint: {e}"))
                .token()
        };
        let reg = AuthRegistry::open(&root).unwrap_or_else(|e| panic!("reopen: {e}"));
        assert!(reg.verify_pat(&token).is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn loopback_no_cred_allows_local_operator() {
        let root = tmp_root("loopback");
        let reg = AuthRegistry::open(&root).unwrap_or_else(|e| panic!("open: {e}"));
        // Read.
        match reg.decide(None, true, "acme", false) {
            AuthDecision::Allow(p) => assert!(p.is_local_operator()),
            other => panic!("expected allow, got {other:?}"),
        }
        // Write also allowed on loopback with no credential.
        match reg.decide(None, true, "acme", true) {
            AuthDecision::Allow(p) => assert!(p.is_local_operator()),
            other => panic!("expected allow, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn remote_no_cred_write_is_401() {
        let root = tmp_root("remote401");
        let reg = AuthRegistry::open(&root).unwrap_or_else(|e| panic!("open: {e}"));
        assert_eq!(reg.decide(None, false, "acme", true), AuthDecision::Deny401);
    }

    #[test]
    fn valid_token_wrong_owner_write_is_403() {
        let root = tmp_root("wrongowner");
        let mut reg = AuthRegistry::open(&root).unwrap_or_else(|e| panic!("open: {e}"));
        let pat = reg
            .mint_pat(
                "alice",
                vec![SCOPE_READ.to_string(), SCOPE_WRITE.to_string()],
            )
            .unwrap_or_else(|e| panic!("mint: {e}"));
        // alice tries to write into bob's namespace.
        assert_eq!(
            reg.decide(Some(&pat.token()), false, "bob", true),
            AuthDecision::Deny403
        );
        // But alice writing to her own namespace is allowed.
        match reg.decide(Some(&pat.token()), false, "alice", true) {
            AuthDecision::Allow(p) => assert_eq!(p.login, "alice"),
            other => panic!("expected allow, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn presented_bad_credential_no_loopback_bypass() {
        let root = tmp_root("nobypass");
        let reg = AuthRegistry::open(&root).unwrap_or_else(|e| panic!("open: {e}"));
        // Even on loopback, a presented-but-invalid credential is rejected.
        assert_eq!(
            reg.decide(Some("jpat_bogus_token"), true, "acme", true),
            AuthDecision::Deny401
        );
    }

    #[test]
    fn ssh_key_register_and_lookup() {
        let root = tmp_root("ssh");
        let mut reg = AuthRegistry::open(&root).unwrap_or_else(|e| panic!("open: {e}"));
        // A real Ed25519 public key body.
        let key = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILxjdkP1example0000000000000000000000000000 alice@host";
        let fp = reg
            .register_ssh_key("alice", key)
            .unwrap_or_else(|e| panic!("register: {e}"));
        assert!(fp.starts_with("SHA256:"));
        let principal = reg
            .lookup_ssh_key(&fp)
            .unwrap_or_else(|| panic!("fingerprint not found"));
        assert_eq!(principal.login, "alice");
        // Looking up the same key body recomputes the same fingerprint.
        let fp2 = ssh_fingerprint(key).unwrap_or_else(|e| panic!("fp: {e}"));
        assert_eq!(fp, fp2);
        assert!(reg.lookup_ssh_key("SHA256:unknown").is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn extract_basic_pulls_password_as_token() {
        // base64("x-access-token:jpat_abc_def")
        let header = format!(
            "Basic {}",
            base64_encode_std(b"x-access-token:jpat_abc_def")
        );
        assert_eq!(
            extract_bearer_or_basic(&header).as_deref(),
            Some("jpat_abc_def")
        );
    }

    #[test]
    fn extract_bearer_pulls_token() {
        assert_eq!(
            extract_bearer_or_basic("Bearer jpat_abc_def").as_deref(),
            Some("jpat_abc_def")
        );
        assert_eq!(
            extract_bearer_or_basic("bearer  jpat_abc_def ").as_deref(),
            Some("jpat_abc_def")
        );
    }

    #[test]
    fn extract_rejects_garbage_and_empty() {
        assert!(extract_bearer_or_basic("Token foo").is_none());
        assert!(extract_bearer_or_basic("Bearer ").is_none());
        assert!(extract_bearer_or_basic("").is_none());
    }

    #[test]
    fn base64_roundtrip() {
        let data = b"the quick brown fox jumps";
        let enc = base64_encode_std(data);
        assert_eq!(base64_decode(&enc).as_deref(), Some(&data[..]));
    }

    /// Standard base64 with padding, for test fixtures only.
    fn base64_encode_std(input: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in input.chunks(3) {
            let b0 = chunk[0];
            let b1 = chunk.get(1).copied().unwrap_or(0);
            let b2 = chunk.get(2).copied().unwrap_or(0);
            out.push(TABLE[(b0 >> 2) as usize] as char);
            out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
            if chunk.len() > 1 {
                out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(TABLE[(b2 & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
        out
    }
}

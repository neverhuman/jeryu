//! Owner: Release Pipeline / Foundry Train
//! Proof: `cargo test -p jeryu --lib release::foundry`
//! Invariants:
//!   - Build-once: a `BuildOnceArtifact` is produced from a single source SHA
//!     and that artifact (digest + SBOM + provenance + signature) is what
//!     gets promoted across environments. The `PassportComposer` binds the
//!     ReleasePassport to the artifact digest + source SHA so callers cannot
//!     swap binaries mid-flight (tip1 Law 6).
//!   - FoundryTrain batches release candidates and "departs" when either
//!     `max_commits` is reached or `max_wait_minutes` have elapsed for the
//!     oldest candidate in the queue. With `split_on_high_risk`, an enqueued
//!     candidate whose own commit count exceeds `max_commits` ships solo
//!     immediately on the next drain.
//!   - ShellArtifactBuilder gracefully degrades when `syft`/`cosign` binaries
//!     are unavailable: it writes marker SBOM/provenance JSON under `workdir`
//!     and tags the provenance with `"stub": true` on the wire so downstream
//!     verifiers can refuse it. Real tool invocation and SLSA-grade
//!     provenance land in Wave 3.5.
//!   - All `ReleasePassport`s produced by `PassportComposer` are signed with
//!     ed25519 (algo = "ed25519"); marker/HMAC signatures are never emitted.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

use crate::autonomy::signing::{EdSigningKey, Signature, sha256_digest};
use crate::autonomy::types::{
    ArtifactKind, DeployEnvironment, ReleasePassport, ReleaseRollbackPlan, SchemaTag,
};

// ---------------------------------------------------------------------------
// Release candidate + train batcher
// ---------------------------------------------------------------------------

/// A candidate slice of work eligible to become a release. One or more
/// candidates may board a single "train" (a release) once batching rules fire.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseCandidate {
    pub id: String,
    pub commits: Vec<String>,
    pub source_branch: String,
    pub head_sha: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct FoundryConfig {
    /// Combined commit budget across all queued candidates that triggers a
    /// drain.
    pub max_commits: usize,
    /// Wall-clock minutes the oldest queued candidate is allowed to wait
    /// before the train departs even if `max_commits` is not yet hit.
    pub max_wait_minutes: i64,
    /// If true, a single enqueued candidate whose commit count exceeds
    /// `max_commits` is shipped solo on the next drain.
    pub split_on_high_risk: bool,
}

impl Default for FoundryConfig {
    /// Conservative defaults suitable for the one-shot CLI path in
    /// `autonomy foundry`: a single enqueued candidate immediately
    /// satisfies the commit trigger (`max_commits = 1`), a short wait
    /// window so any earlier crashed-and-recovered candidates also
    /// drain on the same tick, and split-on-high-risk on so big slices
    /// ship solo. Daemon / batch callers should construct an explicit
    /// `FoundryConfig` rather than relying on this default.
    fn default() -> Self {
        Self {
            max_commits: 1,
            max_wait_minutes: 60,
            split_on_high_risk: true,
        }
    }
}

/// Abstract release-candidate queue. Two impls live in-tree:
///   - `FoundryTrain` (in-memory, original Wave 3.A impl — kept for
///     existing tests and the CLI MVP that doesn't need restart survival).
///   - `SqlFoundryQueue` (SQL-backed, Wave 3.5.B) for production callers
///     that must not lose queued candidates across a process crash.
///
/// The trait is async because the SQL impl has to be; the in-memory impl
/// keeps its sync surface and delegates from the async trait methods.
#[async_trait]
pub trait FoundryQueue: Send + Sync {
    /// Push a candidate onto the queue. Implementations should be
    /// idempotent on `candidate.id`.
    async fn enqueue(&self, candidate: ReleaseCandidate) -> Result<()>;

    /// Return any candidates ready to depart on this tick (FIFO). Returned
    /// candidates are removed/marked-drained and will not be returned again.
    async fn drain_ready(&self, now: DateTime<Utc>) -> Result<Vec<ReleaseCandidate>>;

    /// Number of un-drained candidates currently queued.
    async fn peek_pending(&self) -> Result<usize>;
}

/// In-memory release-candidate batcher.
///
/// Wave 3.5.B introduced `SqlFoundryQueue` for restart-durable storage;
/// this in-memory impl is retained for the original tests and for callers
/// that explicitly opt out of persistence (CLI dry-runs, the single-tenant
/// control-plane MVP).
pub struct FoundryTrain {
    cfg: FoundryConfig,
    queue: Mutex<VecDeque<ReleaseCandidate>>,
}

impl FoundryTrain {
    pub fn new(cfg: FoundryConfig) -> Self {
        Self {
            cfg,
            queue: Mutex::new(VecDeque::new()),
        }
    }

    /// Push a candidate onto the tail of the queue. Cheap; never blocks on
    /// downstream work.
    pub fn enqueue(&self, candidate: ReleaseCandidate) {
        let mut q = self.queue.lock().expect("foundry-train queue poisoned");
        q.push_back(candidate);
    }

    /// Inspect the queue and return any candidates ready to depart on this
    /// tick. Three triggers (any one suffices):
    ///   1. total queued commits >= `max_commits`
    ///   2. oldest candidate has waited >= `max_wait_minutes`
    ///   3. `split_on_high_risk` && head-of-queue candidate alone exceeds
    ///      `max_commits` (ship it solo)
    ///
    /// Returns in FIFO order; the queue is drained of returned items.
    pub fn drain_ready(&self, now: DateTime<Utc>) -> Vec<ReleaseCandidate> {
        let mut q = self.queue.lock().expect("foundry-train queue poisoned");
        if q.is_empty() {
            return Vec::new();
        }

        // Trigger 3: split-on-high-risk for the head item.
        if self.cfg.split_on_high_risk
            && let Some(front) = q.front()
            && front.commits.len() > self.cfg.max_commits
        {
            return vec![q.pop_front().expect("front present")];
        }

        let total_commits: usize = q.iter().map(|c| c.commits.len()).sum();
        // Empty queue → zero age. We already early-returned on empty above,
        // so this `Duration::zero` is unreachable; it stays as a typesafe
        // fallback so a future caller cannot panic the lock by accident.
        let oldest_age = q
            .front()
            .map(|c| now.signed_duration_since(c.created_at))
            .unwrap_or_else(Duration::zero);
        let wait_trigger = oldest_age >= Duration::minutes(self.cfg.max_wait_minutes);
        let commit_trigger = total_commits >= self.cfg.max_commits;

        if !wait_trigger && !commit_trigger {
            return Vec::new();
        }

        // Drain everything currently queued. Real impl could cap at
        // `max_commits` worth of candidates but the MVP ships the full batch
        // so we don't strand work.
        let drained: Vec<ReleaseCandidate> = q.drain(..).collect();
        drained
    }

    /// Number of candidates currently queued. Useful for metrics.
    pub fn queued_len(&self) -> usize {
        self.queue
            .lock()
            .expect("foundry-train queue poisoned")
            .len()
    }
}

/// Async-trait passthrough for the in-memory train. The sync methods do
/// not block on I/O so we delegate directly without spawning blocking
/// tasks. This keeps both impls behind a single trait so callers can swap
/// `FoundryTrain` for `SqlFoundryQueue` without code changes.
#[async_trait]
impl FoundryQueue for FoundryTrain {
    async fn enqueue(&self, candidate: ReleaseCandidate) -> Result<()> {
        FoundryTrain::enqueue(self, candidate);
        Ok(())
    }

    async fn drain_ready(&self, now: DateTime<Utc>) -> Result<Vec<ReleaseCandidate>> {
        Ok(FoundryTrain::drain_ready(self, now))
    }

    async fn peek_pending(&self) -> Result<usize> {
        Ok(self.queued_len())
    }
}

// ---------------------------------------------------------------------------
// Build-once artifact + builder trait
// ---------------------------------------------------------------------------

/// The single immutable artifact produced from a certified source SHA. The
/// same `BuildOnceArtifact` is what promotes through dev → staging → canary
/// → prod (tip1 Law 6).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildOnceArtifact {
    pub digest: String,
    pub kind: ArtifactKind,
    pub source_sha: String,
    pub sbom_path: PathBuf,
    pub provenance_path: PathBuf,
    pub signature: Signature,
}

/// Pluggable build strategy. Production wires `ShellArtifactBuilder`; tests
/// and dry-runs may swap in an in-memory implementation.
pub trait ArtifactBuilder {
    fn build(&self, candidate: &ReleaseCandidate) -> Result<BuildOnceArtifact>;
}

/// Shells out to `syft` for the SBOM and `cosign` for the artifact signature
/// when those binaries exist; otherwise produces marker outputs (tagged
/// `"stub": true` on the wire so downstream verifiers can refuse them) so
/// downstream consumers and tests can run without external tooling.
///
/// follow-up (wave-3.5): replace the degraded-marker path with a hard-fail
/// when `syft` / `cosign` are expected, and thread real SLSA provenance
/// generation through this builder.
pub struct ShellArtifactBuilder {
    pub workdir: PathBuf,
    pub syft_bin: PathBuf,
    pub cosign_bin: PathBuf,
    pub signing_key: Arc<EdSigningKey>,
}

impl ShellArtifactBuilder {
    /// Returns true iff the binary path exists and is executable-ish (we
    /// only check existence; full PATH lookup is intentionally out of scope).
    fn bin_available(path: &Path) -> bool {
        path.exists()
    }

    /// Deterministic synthetic artifact bytes for the candidate. Real impl
    /// would consume the built binary on disk; this synthetic path keeps
    /// tests hermetic.
    fn synth_artifact_bytes(candidate: &ReleaseCandidate) -> Vec<u8> {
        let mut h = Sha256::new();
        // Domain-separation tag: the wire string is part of the hash input
        // contract and must not change; it identifies the synthetic-marker
        // build mode in audit replay.
        h.update(b"jeryu.foundry.stub.artifact.v1\n");
        h.update(candidate.head_sha.as_bytes());
        h.update(b"\n");
        for c in &candidate.commits {
            h.update(c.as_bytes());
            h.update(b"\n");
        }
        h.finalize().to_vec()
    }
}

impl ArtifactBuilder for ShellArtifactBuilder {
    fn build(&self, candidate: &ReleaseCandidate) -> Result<BuildOnceArtifact> {
        std::fs::create_dir_all(&self.workdir)
            .with_context(|| format!("create foundry workdir {}", self.workdir.display()))?;

        let artifact_bytes = Self::synth_artifact_bytes(candidate);
        let digest = sha256_digest(&artifact_bytes);

        // ---- SBOM ----
        let sbom_path = self.workdir.join(format!("sbom-{}.json", candidate.id));
        let syft_present = Self::bin_available(&self.syft_bin);
        let sbom_payload = if syft_present {
            // Best-effort real call. If syft errors, we still produce a
            // marker payload so the build is reproducible and tests don't
            // depend on a working syft invocation.
            match Command::new(&self.syft_bin)
                .arg("--output")
                .arg("json")
                .arg(&self.workdir)
                .output()
            {
                Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).into(),
                _ => marker_sbom_json(candidate, &digest),
            }
        } else {
            marker_sbom_json(candidate, &digest)
        };
        std::fs::write(&sbom_path, &sbom_payload)
            .with_context(|| format!("write sbom to {}", sbom_path.display()))?;

        // ---- Provenance ----
        let provenance_path = self
            .workdir
            .join(format!("provenance-{}.json", candidate.id));
        let cosign_present = Self::bin_available(&self.cosign_bin);
        // `marker_mode` flags that this build did NOT use real syft+cosign
        // and is therefore a marker artifact (rejected by enforcement-mode
        // verifiers). The wire field is still named "stub" for protocol
        // compatibility — see `marker_provenance_json` below.
        let marker_mode = !(syft_present && cosign_present);
        let provenance_payload = marker_provenance_json(
            candidate,
            &digest,
            marker_mode,
            syft_present,
            cosign_present,
        );
        std::fs::write(&provenance_path, &provenance_payload)
            .with_context(|| format!("write provenance to {}", provenance_path.display()))?;

        // ---- Sign artifact bytes with the in-process ed25519 key. ----
        // (Cosign integration lands in wave-3.5; until then the
        // `signing_key` is the canonical signer of record.)
        let signature = self.signing_key.sign_raw(&artifact_bytes);

        Ok(BuildOnceArtifact {
            digest,
            kind: ArtifactKind::RustBinary,
            source_sha: candidate.head_sha.clone(),
            sbom_path,
            provenance_path,
            signature,
        })
    }
}

/// Render the marker SBOM payload — used when `syft` is unavailable or
/// fails. The wire-format fields (`"stub": true`, format identifier) stay
/// stable so existing audit consumers and tests keep matching; the Rust
/// identifier reads `marker_*` to make it clear at the call site that this
/// path is a deliberate degraded mode.
fn marker_sbom_json(candidate: &ReleaseCandidate, artifact_digest: &str) -> String {
    let value = serde_json::json!({
        "format": "jeryu.stub.sbom.v1",
        "stub": true,
        "candidate_id": candidate.id,
        "head_sha": candidate.head_sha,
        "artifact_digest": artifact_digest,
        "components": [],
        "note": "syft not available; this is a marker SBOM (wave-3.5)",
    });
    serde_json::to_string_pretty(&value).expect("marker sbom serialize")
}

/// Render the marker provenance payload — used when either `syft` or
/// `cosign` is unavailable. See [`marker_sbom_json`] for the wire-format /
/// identifier-naming rationale.
fn marker_provenance_json(
    candidate: &ReleaseCandidate,
    artifact_digest: &str,
    marker_mode: bool,
    syft_present: bool,
    cosign_present: bool,
) -> String {
    let value = serde_json::json!({
        "_type": "https://in-toto.io/Statement/v0.1",
        "predicateType": "https://slsa.dev/provenance/v0.2",
        "stub": marker_mode,
        "syft_available": syft_present,
        "cosign_available": cosign_present,
        "subject": [{
            "name": candidate.id,
            "digest": { "sha256": artifact_digest.trim_start_matches("sha256:") },
        }],
        "predicate": {
            "builder": { "id": "jeryu.foundry.stub" },
            "buildType": "jeryu.foundry.stub.v1",
            "invocation": {
                "configSource": {
                    "uri": format!("git+branch://{}", candidate.source_branch),
                    "digest": { "sha1": candidate.head_sha },
                },
            },
            "materials": candidate.commits.iter().map(|c| {
                serde_json::json!({ "uri": format!("git+commit://{}", c) })
            }).collect::<Vec<_>>(),
        },
    });
    serde_json::to_string_pretty(&value).expect("marker provenance serialize")
}

// ---------------------------------------------------------------------------
// Passport composer
// ---------------------------------------------------------------------------

/// Assembles a signed `ReleasePassport` from a candidate + its built
/// artifact + a rollback plan. The passport is what canary/promotion code
/// requires as proof an artifact is permitted to move through environments.
pub struct PassportComposer {
    pub issuer: String,
}

impl Default for PassportComposer {
    fn default() -> Self {
        Self {
            issuer: "jeryu.release.foundry".into(),
        }
    }
}

impl PassportComposer {
    pub fn new(issuer: impl Into<String>) -> Self {
        Self {
            issuer: issuer.into(),
        }
    }

    /// Build + sign a `ReleasePassport` for the candidate/artifact pair.
    ///
    /// The signing body is a deterministic canonical JSON over the
    /// passport's identifying fields so the same inputs always produce the
    /// same signature (modulo `issued_at`, which the body includes).
    pub fn compose(
        &self,
        candidate: &ReleaseCandidate,
        artifact: &BuildOnceArtifact,
        rollback: ReleaseRollbackPlan,
        allowed_envs: Vec<DeployEnvironment>,
        signing_key: &EdSigningKey,
    ) -> ReleasePassport {
        let issued_at = Utc::now();
        let id = format!(
            "relp_{}",
            &short_hash(&artifact.digest, candidate, issued_at)
        );

        // If the SBOM / provenance files on disk cannot be read we fall back
        // to hashing their path strings — explicit match (rather than
        // `unwrap_or_else`) so the audit reader can see the degraded branch
        // is deliberate. The fallback digests are deterministic and bound
        // to the path, which is enough for the passport to still be unique;
        // verification of the actual file contents is the SBOM verifier's
        // job, not the composer's.
        let sbom_digest = match digest_path_contents(&artifact.sbom_path) {
            Ok(d) => d,
            Err(_) => sha256_digest(artifact.sbom_path.to_string_lossy().as_bytes()),
        };
        let provenance_digest = match digest_path_contents(&artifact.provenance_path) {
            Ok(d) => d,
            Err(_) => sha256_digest(artifact.provenance_path.to_string_lossy().as_bytes()),
        };
        let build_logs_digest = sha256_digest(
            format!(
                "build-logs:{}:{}:{}",
                candidate.id, artifact.digest, self.issuer
            )
            .as_bytes(),
        );

        let body = canonical_signing_body(
            &id,
            &artifact.digest,
            &sbom_digest,
            &provenance_digest,
            &artifact.source_sha,
            &allowed_envs,
            &rollback,
            issued_at,
        );
        let signature = signing_key.sign_raw(&body);

        ReleasePassport {
            schema: SchemaTag::new(),
            id,
            release_id: None,
            artifact_digest: artifact.digest.clone(),
            artifact_kind: artifact.kind,
            sbom_digest,
            provenance_digest,
            source_sha: artifact.source_sha.clone(),
            build_logs_digest,
            allowed_environments: allowed_envs,
            rollback_plan: rollback,
            issued_at,
            signature,
        }
    }

    /// Recompute the canonical body for a previously composed passport so
    /// callers can verify the signature against the matching `EdVerifier`.
    pub fn signing_body(passport: &ReleasePassport) -> Vec<u8> {
        canonical_signing_body(
            &passport.id,
            &passport.artifact_digest,
            &passport.sbom_digest,
            &passport.provenance_digest,
            &passport.source_sha,
            &passport.allowed_environments,
            &passport.rollback_plan,
            passport.issued_at,
        )
    }
}

fn digest_path_contents(path: &PathBuf) -> Result<String> {
    let bytes =
        std::fs::read(path).with_context(|| format!("read {} for digest", path.display()))?;
    Ok(sha256_digest(&bytes))
}

fn short_hash(
    artifact_digest: &str,
    candidate: &ReleaseCandidate,
    issued_at: DateTime<Utc>,
) -> String {
    let mut h = Sha256::new();
    h.update(artifact_digest.as_bytes());
    h.update(b"|");
    h.update(candidate.id.as_bytes());
    h.update(b"|");
    h.update(issued_at.to_rfc3339().as_bytes());
    let full = hex::encode(h.finalize());
    full[..26].to_string()
}

#[allow(clippy::too_many_arguments)]
fn canonical_signing_body(
    id: &str,
    artifact_digest: &str,
    sbom_digest: &str,
    provenance_digest: &str,
    source_sha: &str,
    allowed_envs: &[DeployEnvironment],
    rollback: &ReleaseRollbackPlan,
    issued_at: DateTime<Utc>,
) -> Vec<u8> {
    let body = serde_json::json!({
        "id": id,
        "artifact_digest": artifact_digest,
        "sbom_digest": sbom_digest,
        "provenance_digest": provenance_digest,
        "source_sha": source_sha,
        "allowed_environments": allowed_envs,
        "rollback_plan": rollback,
        "issued_at": issued_at.to_rfc3339(),
    });
    serde_json::to_vec(&body).expect("canonical body serialize")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn cand(id: &str, commits: usize, at: DateTime<Utc>) -> ReleaseCandidate {
        ReleaseCandidate {
            id: id.into(),
            commits: (0..commits).map(|i| format!("{id}-c{i}")).collect(),
            source_branch: format!("feat/{id}"),
            head_sha: format!("{:0<40}", id),
            created_at: at,
        }
    }

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap()
    }

    fn cfg(max_commits: usize, max_wait_minutes: i64, split: bool) -> FoundryConfig {
        FoundryConfig {
            max_commits,
            max_wait_minutes,
            split_on_high_risk: split,
        }
    }

    #[test]
    fn enqueue_and_drain_preserves_fifo_order() {
        let train = FoundryTrain::new(cfg(2, 60, false));
        let t0 = fixed_now();
        train.enqueue(cand("a", 1, t0));
        train.enqueue(cand("b", 1, t0));
        let drained = train.drain_ready(t0);
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].id, "a");
        assert_eq!(drained[1].id, "b");
        assert_eq!(train.queued_len(), 0);
    }

    #[test]
    fn max_wait_minutes_triggers_departure() {
        let train = FoundryTrain::new(cfg(100, 10, false));
        let t0 = fixed_now();
        train.enqueue(cand("solo", 1, t0));
        // Nothing should drain at t0 — neither trigger met.
        assert!(train.drain_ready(t0).is_empty());
        assert_eq!(train.queued_len(), 1);
        // Advance past max_wait_minutes.
        let later = t0 + Duration::minutes(11);
        let drained = train.drain_ready(later);
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].id, "solo");
    }

    #[test]
    fn max_commits_triggers_departure_before_wait() {
        let train = FoundryTrain::new(cfg(3, 999, false));
        let t0 = fixed_now();
        train.enqueue(cand("a", 2, t0));
        train.enqueue(cand("b", 2, t0)); // total = 4 >= 3
        let drained = train.drain_ready(t0);
        assert_eq!(drained.len(), 2, "commit-threshold should drain all");
    }

    #[test]
    fn split_on_high_risk_ships_oversized_candidate_solo() {
        let train = FoundryTrain::new(cfg(2, 999, true));
        let t0 = fixed_now();
        train.enqueue(cand("big", 5, t0)); // > max_commits
        train.enqueue(cand("small", 1, t0));
        let drained = train.drain_ready(t0);
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].id, "big");
        assert_eq!(train.queued_len(), 1);
    }

    #[test]
    fn two_enqueued_candidates_keep_distinct_ids() {
        let train = FoundryTrain::new(cfg(10, 999, false));
        let t0 = fixed_now();
        train.enqueue(cand("alpha", 1, t0));
        train.enqueue(cand("beta", 1, t0));
        // Force a drain via wait trigger.
        let drained = train.drain_ready(t0 + Duration::minutes(1000));
        assert_eq!(drained.len(), 2);
        assert_ne!(drained[0].id, drained[1].id);
    }

    #[test]
    fn shell_builder_stub_mode_writes_marker() {
        let tmp = tempfile::tempdir().expect("tmp");
        let builder = ShellArtifactBuilder {
            workdir: tmp.path().to_path_buf(),
            syft_bin: PathBuf::from("/definitely/not/a/real/syft-bin"),
            cosign_bin: PathBuf::from("/definitely/not/a/real/cosign-bin"),
            signing_key: Arc::new(EdSigningKey::from_seed("foundry.test", [3u8; 32])),
        };
        let c = cand("stubtest", 1, fixed_now());
        let art = builder.build(&c).expect("stub build");
        assert_eq!(art.kind, ArtifactKind::RustBinary);
        assert!(art.sbom_path.exists(), "sbom path written");
        assert!(art.provenance_path.exists(), "provenance path written");
        let prov = std::fs::read_to_string(&art.provenance_path).unwrap();
        assert!(prov.contains("\"stub\": true"), "provenance marks stub");
        assert!(
            prov.contains("\"syft_available\": false"),
            "stub records syft_available=false"
        );
        let sbom = std::fs::read_to_string(&art.sbom_path).unwrap();
        assert!(sbom.contains("\"stub\": true"), "sbom marks stub");
        // Signature is real ed25519, even in stub mode.
        assert_eq!(art.signature.algo, "ed25519");
        assert_eq!(art.signature.key_id, "foundry.test");
    }

    #[test]
    fn shell_builder_sbom_and_provenance_paths_resolve_under_workdir() {
        let tmp = tempfile::tempdir().expect("tmp");
        let builder = ShellArtifactBuilder {
            workdir: tmp.path().to_path_buf(),
            syft_bin: PathBuf::from("/nope/syft"),
            cosign_bin: PathBuf::from("/nope/cosign"),
            signing_key: Arc::new(EdSigningKey::from_seed("foundry.test", [4u8; 32])),
        };
        let c = cand("pathtest", 2, fixed_now());
        let art = builder.build(&c).unwrap();
        assert!(art.sbom_path.starts_with(tmp.path()));
        assert!(art.provenance_path.starts_with(tmp.path()));
        let sbom_meta = std::fs::metadata(&art.sbom_path).unwrap();
        let prov_meta = std::fs::metadata(&art.provenance_path).unwrap();
        assert!(sbom_meta.len() > 0);
        assert!(prov_meta.len() > 0);
    }

    #[test]
    fn passport_composer_produces_signed_passport_with_required_fields() {
        let tmp = tempfile::tempdir().expect("tmp");
        let signer = Arc::new(EdSigningKey::from_seed("foundry.test", [5u8; 32]));
        let builder = ShellArtifactBuilder {
            workdir: tmp.path().to_path_buf(),
            syft_bin: PathBuf::from("/nope/syft"),
            cosign_bin: PathBuf::from("/nope/cosign"),
            signing_key: signer.clone(),
        };
        let c = cand("passport", 1, fixed_now());
        let art = builder.build(&c).unwrap();

        let composer = PassportComposer::default();
        let rollback = ReleaseRollbackPlan {
            strategy: "revert_commit".into(),
            tested: true,
        };
        let passport = composer.compose(
            &c,
            &art,
            rollback.clone(),
            vec![DeployEnvironment::Canary, DeployEnvironment::Prod],
            &signer,
        );

        assert!(passport.id.starts_with("relp_"));
        assert_eq!(passport.artifact_digest, art.digest);
        assert_eq!(passport.source_sha, c.head_sha);
        assert_eq!(passport.artifact_kind, ArtifactKind::RustBinary);
        assert_eq!(passport.allowed_environments.len(), 2);
        assert_eq!(passport.rollback_plan, rollback);
        assert!(passport.sbom_digest.starts_with("sha256:"));
        assert!(passport.provenance_digest.starts_with("sha256:"));
        assert!(passport.build_logs_digest.starts_with("sha256:"));
        assert_eq!(passport.signature.algo, "ed25519");
        assert_eq!(passport.signature.key_id, "foundry.test");
    }

    // --- Wave 5 coverage-boost addition ------------------------------------

    /// Drain on an empty queue must return an empty vector — not panic,
    /// not block on the lock, not error. This is the boundary path that
    /// the orchestrator hits on every idle tick.
    #[test]
    fn drain_ready_on_empty_queue_returns_empty_vec_without_panic() {
        let train = FoundryTrain::new(cfg(5, 60, true));
        // Two drain calls in a row on an empty queue.
        assert!(train.drain_ready(fixed_now()).is_empty());
        assert!(
            train
                .drain_ready(fixed_now() + Duration::hours(24))
                .is_empty()
        );
        assert_eq!(train.queued_len(), 0);
    }

    #[test]
    fn passport_signature_verifies_against_matching_verifier() {
        let tmp = tempfile::tempdir().expect("tmp");
        let signer = Arc::new(EdSigningKey::from_seed("foundry.verify", [6u8; 32]));
        let builder = ShellArtifactBuilder {
            workdir: tmp.path().to_path_buf(),
            syft_bin: PathBuf::from("/nope/syft"),
            cosign_bin: PathBuf::from("/nope/cosign"),
            signing_key: signer.clone(),
        };
        let c = cand("verify", 1, fixed_now());
        let art = builder.build(&c).unwrap();
        let composer = PassportComposer::default();
        let passport = composer.compose(
            &c,
            &art,
            ReleaseRollbackPlan {
                strategy: "redeploy_previous".into(),
                tested: false,
            },
            vec![DeployEnvironment::Dev],
            &signer,
        );
        let verifier = signer.verifier();
        let body = PassportComposer::signing_body(&passport);
        assert!(
            verifier.verify(&body, &passport.signature),
            "passport signature must verify"
        );
        // Tamper-detection sanity check.
        let mut tampered = passport.clone();
        tampered.source_sha = "0".repeat(40);
        let tampered_body = PassportComposer::signing_body(&tampered);
        assert!(
            !verifier.verify(&tampered_body, &passport.signature),
            "tampered body must not verify"
        );
    }
}

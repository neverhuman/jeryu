use jeryu_signrail::{
    Artifact, HmacSha256Signer, OidcJobIdentity, Release, ReleasePolicy, RollbackMetadata,
    SbomDocument, UnavailableSigner, validate_release,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(1)
}

fn temp_artifact(name: &str, contents: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("jeryu_signrail-{name}-{}", now()));
    fs::write(&path, contents).unwrap_or_else(|err| panic!("failed to write temp artifact: {err}"));
    path
}

fn identity() -> OidcJobIdentity {
    OidcJobIdentity::new(
        "https://jeryu.example.invalid",
        "jeryu_signrail",
        "repo:acme/jeryu:ref:refs/tags/v1.2.3",
        "acme/jeryu",
        ".jit/release@refs/tags/v1.2.3",
        "job-release-1",
        "runner-release-hermetic-1",
        now() + 3600,
    )
}

fn policy() -> ReleasePolicy {
    ReleasePolicy::strict(
        "https://git.example.invalid/acme/jeryu",
        "https://jeryu.example.invalid",
        "jeryu_signrail",
        now(),
    )
}

fn unsigned_release() -> Release {
    let path = temp_artifact("release.bin", b"release artifact bytes");
    let artifact = Artifact::from_file("jeryu-linux-x86_64", &path, "application/octet-stream")
        .unwrap_or_else(|err| panic!("artifact failed: {err}"));
    let mut release = Release::new(
        "rel_01JPHASE8",
        "Jeryu 1.2.3",
        "v1.2.3",
        "https://git.example.invalid/acme/jeryu",
        "abc123commit",
        "def456tree",
        "blake3:jeryu_ci_ir",
        "release-hermetic",
        "sha256:runner-rootfs",
        "sha256:toolchain",
        "sha256:cargo-lock",
        identity(),
    );
    release.add_artifact(artifact);
    release.attach_sbom(SbomDocument::from_artifacts(
        "v1.2.3",
        &release.artifacts,
        now(),
    ));
    release.attach_rollback(RollbackMetadata::new(
        "v1.2.2",
        "jit release rollback v1.2.2",
        "sha256:config",
        "no irreversible migration",
        now(),
    ));
    release
}

fn signed_release() -> (Release, HmacSha256Signer) {
    let mut release = unsigned_release();
    let signer = HmacSha256Signer::new("phase8-test-key", b"phase8-secret");
    release
        .sign_with(&signer, now())
        .unwrap_or_else(|err| panic!("signing failed: {err}"));
    (release, signer)
}

#[test]
fn valid_release_emits_witness_with_full_coverage() {
    let (release, signer) = signed_release();
    let witness = validate_release(&release, &policy(), &signer)
        .unwrap_or_else(|err| panic!("validation failed: {err}"));
    assert_eq!(witness.artifact_count, 1);
    assert_eq!(witness.signature_count, 1);
    assert_eq!(witness.signature_coverage_percent, 100);
}

#[test]
fn unsigned_release_blocked() {
    let release = unsigned_release();
    let signer = HmacSha256Signer::new("phase8-test-key", b"phase8-secret");
    let err = validate_release(&release, &policy(), &signer).unwrap_err();
    assert!(err.to_string().contains("signature coverage"));
}

#[test]
fn missing_sbom_blocked() {
    let mut release = unsigned_release();
    release.sbom = None;
    let signer = HmacSha256Signer::new("phase8-test-key", b"phase8-secret");
    let err = validate_release(&release, &policy(), &signer).unwrap_err();
    assert!(err.to_string().contains("missing SBOM"));
}

#[test]
fn missing_rollback_metadata_blocked() {
    let (mut release, signer) = signed_release();
    release.rollback = None;
    let err = validate_release(&release, &policy(), &signer).unwrap_err();
    assert!(err.to_string().contains("missing rollback metadata"));
}

#[test]
fn mutable_latest_only_asset_blocked() {
    let mut release = unsigned_release();
    release.version = "latest".to_string();
    release.set_immutable(false);
    let signer = HmacSha256Signer::new("phase8-test-key", b"phase8-secret");
    let err = validate_release(&release, &policy(), &signer).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("mutable"));
}

#[test]
fn wrong_source_sha_blocked() {
    let (mut release, signer) = signed_release();
    release.commit_sha = "wrong-sha".to_string();
    let err = validate_release(&release, &policy(), &signer).unwrap_err();
    assert!(err.to_string().contains("wrong source SHA"));
}

#[test]
fn signing_outage_fails_closed() {
    let mut release = unsigned_release();
    let signer = UnavailableSigner::new("offline-key");
    let err = release.sign_with(&signer, now()).unwrap_err();
    assert!(err.to_string().contains("signing unavailable"));
}

#[test]
fn provenance_signature_verifies() {
    let (mut release, signer) = signed_release();
    validate_release(&release, &policy(), &signer)
        .unwrap_or_else(|err| panic!("initial validation failed: {err}"));
    release.provenance[0].statement.artifact_digest = "sha256:tampered".to_string();
    let err = validate_release(&release, &policy(), &signer).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("unknown artifact") || msg.contains("signature mismatch"));
}

#[test]
fn signer_identity_mismatch_blocked() {
    let (release, _signer) = signed_release();
    let wrong_signer = HmacSha256Signer::new("different-key", b"phase8-secret");
    let err = validate_release(&release, &policy(), &wrong_signer).unwrap_err();
    assert!(err.to_string().contains("signer identity mismatch"));
}

#[test]
fn duplicate_provenance_artifact_digest_blocked() {
    let mut release = unsigned_release();
    let second_path = temp_artifact("release-extra.bin", b"extra artifact bytes");
    let second = Artifact::from_file(
        "jeryu-extra-linux-x86_64",
        &second_path,
        "application/octet-stream",
    )
    .unwrap_or_else(|err| panic!("artifact failed: {err}"));
    release.add_artifact(second);
    release.attach_sbom(SbomDocument::from_artifacts(
        "v1.2.3",
        &release.artifacts,
        now(),
    ));

    let signer = HmacSha256Signer::new("phase8-test-key", b"phase8-secret");
    release
        .sign_with(&signer, now())
        .unwrap_or_else(|err| panic!("signing failed: {err}"));
    release.provenance[1].statement.artifact_digest =
        release.provenance[0].statement.artifact_digest.clone();

    let err = validate_release(&release, &policy(), &signer).unwrap_err();
    assert!(
        err.to_string()
            .contains("duplicate provenance artifact digest")
    );
}

#[test]
fn oidc_expiry_blocks_release() {
    let (mut release, signer) = signed_release();
    release.oidc.expires_at_epoch = 1;
    let err = validate_release(&release, &policy(), &signer).unwrap_err();
    assert!(err.to_string().contains("expired"));
}

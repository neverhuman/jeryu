use super::*;

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
    assert!(err.to_string().contains("mutable"));
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
    let message = err.to_string();
    assert!(message.contains("unknown artifact") || message.contains("signature mismatch"));
}

#[test]
fn wrong_release_witness_marker_blocked() {
    let (mut release, signer) = signed_release();
    release.provenance[0].statement.jankurai_release_witness = "tampered-witness".to_string();
    release.provenance[0].signature = signer
        .sign(&release.provenance[0].statement.canonical_message())
        .unwrap_or_else(|err| panic!("resigning failed: {err}"));
    let err = validate_release(&release, &policy(), &signer).unwrap_err();
    assert!(err.to_string().contains("release witness marker mismatch"));
}

#[test]
fn signer_identity_mismatch_blocked() {
    let (release, _signer) = signed_release();
    let wrong_signer = HmacSha256Signer::new("different-key", b"phase8-secret");
    let err = validate_release(&release, &policy(), &wrong_signer).unwrap_err();
    assert!(err.to_string().contains("signer identity mismatch"));
}

#[test]
fn release_signing_uses_expected_witness_marker() {
    let (release, _signer) = signed_release();
    assert_eq!(
        release.provenance[0].statement.jankurai_release_witness,
        RELEASE_WITNESS_MARKER
    );
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

#[test]
fn artifact_store_shards_artifacts_and_sanitizes_json_names() {
    let root = temp_store_root("artifact");
    let path = temp_artifact("stored.bin", b"stored artifact bytes");
    let artifact = Artifact::from_file("stored.bin", &path, "application/octet-stream")
        .unwrap_or_else(|err| panic!("artifact failed: {err}"));
    let store = ArtifactStore::open(&root).unwrap_or_else(|err| panic!("store failed: {err}"));
    let stored = store
        .put_artifact(&artifact)
        .unwrap_or_else(|err| panic!("put artifact failed: {err}"));
    assert_eq!(
        fs::read(&stored).unwrap_or_else(|err| panic!("read stored failed: {err}")),
        b"stored artifact bytes"
    );
    assert!(stored.starts_with(store.root().join("artifacts")));
    assert!(stored.ends_with(PathBuf::from("stored.bin")));
    assert!(stored.to_string_lossy().contains(&artifact.digest[..2]));
    let json_path = store
        .put_json("receipts", "release/v1.2.3", "{\"ok\":true}")
        .unwrap_or_else(|err| panic!("put json failed: {err}"));
    assert!(json_path.ends_with(PathBuf::from("release_v1.2.3.json")));
    assert_eq!(
        fs::read_to_string(json_path).unwrap_or_else(|err| panic!("read json failed: {err}")),
        "{\"ok\":true}"
    );
}

#[test]
fn signed_provenance_json_preserves_canonical_witness_fields() {
    let (release, _signer) = signed_release();
    let provenance = &release.provenance[0];
    let canonical = String::from_utf8(provenance.statement.canonical_message())
        .unwrap_or_else(|err| panic!("canonical utf8 failed: {err}"));
    assert!(canonical.starts_with("source_repository=https://git.example.invalid/acme/jeryu\n"));
    assert!(canonical.contains("jankurai_release_witness=phase8-release-witness-required\n"));
    let json = provenance.to_json();
    assert!(json.contains("\"statement\":{"));
    assert!(json.contains("\"signature\":{"));
    assert!(json.contains("\"jankurai_release_witness\":\"phase8-release-witness-required\""));
}

#[test]
fn receipt_digest_changes_with_payload() {
    let first = jeryu_signrail::receipt::Receipt::new("release", "v1", "{\"ok\":true}");
    let second = jeryu_signrail::receipt::Receipt::new("release", "v1", "{\"ok\":false}");
    assert_ne!(first.digest, second.digest);
    let json = first.to_json();
    assert!(json.contains("\"kind\":\"release\""));
    assert!(json.contains("\"payload\":{\"ok\":true}"));
}

#[test]
fn rollback_metadata_requires_actionable_fields() {
    let rollback = RollbackMetadata::new("", "rollback", "sha256:config", "none", now());
    let err = rollback.validate().unwrap_err();
    assert!(err.to_string().contains("previous_release"));
}

#[test]
fn provenance_statement_json_escapes_fields() {
    let statement = ProvenanceStatement {
        source_repository: "https://git.example.invalid/acme/jeryu".to_string(),
        commit_sha: "abc".to_string(),
        tree_sha: "def".to_string(),
        jeryu_ci_ir_hash: "ir".to_string(),
        runner_class: "release-hermetic".to_string(),
        runner_rootfs_digest: "rootfs".to_string(),
        toolchain_digest: "toolchain".to_string(),
        cargo_lock_digest: "lock".to_string(),
        artifact_digest: "artifact".to_string(),
        sbom_digest: "sbom".to_string(),
        signer_identity: "signer\"quoted".to_string(),
        oidc_subject: "subject".to_string(),
        jankurai_release_witness: RELEASE_WITNESS_MARKER.to_string(),
        created_at_epoch: 7,
    };
    let json = statement.to_json();
    assert!(json.contains("\"signer_identity\":\"signer\\\"quoted\""));
    assert!(json.contains("\"created_at_epoch\":7"));
}

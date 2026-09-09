use super::*;

#[test]
fn cli_checksum_and_sbom_commands_are_deterministic() {
    let path = temp_artifact("cli.bin", b"cli artifact bytes");
    let path_str = path.display().to_string();
    let checksum = jeryu_signrail::cli::run_from(["checksum", path_str.as_str()])
        .unwrap_or_else(|err| panic!("checksum failed: {err}"));
    assert!(checksum.ends_with(&format!("  {path_str}")));
    let sbom = jeryu_signrail::cli::run_from(["sbom", "v9.0.0", path_str.as_str()])
        .unwrap_or_else(|err| panic!("sbom failed: {err}"));
    let file_name = path
        .file_name()
        .and_then(|part| part.to_str())
        .unwrap_or_else(|| panic!("temp path has file name"));
    assert!(sbom.contains("\"format\":\"CycloneDX-compatible\""));
    assert!(sbom.contains("\"version\":\"v9.0.0\""));
    assert!(sbom.contains(&format!("\"name\":\"{file_name}\"")));
}

#[test]
fn cli_rejects_unknown_command_with_helpful_usage() {
    let err = jeryu_signrail::cli::run_from(["bogus"]).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("unknown command bogus"));
    assert!(message.contains("checksum <path>"));
}

#[test]
fn cli_sign_release_requires_local_ed25519_seed() {
    let root = temp_store_root("missing-seed");
    let artifact = temp_artifact("missing-seed-bundle.tar.gz", b"bundle");
    let err = jeryu_signrail::cli::run_from_with_env(
        vec![
            "sign-release".to_string(),
            "--artifact".to_string(),
            artifact.display().to_string(),
            "--repo".to_string(),
            "neverhuman/veox-shared".to_string(),
            "--sha".to_string(),
            "abc123".to_string(),
            "--version".to_string(),
            "abc123".to_string(),
            "--rollback-target".to_string(),
            "abc122".to_string(),
            "--store-root".to_string(),
            root.display().to_string(),
        ],
        |_| None,
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("JERYU_SIGNRAIL_ED25519_SEED is required")
    );
}

#[test]
fn cli_sign_release_writes_signed_outputs_and_stage_receipts() {
    let root = temp_store_root("signed-cli");
    let out_dir = root.join("out");
    let artifact = temp_artifact("signed-bundle.tar.gz", b"signed bundle bytes");
    let seed = "42".repeat(32);
    let output = jeryu_signrail::cli::run_from_with_env(
        vec![
            "sign-release".to_string(),
            "--artifact".to_string(),
            artifact.display().to_string(),
            "--repo".to_string(),
            "neverhuman/veox-shared".to_string(),
            "--sha".to_string(),
            "abc123".to_string(),
            "--tree-sha".to_string(),
            "tree123".to_string(),
            "--version".to_string(),
            "abc123".to_string(),
            "--rollback-target".to_string(),
            "abc122".to_string(),
            "--test-status".to_string(),
            "ci-and-artifact-support-passed".to_string(),
            "--store-root".to_string(),
            root.display().to_string(),
            "--out-dir".to_string(),
            out_dir.display().to_string(),
            "--created-at-epoch".to_string(),
            "100".to_string(),
            "--ci-ir-hash".to_string(),
            "sha256:manifest".to_string(),
            "--runner-rootfs-digest".to_string(),
            "sha256:runner".to_string(),
            "--toolchain-digest".to_string(),
            "sha256:toolchain".to_string(),
            "--cargo-lock-digest".to_string(),
            "sha256:cargo-lock".to_string(),
        ],
        |key| match key {
            "JERYU_SIGNRAIL_ED25519_SEED" => Some(seed.clone()),
            _ => None,
        },
    )
    .unwrap_or_else(|err| panic!("sign-release failed: {err}"));

    assert!(output.contains("\"signature_coverage_percent\":100"));
    assert!(output.contains("\"signer_key_id\":\"signrail-ed25519:"));
    for file in [
        "release.json",
        "sbom.json",
        "provenance.json",
        "witness.json",
    ] {
        assert!(
            out_dir.join(file).is_file(),
            "expected {} to exist",
            out_dir.join(file).display()
        );
    }
    for stage in ["local", "dev-canary", "prod"] {
        let receipt_path = out_dir.join("stage-receipts").join(format!("{stage}.json"));
        let receipt = fs::read_to_string(&receipt_path)
            .unwrap_or_else(|err| panic!("read {} failed: {err}", receipt_path.display()));
        assert!(receipt.contains(&format!("\"stage\":\"{stage}\"")));
        assert!(receipt.contains("\"sha\":\"abc123\""));
        assert!(receipt.contains("\"rollback_target\":\"abc122\""));
        assert!(receipt.contains("\"signature_coverage_percent\":100"));
        assert!(receipt.contains("\"test_status\":\"ci-and-artifact-support-passed\""));
    }
    let release = fs::read_to_string(out_dir.join("release.json"))
        .unwrap_or_else(|err| panic!("read release failed: {err}"));
    assert!(release.contains("\"algorithm\":\"JFSIG-ED25519\""));
    assert!(release.contains("\"rollback\":{"));
    assert!(root.join("artifacts").is_dir());
    assert!(root.join("releases").is_dir());
    assert!(root.join("witnesses").is_dir());
}

#[test]
fn cli_verify_release_checks_stage_receipt_and_public_key() {
    let root = temp_store_root("verify-cli");
    let out_dir = root.join("out");
    let artifact = temp_artifact("verify-bundle.tar.gz", b"verify bundle bytes");
    let seed = "24".repeat(32);
    jeryu_signrail::cli::run_from_with_env(
        vec![
            "sign-release".to_string(),
            "--artifact".to_string(),
            artifact.display().to_string(),
            "--repo".to_string(),
            "neverhuman/veox-shared".to_string(),
            "--sha".to_string(),
            "abc123".to_string(),
            "--version".to_string(),
            "abc123".to_string(),
            "--rollback-target".to_string(),
            "abc122".to_string(),
            "--store-root".to_string(),
            root.display().to_string(),
            "--out-dir".to_string(),
            out_dir.display().to_string(),
            "--created-at-epoch".to_string(),
            "100".to_string(),
        ],
        |key| match key {
            "JERYU_SIGNRAIL_ED25519_SEED" => Some(seed.clone()),
            _ => None,
        },
    )
    .unwrap_or_else(|err| panic!("sign-release failed: {err}"));

    let signer = jeryu_signrail::Ed25519Signer::from_seed_hex(None, &seed)
        .unwrap_or_else(|err| panic!("signer failed: {err}"));
    let pubkey = root.join("signrail.pub");
    fs::write(&pubkey, signer.public_key_hex())
        .unwrap_or_else(|err| panic!("write pubkey failed: {err}"));
    let verified = jeryu_signrail::cli::run_from([
        "verify-release",
        "--release",
        out_dir.join("release.json").to_str().unwrap(),
        "--stage",
        "prod",
        "--store-root",
        root.to_str().unwrap(),
        "--pubkey-file",
        pubkey.to_str().unwrap(),
        "--json",
    ])
    .unwrap_or_else(|err| panic!("verify-release failed: {err}"));
    assert!(verified.contains("\"release_id\":\"neverhuman/veox-shared@abc123\""));
    assert!(verified.contains("\"stage\":\"prod\""));
    assert!(verified.contains("\"signature_coverage_percent\":100"));
}

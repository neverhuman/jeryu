use super::*;

pub(super) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(1)
}

pub(super) fn temp_artifact(name: &str, contents: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "jeryu_signrail-{name}-{}-{}",
        std::process::id(),
        now()
    ));
    fs::write(&path, contents).unwrap_or_else(|err| panic!("failed to write temp artifact: {err}"));
    path
}

pub(super) fn temp_store_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "jeryu_signrail-store-{name}-{}-{}",
        std::process::id(),
        now()
    ))
}

pub(super) fn identity() -> OidcJobIdentity {
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

pub(super) fn policy() -> ReleasePolicy {
    ReleasePolicy::strict(
        "https://git.example.invalid/acme/jeryu",
        "https://jeryu.example.invalid",
        "jeryu_signrail",
        now(),
    )
}

pub(super) fn unsigned_release() -> Release {
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

pub(super) fn signed_release() -> (Release, HmacSha256Signer) {
    let mut release = unsigned_release();
    let signer = HmacSha256Signer::new("phase8-test-key", b"phase8-secret");
    release
        .sign_with(&signer, now())
        .unwrap_or_else(|err| panic!("signing failed: {err}"));
    (release, signer)
}

# SignRail Threat Model

## Assets protected

- Release artifacts and checksums.
- SBOM documents.
- Provenance statements.
- Signing keys and signer identity.
- OIDC job identity.
- Release witness receipts.
- Rollback instructions.

## Primary threats

1. Unsigned artifact promoted as release.
2. Artifact swapped after provenance creation.
3. SBOM omitted or mismatched.
4. Mutable `latest` asset used as the only release asset.
5. Release built from the wrong source SHA.
6. Signing service outage silently bypassed.
7. OIDC identity minted for the wrong audience or issuer.
8. Rollback metadata omitted during an incident.

## Phase 8 controls

- Content-addressed artifact digests.
- SBOM digest included in every provenance statement.
- Source repository, commit SHA, tree SHA, CI IR hash, runner class, toolchain digest, Cargo.lock digest, runner rootfs digest, artifact digest, and signer identity included in provenance.
- Signature verification before release witness generation.
- Fail-closed policy engine.
- Immutable release enforcement.
- OIDC issuer/audience/expiry checks.
- Required rollback metadata.

## Production signer note

The included `HmacSha256Signer` is a deterministic local signer for development and tests. Production deployments should implement the `Signer` trait with a KMS, HSM, Sigstore, or equivalent signing backend while preserving the same fail-closed policy contract.

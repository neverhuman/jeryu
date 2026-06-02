# SignRail Release Signing

`jeryu-signrail sign-release` signs an artifact-support evidence bundle with an
Ed25519 seed, verifies 100% signature coverage, and writes the release, SBOM,
provenance, witness, and per-stage receipts.

Local runs must provide `JERYU_SIGNRAIL_ED25519_SEED`. GitHub Actions runs must
provide `SIGNRAIL_ED25519_SEED`. The command fails closed when the required seed
is absent.

Required inputs:

- `--artifact`: artifact-support bundle to sign.
- `--repo`: repository slug such as `neverhuman/veox-shared`.
- `--sha`: commit SHA covered by the bundle.
- `--version`: release evidence version, usually the same SHA.
- `--rollback-target`: commit or release target used for rollback evidence.
- `--store-root`: SignRail artifact store, locally `~/.local/share/jeryu/signrail`.
- `--out-dir`: directory for `release.json`, `sbom.json`, `provenance.json`,
  `witness.json`, and `stage-receipts/*.json`.

Expected stage receipts are `local`, `dev-canary`, and `prod`. Each receipt
records the commit SHA, artifact digest, rollback target, signer key id,
signature coverage, and test status. The split artifact-support workflow uploads
the signed evidence only after normal `ci` has passed on `main`.

Validation:

```bash
cargo test -p jeryu-signrail --test release_witness
cargo clippy -p jeryu-signrail --all-targets -- -D warnings
```

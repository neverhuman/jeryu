# Release

`VERSION` is the version source for this split repository. `CHANGELOG.md`
records user-visible changes. Releases land through PRs with green required
checks and the Jankurai score at or above the policy floor.

## Release Command Policy

Release evidence is produced locally before tagging:

```bash
rtk just ci-local
rtk just release-readiness
rtk just artifact-support
```

Release automation must not bypass the split-family PR path. The release
receipt must name the commit SHA, Work contract drift result, migration drift
result, security evidence path, artifact-support evidence path, and rollback
target.

Release automation entrypoint: `bash ops/ci/release.sh`. It verifies `VERSION`,
`CHANGELOG.md`, release and rollback docs, and security evidence, then writes
`target/jankurai/release-readiness.json`.

## Release Structure

- Version source: `VERSION`.
- Changelog source: `CHANGELOG.md`.
- Release process doc: `docs/release.md`.
- Rollback guidance: `docs/rollback.md`.
- CI or script evidence: `target/jankurai/`, `target/security/`, and
  `target/artifact-support/`.
- Integrity/provenance evidence:
  `target/jankurai/security/Cargo.lock.sha256` and artifact-support output.

## Checklist

1. Run `rtk just ci-local` from `jeryu-jira`.
2. Confirm `contracts/generated/` is byte-identical to `ts-rs` output.
3. Confirm DB migration tests pass against a fresh SQLite file.
4. Copy generated Work contracts into `jeryu-web/contracts/generated/`.
5. Validate deploy and web integration lanes that consume Work APIs.

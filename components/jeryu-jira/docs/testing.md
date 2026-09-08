# Testing

Primary local lanes:

- `rtk just fast`: formatting plus Rust type/build check.
- `rtk just check`: Rust unit tests and script syntax checks.
- `rtk just property-tests`: property tests for Work invariants.
- `rtk just migration-drift`: SQLite schema, constraints, and migration
  idempotence.
- `rtk just contract-drift`: generated TypeScript file set and bytes.
- `rtk just score`: pinned Jankurai audit lane.
- `rtk just security`: required secret scan, dependency audit, workflow lint,
  and lockfile provenance evidence through `tools/security-lane.sh`; absence
  of any required tool fails closed.
- `rtk just ci-local`: local parity wrapper for hosted lanes.

On a clean committed topic derived from hosted `origin/main`, run
`rtk bash ops/ci/proof_evidence.sh`. It executes the changed-surface proof
plan, validates every receipt, runs the Jankurai security and Rust evidence
lanes, and ratchets the candidate against an automatically removed no-local
clone of protected main. It refuses a dirty tree, an empty change, another
origin, or a score below 91.

The pre-push hook at `ops/git-hooks/pre-push` runs
`bash ops/ci/quality-gates.sh`. Enable it locally with:

```bash
git config core.hooksPath ops/git-hooks
```

## Repair Evidence

Typed Work errors expose a stable `code` plus their purpose, reason, common
fixes (`common_fixes` in JSON), `docs_url`, and `repair_hint` through the public,
serializable `WorkRepairHint` returned by `WorkError::repair_hint()`. Machine
receipts must serialize that static hint, never `WorkError::to_string()`, because
storage and row-decoding errors can contain backend details. Failed lanes should
print the rerun command and preserve local artifacts under `target/jankurai/`
when an artifact exists.

## Launch Gates

Release readiness requires artifact-backed evidence for:

- security: `target/jankurai/security/evidence.json`
- backups: SQLite backup path recorded before any Work schema-changing release
- monitoring: `target/jankurai/release-readiness.json`
- rollback: `docs/rollback.md` and the previous split-family tag
- abuse controls: Work validation, DB constraints, and bridge repair evidence

`rtk just release-readiness` verifies these control documents and writes the
release-readiness receipt consumed by artifact support.

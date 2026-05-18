# Example release evidence directory

This directory is a **template fixture**. It documents the schema described in
`docs/release-policy.md` § "Evidence directory contract" so future versions
have a copy-and-fill starting point.

Do **not** treat this as real evidence. Real versions live at
`ops/releases/<version>/` (no `.example` suffix).

Required files (skeletons below):

```
release-plan.md
release-attempt.json
vti-plan.json
proof-receipts.jsonl
release-doctor.json
preflight.json
security-evidence.json
sbom.cdx.json
attestations.json
canary-report.json
install-smoke.json
rollback-target.json
changelog.md
```

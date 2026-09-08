# jeryu-web

Vite/React/TypeScript app, rendered UX QA, and generated contract mirror.

This repository was seeded from Jeryu source commit `cbecf7caa0e932c76a341b2521e66e911233860d` by
`ops/split/materialize.py`. It is part of the seven-repo Jeryu split family and keeps source
paths stable where practical so ownership remains auditable.

## Docs

Durable guidance lives in `docs/` and is routed from `AGENTS.md`:
`architecture.md`, `boundaries.md`, `generated-zones.md`, `testing.md`,
`release.md`, `release-plan.md`, `security-tool-matrix.md`, and
`audit-rubric.md`. The `db/` tree owns the dev/e2e fixture schema + seed
(`npm run fixtures:build`); the security lane is `tools/security-lane.sh`.

## Owned Cargo Packages

- none

## Source Coverage

- `apps/web/**`
- `ux-qa/**`
- `packages/ux-qa/**`
- `contracts/generated/**`
- `contracts/AGENTS.md`
- `package.json`
- `tsconfig.json`

## Local Commands

- `just fast`
- `just check`
- `just score`
- `just security`
- `just artifact-support`

## Governed auditor

CI invokes only the receipt-verified `/home/ubuntu/.jeryu/bin/jankurai` identity
rendered by `jeryu-tool`. The 1.6.11 auditor cutover is CI authority only; it
does not change this repository's product version, release tag, or artifacts.

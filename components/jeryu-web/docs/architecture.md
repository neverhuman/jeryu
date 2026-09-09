# Architecture

`jeryu-web` is part of the Jeryu split family.

The public portal is `neverhuman/jeryu`. Release authority remains
`neverhuman/jeryu-deploy`; split member repositories own bounded product
surfaces and consume sibling crates from pinned public Git tags.

## Boundaries

- Profile: `node-frontend`
- Required check: `jeryu-web/required`
- Local release source of truth: `agent/boundaries.toml`

## Owned Surface

- `apps/web/**`
- `ux-qa/**`
- `packages/ux-qa/**`
- `contracts/generated/**`
- `contracts/AGENTS.md`
- `package.json`
- `tsconfig.json`

## Docs

- `boundaries.md` — owned seams and the `agent/boundaries.toml` manifest.
- `generated-zones.md` — the `contracts/generated/**` mirror rules.
- `testing.md` — proof lanes and the `jeryu-web/required` check.
- `release.md` — the release control surface and rollback guidance.
- `audit-rubric.md` — how the jankurai audit scores this repo.

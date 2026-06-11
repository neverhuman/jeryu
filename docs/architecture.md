# Architecture

`jeryu` is part of the Jeryu split family.

The public portal is `neverhuman/jeryu`. Release authority remains
`neverhuman/jeryu-deploy`; split member repositories own bounded product
surfaces and consume sibling crates from pinned public Git tags.

## Boundaries

- Profile: `public-portal`
- Required check: `jeryu/required`
- Local release source of truth: `agent/boundaries.toml`

## Owned Surface

- Portal and operational metadata only.

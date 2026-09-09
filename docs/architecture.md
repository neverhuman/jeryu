# Architecture

`jeryu` is part of the Jeryu split family.

The public portal source authority is
`https://git.neverhuman.org/git/jeryu/jeryu.git`. Release authority remains
`jeryu/jeryu-deploy` on the same hosted forge; split member repositories own
bounded product surfaces and consume sibling crates from pinned immutable Git
tags.

## Boundaries

- Profile: `public-portal`
- Required check: `jeryu/required`
- Repository boundary source of truth: `agent/boundaries.toml`

## Owned Surface

- Portal and operational metadata only.

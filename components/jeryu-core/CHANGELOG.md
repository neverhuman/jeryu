# Changelog

## Unreleased
- Make secret, workflow, advisory, license/source-policy, and CycloneDX SBOM
  checks mandatory in the full gate through one canonical security-lane
  implementation; refresh the three lock resolutions with current RustSec
  fixes and add a focused Core package loop.
- Make repeated repository-visibility updates true no-ops while preserving
  strictly increasing activity timestamps for actual visibility changes.
- Bind pull-request reviews and merge authority to the exact PR head, including
  authenticated head/base tree identities in the read contract.
- Repair release identity forward at `jeryu-core-v5.0.0-split.5`; earlier
  immutable v5 tags retain their historical committed `VERSION` bytes.
- v5.0.0 split baseline live on the local forge; merge-to-GitHub mirror verified.

## jeryu-core-v5.0.0-split.0 - 2026-06-11
- MAJOR: first standalone split-family release; the legacy monorepo
  (/home/ubuntu/jeryu) is deprecated and its drift fully reconciled.

## jeryu-core-v4.0.0-split.0

- Initial split-family baseline for `jeryu-core`.

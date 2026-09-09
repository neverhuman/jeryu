# Changelog

## Unreleased
- Repair the immutable release identity forward at
  `jeryu-web-v5.0.0-split.2`; earlier tags retain their historical `VERSION`
  bytes.
- Make the local CI surface dispatch exactly one named protected lane, reject
  hostile or ambiguous arguments before delegation, and expose the existing
  TypeScript contract test as a genuine `contract-drift` lane.
- Web: high-contrast multi-neon TUI overhaul — boot splash + moving feature
  carousel + keyboard-first login; the dark terminal theme is the new default
  (light and high-contrast still selectable); self-hosted JetBrains Mono.
- Docs: added `docs/boundaries.md`, `docs/generated-zones.md`,
  `docs/audit-rubric.md`, and `agent/standard-version.toml` for family parity.
- v5.0.0 split baseline retained as immutable history.

## jeryu-web-v5.0.0-split.0 - 2026-06-11
- MAJOR: first standalone split-family release; the legacy monorepo
  (/home/ubuntu/jeryu) is deprecated and its drift fully reconciled.

## jeryu-web-v4.0.0-split.0

- Initial split-family baseline for `jeryu-web`.

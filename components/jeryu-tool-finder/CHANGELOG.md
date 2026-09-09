# Changelog

Release notes for `jeryu-tool-finder`. Version source is `VERSION`; the split
tag it carries is the release coordinate for the family.

## Unreleased

### Changed (2026-09-03: hosted green catch-up)
- Forward-ported the preserved exact-source, contract, score, security, and
  artifact-proof train onto current protected main; refreshed the governed
  Jankurai 1.6.11 wrapper from hosted Tool authority.
- Restored `summary` against `jeryu-tool`'s current Rust-backed registry wrapper,
  rejecting symlinked or multiply linked entrypoints and preserving argument
  and exit-status propagation.
- Made Cargo deny and ShellCheck mandatory, removed Python from Tool Finder's
  own score/test gate, and identified `git.neverhuman.org` as repository and
  release transport without changing the stable Cargo dependency URL identity.

### Changed (2026-08-16: product proof lanes)
- Added a tracked Rust CLI help contract and standalone `contract-drift` lane.
- Added exact-head score/security sidecars and an artifact-support receipt that
  reports `ready` only for a digest-bound release CLI, truthful VERSION/Cargo/
  `--version` identity, governed auditor and build tools, fresh private build,
  clean Cargo audit, and valid SPDX document. Evidence now rejects ambient Git,
  hidden-index, ignored-input, replacement/alternate-object, and build-override
  substitution.
- Removed ambient sibling path patches and pinned `jeryu-codegraph` to immutable
  source coordinate `jeryu-intelligence-v5.0.0-split.1`, transported through
  the governed hosted rewrite.

### Changed (2026-06-12: all-Rust rewrite, system-wide scope)
- The whole pipeline is now a single Rust binary (`src/`,
  `cargo run -- scan|dossier|propose|summary`); the Python scripts are gone.
  The jeryu-codegraph engine is consumed AS A LIBRARY (pinned git tag +
  `[patch]` to the local split checkout) — one implementation shared with the
  live `/tools` dashboard on :8787 and MCP, so cluster ids and LOC numbers
  always agree.
- `scan --system` covers EVERY split family on the host: manifest-sibling
  discovery with logical-repo dedupe (a second checkout of the same repo can
  no longer manufacture fake cross-repo duplication), `git ls-files`-aware
  file discovery, a corpus-scale top-dir guard, window quality filters,
  overlap merging into maximal spans, categories
  (tool-candidate/managed-scaffold/config-pattern/test-pattern), and pattern
  families. Measured on this host: 58 repos / ~13k product files in ~5 s CLI
  (141k corpus files honestly skipped and counted).
- Typed exception surface (`src/errors.rs`): operator-facing failures carry
  purpose/reason/common fixes/docs/repair_hint.
- Property tests (proptest) over the propose invariants plus an end-to-end
  scan→dossier→propose pipeline test; `cargo fmt/clippy/test` ride
  `ops/ci/check.sh`. Dependency review via `deny.toml` + `just security-deps`.
- Reviewed gate recalibrated: floor 75 (clean full scan measures 76, 0 caps,
  0 hard findings under the family-pinned auditor; `ops/ci/lib.sh` now
  resolves `JERYU_JANKURAI_BIN` exactly like the Layer-2 scoring funnel).

### Added
- New family repo `jeryu-tool-finder`: the tool-discovery arm of the split.
- `scripts/scan_family.py` — drives `jeryu-codegraph tool-build scan-family`
  over `repos.manifest.toml` and writes `dossiers/clusters.json`.
- `scripts/dossier.py` — enriches each cross-repo cluster into an agent-readable
  dossier (per-repo file paths, normalized preview, anticipated LOC saved). Ships
  a hermetic `--selftest` over `fixtures/sample-clusters.json`.
- `scripts/propose.py` — promotes a chosen cluster into a `jeryu-tool` proposal
  (`[[tool]]` + `tasks/NNNN-*.toml`). Idempotent on `origin_cluster`.
- `scripts/registry_summary.py` — reads the golden-box registry summary from
  `jeryu-tool`.
- CI gate (`ops/ci/check.sh`, `ops/ci/score.sh`, `ops/ci/security.sh`) plus the
  local-parity surface (`scripts/ci-local.sh`, `scripts/ci-doctor.sh`,
  `ops/git-hooks/pre-push`).
- Agent-readable docs: `docs/architecture.md`, `docs/testing.md`,
  `docs/release.md`, `docs/tool-finder.md`, routed from `AGENTS.md`.

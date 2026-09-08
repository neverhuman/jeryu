# ops/ Agent Instructions

This cell owns the **CI gate** and the **local-parity surface** for
`jeryu-tool-finder`.

## Owns
- `ops/ci/*.sh` — deterministic lanes: `check.sh` (locked/offline Rust proof),
  `score.sh` (pinned jankurai audit + exact-head sidecar), `security.sh`
  (source/dependency/SBOM evidence), `contract-drift.sh` (tracked CLI help),
  `artifact-support.sh` (compiled CLI and evidence binding), and shared guards.
- `ops/git-hooks/pre-push` — the mandatory local gate (wire once with
  `git config core.hooksPath ops/git-hooks`).
- `.github/workflows/ci.yml` — a non-authoritative parity artifact. Protected
  hosted `jeryu-tool-finder/required` remains release authority.

## Forbidden
- No `fast`/pin-drift lane here — the jankurai pin is owned by `jeryu-tool`, not
  this repo. Do not add a local pin source.
- Do not weaken a lane to make the gate pass (no skipping the selftest, no
  lowering the floor to hide a cap). Caps must reach zero on their own.
- Do not add a second finder implementation or a relative Cargo path patch.
  The one `jeryu-codegraph` dependency must resolve from its immutable tag.

## Proof lane
- Edits under `ops/` are re-verified by the focused mapped lane and `just`.
  Run `bash scripts/ci-local.sh required` before pushing;
  `scripts/ci-doctor.sh` confirms the required tooling is present. See
  `../docs/testing.md`.

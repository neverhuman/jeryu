# ops

Cell guidance for the `ops/` reference-profile path. See `agent/owner-map.json` (`ops/`, `ops/ci/`, and `.github/` all map to `ops`).

## Owns

Operational artifacts that run jeryu in real environments — systemd units, CI workflow inputs, and platform glue. Today this is intentionally small:

- `ops/ci/jeryu-gc.service` and `ops/ci/jeryu-gc.timer` — systemd timer that drives the jeryu garbage-collection sweep on CI/operator hosts.
- `.github/workflows/` — GitHub Actions definitions (also owned by `ops` per `owner-map.json`); kept here in spirit, lives under `.github/` for tooling reasons.

The cell is a **scaffold for further ops growth** (deploy manifests, runbook fragments, observability config). Add new ops artifacts here, not under `src/` or `crates/`.

## Forbidden

- Embedding application logic in unit files; they must invoke the released `jeryu` binary, not reimplement it.
- Adding secrets or credentials in plain text — `.gitleaks.toml` is workspace-owned and any leak path must be cleared with security review.
- Modifying `.github/workflows/*` and `ops/ci/*` in the same change as a domain refactor; ops changes route through the security / workflow-lint lane and need separate proof.

## Proof lane

Workflow-lint and security lanes (per `proof-lanes.toml` and the `HLT-038` evidence `proof_lane=security lane / workflow lint`):

```
cargo deny check                                  # justfile: security
cargo run -p cargo-aer -- scan --output aer-findings.json
cargo test -p jeryu -- secrets exec honeypot admission   # lane.security
```

For systemd unit edits, also run `systemd-analyze verify ops/ci/jeryu-gc.service` on a Linux host before merge.

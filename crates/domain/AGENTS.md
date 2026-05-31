# Domain Agent Guidance

Owns:
- Typed domain error repair surface.
- Re-exports of `jeryu-core` domain errors for local audit routing.

Forbidden:
- Host-provider compatibility aliases.
- String-scraped error handling.
- Mutation paths without proof, receipt, or policy reason evidence.
- API compatibility errors that omit the typed repair fields exposed by this
  crate.

Proof lane:
- `cargo test -p jeryu-domain --jobs 40`
- `cargo test -p jeryu-core --jobs 40`
- Named lane: `agent/proof-lanes.toml` entry `api` covers API boundary
  callers; this crate's own repair contract remains verified by
  `jeryu-domain`.

Live-readiness note:
- When the typed repair surface changes, include this guidance file in the
  changed-fast audit so Jankurai can detect the local domain owner and proof
  lane.

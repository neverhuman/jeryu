# Domain Agent Guidance

Owns:
- Typed domain error repair surface.
- Re-exports of `jeryu-core` domain errors for local audit routing.

Forbidden:
- Host-provider compatibility aliases.
- String-scraped error handling.
- Mutation paths without proof, receipt, or policy reason evidence.

Proof lane:
- `cargo test -p jeryu-domain --jobs 40`
- `cargo test -p jeryu-core --jobs 40`

Live-readiness note:
- When the typed repair surface changes, include this guidance file in the
  changed-fast audit so Jankurai can detect the local domain owner and proof
  lane.

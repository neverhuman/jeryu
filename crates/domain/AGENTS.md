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

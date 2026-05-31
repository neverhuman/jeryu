# jeryu-api Agent Guidance

Owns:
- GitHub-compatible REST response shapes.
- Guided GraphQL repair responses.
- Local Axum web/API edge under the `web` feature.

Forbidden:
- Broad GraphQL execution without a narrow conformance test.
- Provider-source fixtures or copied external API specs.
- String-only errors without `documentation_url` or `jeryu_repair_hint` for
  guided compatibility gaps.

Proof lane:
- `cargo test -p jeryu-api --features web --jobs 40`
- `cargo clippy -p jeryu-api --features web --all-targets --jobs 40 -- -D warnings`

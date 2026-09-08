# Contracts Agent Instructions

Owns the generated TypeScript contract surface under `contracts/generated/` and
the Rust source types that produce it.

Allowed edits:
- Regenerate files only from the Rust source contract command recorded in
  `contracts/generated/README.md`.
- Update this guidance and contract docs when ownership or proof routing changes.

Forbidden edits:
- Do not hand-edit generated TypeScript contract files.
- Do not change web-facing wire names without the Rust source type, drift test,
  and generated output in the same change.
- Do not introduce direct data access or runtime persistence in this directory.

Proof lane:
- `cargo test -p jeryu-readmodel --jobs 40 && cd apps/web && npm run typecheck`


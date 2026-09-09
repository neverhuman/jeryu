# Boundaries

`jeryu-web` owns the JeRyu Web Forge SPA and its rendered UX QA. The
machine-readable boundary manifest is `agent/boundaries.toml`.

## Owned surface

- `apps/web/**` — the Vite + React + TypeScript SPA (UI, hooks, stores, styles).
- `ux-qa/**`, `packages/ux-qa/**` — the rendered UX QA lane.
- `contracts/generated/**` — the TypeScript contract mirror (generated; see
  `docs/generated-zones.md`).
- `contracts/AGENTS.md`, `package.json`, `tsconfig.json`.

## Forbidden

- No hand-edits to `contracts/generated/**`; those types are Rust-generated in
  the owning sibling and land with the source type, drift test, and generated
  output in one change.
- No durable product truth stored in the web layer; truth lives behind the
  sibling Rust boundaries and is consumed over typed contracts.

## Cross-boundary rule

The SPA reaches the forge only through the `/api/v1` REST surface and the
realtime WebSocket. Wire names come from generated contracts; the `tsd`
contract-drift lane guards them before merge.

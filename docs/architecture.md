# Architecture

Jeryu's candidate source lives in one public monorepo at
[neverhuman/jeryu](https://github.com/neverhuman/jeryu). Ten directories under
`components/` define ownership and export boundaries. The root Cargo workspace,
lockfile, toolchain and configuration own the Rust build; the root npm workspace
and lockfile own the browser and UX build.

## Application and storage

The Deploy component owns the `jeryu` CLI and server. It composes Core's domain
and Git/storage interfaces, Work, Cache, Intelligence and the supporting
libraries. Web supplies production assets embedded into the source-built
binary. An explicit `--spa-dir` selects development assets.

Standalone runtime state lives in an explicit data directory selected by the
CLI, environment or XDG rules in [README.md](../README.md). SQLite is bundled
and durable; Git repositories persist alongside application data. Source trees
and temporary in-memory state are not substitutes for that runtime store.

Runner source remains in the workspace, but installing a runner is optional.
Native sandbox, OCI isolation and the actual agent image require distinct
proofs. A probe image does not qualify the product image.

## Dependencies and contracts

Internal Jeryu dependencies resolve through the root workspace, preserving one
package identity. External owned source uses public immutable revisions with
verified identity. Ordinary installation uses application dependencies; auditor
and optional agent acquisition belong to their explicit verification paths.

Generated browser contracts come from their owning Rust packages. The root
build configuration owns generated component compatibility projections.
After changing it, run:

```bash
cargo run --locked -p jeryu-split-tool --bin jeryu-split -- build-config --write
```

RedlineDB is excluded from the product graph. Its optional harness under
`components/jeryu-release-ops/tests/redline` has its own manifest and lockfile.
Canonical Jain Redline source and its two-consumer proof govern compatibility
and duplicate retirement; they do not block SQLite qualification.

## Source and release authority

The root `repos.manifest.toml` describes a pending protected handover. Actual
release authority remains the original `jeryu-release-ops/repos.manifest.toml`
until that handover qualifies. Family identity remains `jeryu-split`, with the
existing v5 tag lineage and component technical identities.

Component repositories are downstream mirror targets. Deterministic exports
bind their component tree and originating monorepo commit; standalone build,
test and audit evidence must identify the actual exported source. See
[split publication](migration/SPLIT-PUBLICATION.md).

Keep the existing canonical checkout at
`/home/ubuntu/jain-split/jeryu-split/jeryu` during this transition. Public clone
locations remain user-selected. Physical relocation, installed-service
maintenance and release activation are separate from source development.

The inherited JainHub composition program remains a separate installed-runtime
boundary. Jeryu libraries must not depend on JainHub or `jain-web`. This
standalone source candidate does not claim the existing service, browser,
runner or SQLite transition has been retired.

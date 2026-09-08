# Architecture

`jeryu-tool` is the Jeryu family tool control plane. It owns the governed
Jankurai identity and the reusable-tool registry; it does not own product
runtime behavior or durable product data.

## Authority chain

`tool-manifest.toml` is the authored source of truth for the Jankurai source
tag, commit, tree, hermetic build inputs, binary digest, and installation
contract. Authority changes land on protected `jeryu-tool/main` through the
reviewed lifecycle. Source tags and release tags are immutable.

`jeryu-toolctl render-tool-manifest` derives `generated/jankurai-pin.env` and
the declared family consumer blocks from that manifest. Check mode is
read-only. Write mode accepts only canonical checkouts, exact expected heads,
and protected-main authority read back from the hosted forge. Generated blocks
are never independent sources of truth.

The governed installer builds from the immutable source with the closed vendor
inventory and network-disabled builder. It verifies every declared digest,
publishes the binary atomically, preserves the predecessor as a content-addressed
rollback artifact, and writes a path-bound installation receipt.

## Components and ownership

- `crates/jeryu-tool-control/` owns manifest parsing, renderer custody, typed
  repair errors, registry validation, and their Rust tests.
- `tool-manifest.toml` and `generated/jankurai-pin.env` own authored and rendered
  Jankurai identity respectively.
- `ops/` owns hermetic build, install, root-seal, renderer, and CI entrypoints.
- `tools-registry.toml` and `tasks/` own reusable-tool inventory and queued work.
- `agent/` owns proof routing, boundaries, ownership, and audit policy.
- `docs/` owns operator-readable architecture, testing, registry, and release
  contracts.

Product enforcement remains in the consuming repositories. Forge enforcement
lives in `jeryu-core` and `jeryu-deploy`; release guards live in
`jeryu-release-ops`; sandbox enforcement lives in `jeryu-ci-runner`.

## Trust boundaries

`https://git.neverhuman.org` is the repository source and review authority. Git
reads use a fixed executable and the exact
`https://git.neverhuman.org/git/jeryu/<repo>.git` identity; loopback transition
remotes and URL variants cannot satisfy custody.
Credentials are read from an explicit absolute, mode-0600, owner-held,
single-link file. The same-inode, process-held askpass child receives only that
path, so PAT bytes are never placed in process arguments or environment
variables.

Canonical checkouts are single-writer and may not have registered worktrees.
Exact-head CI uses an automatically removed `clone --no-local` sandbox, a
detached published SHA, and a protected `origin/main` ref bound to live forge
readback. A local `main`, sibling checkout, symlink, copied repository, or shared
compiled target is not authority.

Installation receipts bind source identity, protected manifest identity, binary
digest, and physical installation path. A matching digest at a different path
does not satisfy that contract. Runtime consumers must fail closed on missing,
mixed, or stale identity.

## Durable truth and generated zones

The manifest, registry, task files, policies, source history, immutable tags,
and content-addressed receipts are durable truth. `target/`, temporary clones,
score output, and build caches are disposable evidence or acceleration only.
No test or release may depend on a compile-time path retained from a deleted
sandbox.

This control plane owns no database, migration stream, paid API, or durable
product record. Its data inputs are the reviewed TOML manifests, task files,
policies, and immutable Git identities named above. Writes are limited to
generated consumers and content-addressed evidence under explicit locks; a
failed write is stopped and retried from authenticated source bytes. Installer
rollback custody is an artifact boundary, not a second data store. Diagnostic
jobs are resource-bounded and have no billable network call or public service
kill switch because no such runtime exists here.

Generated pin blocks carry explicit begin/end markers. Change the manifest and
run the renderer; do not hand-edit generated consumers. Renderer drift is a
failing `just fast` result, not a reason to broaden custody or bypass a consumer.

## Proof routing and failure model

`just fast` proves manifest and generated-consumer agreement. `just fast-proof`
and `just fast-test` provide bounded, locked package-only feedback using the
governed compiler cache. `just check`
proves custody, schemas, tests, warnings-denied Clippy, and shell entrypoints.
`just tool-adoption` produces changed-surface, security, coverage, contract,
witness, duplication, and ratchet evidence before `just score` consumes it.
`just security` runs repository security checks and creates a source SBOM. The
complete merge-blocking contract is `just required` (also the default `just`).

Control failures expose `purpose`, `reason`, common fixes, `docs_url`, and
`repair_hint`. Missing authority, stale refs, dirty state, malformed receipts,
ambiguous forge mutations, and unavailable offline inputs all stop the lane.
Operators read back machine state before retrying; they do not fabricate proof
or weaken policy.

# Auxiliary producer commands

These preliminary commands run actual independent producers for a clean
monorepo candidate. They are callable locally through the root CI entrypoint;
they are not yet included in `all` or the hosted matrix.

```bash
bash scripts/ci.sh auxiliary copy-code jeryu-core
bash scripts/ci.sh auxiliary migration jeryu-web
bash scripts/ci.sh auxiliary rust-workspace
bash scripts/ci.sh auxiliary independent
```

The existing auditor bootstrap verifies the immutable public source, exact
binary/build pins and candidate installation receipt. Producers execute through
its retained descriptor. Source commit/tree, root Cargo manifest/lock/toolchain,
shared implementation blobs and auditor binary/receipt digests must match before
and after each step. The source must remain clean through the existing candidate
renderer. No protected predecessor or completed authority handover is claimed.

`copy-code` runs strict mode with the pinned default scan policy, including its
existing exclusions, and rejects hard classes or instances. `migration` runs
actual stack/migration analysis, including when a component has no SQL migration
files. It does not execute storage migrations or assert that migration is safe.
Each supports `all` or Core, Intelligence, Release Ops or Web by repository name.

`rust-workspace` runs one actual map and one actual witness over all 65 packages.
An additional locked, offline Cargo metadata read binds exact owning package
manifests and both directions of cross-component dependency edges. The graph uses
Cargo's default feature configuration, as does the pinned producer. All four
outputs must contain the same complete member set. Web's owned Rust set is empty;
the shared graph is not a Web-specific Rust proof. Cargo/rustc versions are
recorded. This lane needs the locked dependency metadata in the caller's Cargo
cache; missing offline dependencies fail. It is not a hermetic build receipt.
Jankurai's witness describes package `src` files; it is not a substitute for
the complete ordinary unit, integration, property, feature and documentation tests.

`independent` runs the shared graph once, then both owned producers for all four
components. Each existing `ops/ci/proof_evidence.sh` wrapper delegates only its
component's copy-code and migration producers; it does not rebuild the shared
graph. Nonzero producer exits are retained and make the selected union fail.
Source/identity failures stop further execution. Missing or malformed reports
cannot be replaced by empty JSON. No accepted baseline is written or replaced.

Each attempt has a fresh owner-only directory beneath `target/ci/auxiliary`.
Actual logs/reports, their hashes, step exits and source bindings remain there
on success or failure for review. This command performs no recursive deletion.
The result explicitly says `full_proof=false` and
`qualification=selected-producers-only`.

With no explicit producer, `full` runs the useful independent subset and returns
nonzero. Full proof still needs an authenticated protected predecessor and policy,
complete changed paths including deletion/rename/binary cases, source-bound hunks,
proofbind/proofmark execution and authenticated negative proofs, required auxiliary
UX/vibe/coverage configuration and governed producer conformance. Ordinary audit,
security, actual coverage/mutation, product UX, release and split qualification
remain dedicated gates. The pinned `rust diagnose` drops the Cargo exit status;
it is deliberately excluded from proof admission.

Standalone wrappers refuse until an authentic export adapter exists. The planned
projection reads `scripts/auxiliary-proofs.sh` and `scripts/auxiliary-rust.jq`
from the exact requested monorepo Git commit, records each original path/blob and
SHA256 plus destination path in split provenance, and includes those exact bytes
in the deterministic export tree. It must never read the exporter checkout's
current helper bytes or search adjacent repositories. The export-bound auditor
receipt must authenticate that source and component tree before execution; the
monorepo candidate receipt cannot be relabeled as a split receipt. Rust scope in
a split is its actual component workspace, with public immutable external edges,
and cannot claim the 65-package monorepo proof. Projection/source-adapter tests
and actual independent split execution remain required.

Run `bash tests/auxiliary-proofs.sh` for synthetic admission, report, complete
graph, dispatch and failure tests; it performs no auditor invocation or build.

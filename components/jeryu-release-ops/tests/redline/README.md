# Optional Redline compatibility contract

Jeryu builds and runs with bundled SQLite. This separate, unpublished test
harness preserves the existing Redline transaction, rollback, checkpoint and
reopen contract without adding Redline to the product dependency graph.
Its lockfile pins the public immutable tag and its four-package closure.

From the monorepo root, explicitly opt in with:

```bash
bash scripts/ci.sh redline
```

From a Release Ops split export, run:

```bash
cargo test --locked --manifest-path tests/redline/Cargo.toml
```

Failures return a nonzero exit status. This command is outside the SQLite
release matrix and does not select a runtime storage backend. The contract
covers a small SQL workload; it is not evidence of complete SQLite parity.
The product currently opens SQLite databases directly and has no Redline
runtime selector.

Cargo resolves optional workspace dependencies even when they are not built,
and `--all-features` enables them. This harness therefore has its own excluded
workspace and lockfile. All 65 product packages remain in the root workspace.
Do not add a Redline edge back to that workspace or its release lockfile.

The existing `ops/ci/redline-consumer.sh` producer separately requires reviewed
main, canonical authority and authenticated producer evidence before emitting
two-consumer proof. The candidate authority handover remains pending. Passing
this standalone contract does not satisfy those gates or authorize retirement
of any original Redline checkout.

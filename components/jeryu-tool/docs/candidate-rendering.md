# Monorepo candidate rendering

`render-monorepo-candidate` renders the monorepo's generated auditor consumers
and emits source provenance for a candidate whose protected authority handover is
pending. It does not authenticate a protected predecessor, build or install the
auditor, read credentials, or create a governed installation receipt.

From the monorepo root:

```sh
bash components/jeryu-tool/ops/render-monorepo-candidate.sh --check --expected-head "$(git rev-parse HEAD)"
bash components/jeryu-tool/ops/render-monorepo-candidate.sh --write --expected-head "$(git rev-parse HEAD)"
```

Both commands require a clean physical monorepo checkout with one ordinary `.git`
directory and unsuppressed index entries. Tool must be at `components/jeryu-tool`.
Write mode requires the exact reviewed HEAD. It writes only held, validated
generated consumers. Review and commit those changes, then rerun check mode at
the new commit before installation qualification. An interrupted write can leave
partial generated changes for inspection; a failing command emits no success
provenance. The renderer does not commit, reset, change refs, or alter the index.

Without a mode, the command checks. `--repo NAME` may repeat with distinct names
from the closed root/component inventory for a repair. Qualification requires
an unscoped check with `scope.complete=true`, all eleven sorted repository names,
`drift=false`, and no changed consumers. A scoped result cannot qualify the family.

The same Rust command can be invoked directly:

```sh
cargo run --locked --offline -p jeryu-tool-control --bin jeryu-toolctl -- \
  --tool-root /absolute/monorepo/components/jeryu-tool render-monorepo-candidate \
  --monorepo-root /absolute/monorepo --check --expected-head REVIEWED_COMMIT
```

Only schema 2 manifests with the validated immutable public distribution are
accepted here. All 26 producer/build fields retain their existing generated pin
format. The producer URL remains distinct from the public transport URL.
`render-tool-manifest` and its schema 1/protected lifecycle remain separate.

Candidate consumers include the root and ten component CI pin blocks, the
generated pin, canonical verifier/function and command-wrapper copies, workflow
pin blocks, and required-tool/version policy fields. Authoritative templates
remain authored under Tool; the renderer uses their committed bytes. It preserves
historical installation receipts, protected governance constants/tests, native
observed-evidence records, sandbox images and release documentation. Those need
their own reviewed integration and are not inferred from candidate rendering.

Stdout is one pretty-printed JSON object with exactly one trailing newline.
Check mode reports known drift with that JSON and exits 1; no drift exits 0.
Write mode reports the source before writing and the changes it generated.
Runtime HEAD/tree never enter tracked generated outputs, avoiding self-reference.

The closed output contract is:

```text
schema = jeryu.jankurai-candidate-render/v1
mode = check | write
scope = { repositories: sorted names, complete: boolean }
source = { repository, commit, tree, clean_at_start: true }
manifest = { path, blob, sha256 }
distribution = { repository, tag, commit, tree }
producer_repository = original [jankurai].repo
governance = { protected_main: false, handover: pending, predecessor_authentication: not-performed }
verification = { build: not-performed, installation: not-performed, public_readback: not-performed }
generated_pin_sha256 = SHA256 of the canonical generated pin bytes
consumers = [{ path, before_sha256, expected_sha256, changed }]
drift = boolean
```

`source.repository` identifies the intended public monorepo, not an authenticated
readback of the checkout's current transport. Commit/tree and manifest blob/digest
come from the selected Git objects with replacement objects disabled. The
distribution fields are expected immutable source identity; anonymous readback,
source acquisition, the exact binary rebuild and installation evidence remain
separate operations. The installer must recompute and bind this metadata at the
exact clean commit and retain its own inputs by descriptor.

Existing thin Tool wrappers now address the member Cargo manifest, which resolves
the correct workspace in both monorepo and split layouts. Root CI must explicitly
route candidate qualification to this command; a legacy protected check is not
silently reclassified. Candidate provenance does not rely on a previous candidate
receipt and carries no protected installation claim.

Focused tests run with the normal Tool package tests:

```sh
cargo test --locked --offline -p jeryu-tool-control candidate_
cargo clippy --locked --offline -p jeryu-tool-control --all-targets -- -D warnings
```

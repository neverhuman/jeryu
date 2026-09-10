# Exact source change inventory

`jeryu-toolctl inventory-proof-changes` records the complete change between two
exact ancestor commits in a clean Linux monorepo checkout. It requires no forge
credentials, private Jain configuration or Jankurai installation. It does not
run proofs or authenticate a protected predecessor.

Pass all four flags exactly once, with absolute physical paths and distinct
lowercase 40-character commit IDs. The candidate must be the checkout HEAD;
the base must exist locally and be its ancestor. The Tool root must be that
checkout's `components/jeryu-tool` directory.

```bash
mkdir -m 700 "$HOME/jeryu-proof-evidence"
cargo run --locked -p jeryu-tool-control --bin jeryu-toolctl -- \
  --tool-root "$PWD/components/jeryu-tool" inventory-proof-changes \
  --monorepo-root "$PWD" --base BASE_COMMIT --head CANDIDATE_COMMIT \
  --out "$HOME/jeryu-proof-evidence/inventory.json"
```

Replace both commit placeholders with exact IDs. The output must be new, inside
an existing owner-held mode0700 physical directory outside the checkout. The
command creates a mode0600 file through a held directory descriptor, rejects
existing files/links, and never deletes evidence. Before reporting success it
synchronizes the file and parent directory, rechecks the original parent inode,
owner, group and mode0700, and compares held/path identities for the regular
owner-held mode0600, single-link output. A failed operation may retain diagnostic
output; only a successful process exit admits this inventory result.

The result says `qualification=change-inventory-only`,
`predecessor_authentication=not-performed`, `routing=not-performed`,
`full_proof=false` and `protected_main=false`. A base commit being an ancestor
is not protected acceptance, an audit baseline, a review or permission to merge.

Both root Git trees and the original root owner map, test map, proof-lanes and
audit-policy blobs are recorded. Root inputs must exist as regular, parseable
JSON/TOML blobs at both commits. No component map is substituted and map
semantics, route completeness and proof execution remain subsequent admission.

Each changed entry retains original pathname bytes as lowercase hex, with an
optional lossless UTF-8 display value. It includes old/new modes, Git object IDs,
blob sizes and SHA256 hashes. Renames and copies retain both endpoints. A second
comparison of the complete trees rejects omitted, invented or duplicate changed
paths. Copy sources may also have an independent modification entry.

The fixed Git comparison uses 50% rename/copy detection with harder copies and
limit1000. A pair beyond Git's heuristic limit remains a complete deletion and
addition rather than disappearing. The actual Git version and diff policy are
recorded. Similarity is a heuristic association; it is not proof of behavior.

Regular UTF-8 blobs without NUL bytes receive old/new line spans from an explicit
Myers diff with zero context and no indent heuristic. Deleted lines remain on
the old side. Empty files, pure renames and mode-only changes do not acquire an
invented line1. A copy with identical content has empty content-change spans;
its new destination remains a changed path requiring subsequent admission.
Binary files, symlink target blobs and gitlinks receive explicit kinds and null
text spans. Symlinks are never followed, and gitlink objects are recorded without
opening or fetching their repositories. Null spans never mean a proof passed.

The command clears the environment for its object/diff reads, disables replacement
objects, external diffs and text conversion, and refuses grafts, alternates and
shallow history. It reuses the existing candidate physical repository, index
suppression and clean HEAD checks before and after inventory generation. It
places a 64MiB bound on each Git output/blob; exceeding it is an error, not a
partial inventory. Large comparisons may require an explicitly reviewed limit
change and a separate allocation.

Run the owning regression tests and warning-denied Clippy under an allocated CI
slot after committing the source:

```bash
cargo test --locked -p jeryu-tool-control --bin jeryu-toolctl inventory_
cargo clippy --locked -p jeryu-tool-control --all-targets -- -D warnings
bash tests/auxiliary-proofs.sh
```

The Rust tests construct ordinary disposable Git repositories from small authored
fixtures, exercise the real inventory implementation, and inspect physical root,
mount and expected symlink identities before successful cleanup. On a test
panic they retain the fixture and print its path before any cleanup; unexpected
root, mount or symlink identities also retain it. Failed fixtures require later
verified preservation and guarded retirement. The tests do not copy a source
checkout or create Git worktrees. Passing tests of this command does not qualify
`auxiliary full`, Jankurai output, protected review, source coverage, mutations,
negative behavior, the running service or a standalone split export.

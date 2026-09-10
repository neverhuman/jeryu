# Offline mirror-update preparation

`jeryu-split prepare-mirror-update` prepares an unreferenced commit object for
an **existing** component mirror. It does not contact a remote, fetch objects,
update a branch/tag, publish a check, open a PR, or qualify publication. Keep
the live-forge `mirror_github_main` relay disabled.

Run this only inside the existing reviewed, source-bound disposable Git
qualification environment. The caller owns process bounds, source custody,
retained objects and later preservation. The source repository must be a clean
physical ordinary SHA-1 checkout at the selected source commit; the mirror
must be a separate physical full bare SHA-1 repository. Shallow stores,
alternates, grafts, replacement refs and linked Git directories are refused.
This admission does not replace the outer filesystem/ownership/mount checks.

Inputs are explicit:

```text
jeryu-split prepare-mirror-update \
  --source-repo /absolute/admitted/source \
  --source FULL_SOURCE_COMMIT \
  --component jeryu-cache \
  --export-tree PREVIOUSLY_RESOLVED_EXPORT_TREE \
  --mirror /absolute/admitted/mirror.git \
  --expected-tip EXACT_EXISTING_MAIN_COMMIT \
  --expected-tags /absolute/retained/tags.txt
```

The expected tag snapshot is the exact output, including its trailing newline
(or empty bytes when there are no tags), of:

```text
git for-each-ref --sort=refname --format='%(objectname) %(refname)' refs/tags/
```

The caller obtains and preserves this snapshot together with the existing main
tip through a separate authorized readback. This command compares those local
inputs; it does not prove that they are the current public remote state. All
mirror refs, tags and main are read again before success. All source refs and
source cleanliness are also rechecked. A change observed by readback causes refusal;
an already written object remains harmlessly unreferenced, with no rollback
or deletion attempted.

The source object store must already contain the exact mirror parent commit.
This command performs no object transport. Any required local/public fetch
belongs in the existing separately admitted preparation workflow. No absent,
empty or new-repository initialization is supported here.

Use the tree from the maintained `export-tree --resolve-lock` stage. Its exact
`.jeryu-source.json` must bind the selected source/component/subtree, report no
remaining lock regeneration, and retain `publication_qualified:false`. The
seven generator/command source files and templates compiled into this binary
must match their committed source bytes; the result records their Git blob
IDs and the full monorepo source tree. This is a source-content binding, not a
compiler/binary attestation. The caller must still bind the actual executable
through its ordinary build qualification.

The resolved tree is an explicit input, not a newly verified build result.
This command does not rerun Cargo/npm resolution, prove that every tree byte
was generated correctly, or authenticate independent qualification receipts.
Those remain the export/build/CI stages. Both `qualification_verified` and
`publication_qualified` stay false in its output.

For a preexisting historical mirror without `.jeryu-source.json`, add
`--initial-tip EXACT_EXISTING_MAIN_COMMIT`. It must equal `--expected-tip`.
This explicitly requests a first generated transition while preserving that
history as the single parent; it does not mean the transition is approved.
A malformed existing descriptor is refused, never treated as absent.

For an already generated mirror, omit `--initial-tip`. The prior descriptor
must be consistent with its source's real component subtree. Its source must
be an ancestor of the next source. Identical source and identical exported
tree return the existing main commit with `no_op:true`; the same source with
different tree/provenance is refused. Initial preparation repeated before any
mirror update returns the same deterministic commit ID. Author/committer
identity, commit message and timestamps are fixed from the source input.

The candidate commit uses the resolved export tree and exactly the existing
mirror main as its parent. It adds originating source, source tree, component
and export-tree trailers. No monorepo parent is substituted for mirror history,
and no existing tag is edited. Preparation writes only an unreferenced commit
object to the source object store, then emits a JSON result.

After complete local qualification, the later publisher still needs current
public source availability, fresh remote tip/tag/protection readback, an
ordinary candidate-branch push, independent protected review and merger, and
final forward-only readback. That publisher is not implemented by this slice.
Local exact-commit preparation remains distinct from anonymous qualification.

The owning regression target is:

```text
cargo test --locked -p jeryu-split-tool --test mirror_update
```

Its eight tests use local synthetic Git repositories, the actual exporter and
the actual preparation CLI. They synthesize the resolved-lock descriptor for
the test input; they do not execute or attest package resolution, network or
remote publication. The fixture's local ref setup is deliberately separate
from the production command, whose unchanged-ref assertions are checked.

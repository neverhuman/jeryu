# Retired container-root Python controls

The non-repository bootstrap scripts formerly located at
`/home/ubuntu/jain-split/jeryu-split/ops/split/{reconcile.py,source_coverage.py,materialize.py,bump-family-version.py}`
were one-shot split-migration controls. A 2026-07-25 machine-wide call-site
census found no active service, timer, cron entry, CI lane, shell entrypoint,
or repository caller. Their hard-coded predecessor roots and v4-to-v5
one-shot mutations are superseded by the typed family authority and the
reviewed Rust split controls in `jeryu-deploy`.

Their exact bytes remain recoverable from the complete Git bundle
`/home/ubuntu/jain-split/.bundles/20260725-jeryu-container-retired-python-controls/jeryu-container-python-controls.bundle`
at preservation ref
`refs/preserve/jeryu-split-container/20260725/retired-python-controls`, commit
`831a55a5109635fbec31f171e0fa3ad8aa93429e`, tree
`b1cfc1d8f91a35764b999a46d61ca97555700ab3`. The mode-0444, single-link
bundle is 20,363 bytes with SHA-256
`ef9357283f748b65bb713d5bcf0f1204d714ca977734a8a18edec80ff517790b`.
Bundle verification reports complete history, and a detached standalone
restore reproduced all four starting SHA-256 identities byte-for-byte before
retirement.

This is preservation evidence, not executable release input. Restoring any of
these Python controls into the governed Jeryu family would violate the global
Rust-only source boundary and requires a new reviewed Rust implementation
instead.

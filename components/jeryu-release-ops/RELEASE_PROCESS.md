# Release process

`jeryu-release-ops` is a reviewed source member of the Jeryu split family;
`jeryu-deploy` remains the binary and production promotion authority. Run
`just release-readiness` before proposing a source tag or using this repository
as release evidence. That command binds the fast, check, score, security,
artifact-support, and Redline consumer contract lanes.

Source authority and release-ref readback use
`https://git.neverhuman.org/git/jeryu/<repo>.git`. The loopback forge profile is
non-authoritative and must not be selected for a new release. Cargo dependency
source strings are not rewritten during this cutover: stable source spelling
prevents duplicate crate identities, while governed Git CLI rewrites carry the
actual transport to the hosted forge.

The Redline compatibility producer is `ops/ci/redline-consumer.sh`. It may run
only from clean, forge-equal `main` after a fresh Redline family receipt has
verified every immutable family tag. The committed lock must resolve
`redline-core-v4.1.0-jain.6` to
`d0de59930141baffcfa2b514480e75b14627f24d`; the producer then executes the
transaction, rollback, checkpoint, and reopen contract and writes the evidence,
test log, and checksum sidecar together.

Evidence is never accepted from a review branch, a floating ref, a historical
TOML assertion, a waived consumer, or a manually declared check. This repository
does not push images, move tags, change routes, or mutate Jain production.

# Ops Agent Guidance

## Owns

This directory owns local CI wrappers, release-support scripts, and audit evidence lanes.
`ops/ci/redline-consumer.sh` is the sole producer for Jeryu's Redline consumer
evidence after its dependency pin has landed on reviewed, forge-equal `main`.

## Forbidden

Do not generate consumer evidence from a topic branch, a dirty worktree, a
floating dependency, or a family receipt whose checksum and manifest identity
do not match. Do not edit generated evidence or checksum sidecars by hand.

## Proof lanes

Keep hosted workflows thin: they should delegate to scripts under `ops/ci/` so
every gate can be reproduced locally. Changes to the Redline pin or producer
must pass `just redline-consumer-test`, `just release-readiness`, and the full
`ops/ci/pr-ci.sh` workspace gate before review.

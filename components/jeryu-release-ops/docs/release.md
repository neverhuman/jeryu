# Release

This split member publishes source changes through pinned tags; `jeryu-deploy` remains the binary release authority.

Version source is `VERSION` plus the split tag recorded in
`repos.manifest.toml` when present. Release notes are recorded in
`CHANGELOG.md`.

The authoritative repository and tag transport is `git.neverhuman.org` for
every active Jeryu split member. Loopback remains only a declared transition
profile and is not an eligible authority selector. Cargo Git source spellings
stay stable and are transported to the hosted forge through governed Git CLI
rewrites, avoiding duplicate dependency identities.

## Release Gate

Before a release or split tag is promoted:

- run `just fast`, `just check`, `just score`, `just security`, and `just artifact-support`
- run `just redline-consumer-test` when Redline compatibility evidence is in scope
- confirm checksum, provenance, SBOM, and cosign evidence for release artifacts
- confirm monitoring is active for the promoted version
- confirm backups or reproducible source inputs exist for rollback
- confirm rate limit or abuse controls are configured for public surfaces

## Redline Consumer Evidence

The reviewed Redline dependency is
`redline-core-v4.1.0-jain.6` at
`d0de59930141baffcfa2b514480e75b14627f24d`. After this dependency and its
contract merge to clean, forge-equal `main`, `ops/ci/redline-consumer.sh`
accepts only a fresh checksummed family receipt with matching manifest and
policy hashes. It runs the real transaction, rollback, checkpoint, and reopen
contract before writing evidence, a test log, and checksum sidecar. A branch,
floating ref, waiver, manually asserted check, or historical manifest is not
release evidence.

Pass this repository's canonical Jeryu family manifest explicitly; it is the
family authority and must never be inferred from the consumer checkout:

```bash
ops/ci/redline-consumer.sh \
  --family-ci /path/to/redline-family-ci.json \
  --redline-manifest /path/to/redline/repos.manifest.toml \
  --redline-policy /path/to/redline/agent/audit-policy.toml \
  --consumer-manifest /home/ubuntu/jain-split/jeryu-split/jeryu-release-ops/repos.manifest.toml \
  --output /path/to/jeryu-consumer.json
```

## Rollback

Rollback uses the previous known-good split tag and its artifact evidence. Do
not overwrite tags; publish a new repair tag or restore consumers to the last
verified tag.

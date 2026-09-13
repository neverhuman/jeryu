# Release and recovery status

The public monorepo and its Linux x86_64 source installation are candidates.
Central signed binary releases, final public-origin installation, independent
component exports and complete local/hosted qualification remain pending.
Binary installation stays closed until verification works end to end.

The existing `jeryu-release-ops/repos.manifest.toml` remains release authority.
The root manifest records a pending protected handover and preserves
`jeryu-split` identity and immutable v5 lineage. Source publication does not
switch the installed service or activate a release.

## Release gate

The release gate requires backup restore proof, a rollback plan, monitoring
dashboard evidence, and abuse or rate limit receipts before any production
launch. Artifacts carry sha256 checksum, SBOM, and provenance evidence.
Candidate metadata stays fail-closed until those proofs exist.

## Release admission

The exact source commit must pass the complete local and GitHub matrices,
anonymous dependency acquisition and fresh unprivileged installation/runtime
qualification. Required owned dependencies must satisfy their effective audit
policies. Every standalone mirror must build, test and audit its own exact
export. Independent review and protected publication precede release tags.

Central artifacts must include source identity, checksums, signatures, SBOMs,
build provenance and installation evidence. Verify platform, signatures,
checksums and provenance before replacing an installed binary. An unavailable
or invalid artifact fails installation.

## Upgrade and recovery admission

An upgrade requires a verified backup of application data, Git repositories,
configuration and the previous release, plus a successful restore rehearsal.
Database and repository state must remain mutually consistent. Do not restore
an older snapshot over accepted new writes without a lossless recovery plan.

Preserve previous signed artifacts and immutable tags. A repair publishes a new
tag; existing tags never move. Installed-service maintenance needs its own
approved window and post-activation checks.

The historical component [release runbook](../components/jeryu-deploy/docs/release.md)
and [release process](../components/jeryu-deploy/docs/release-process.md) retain
existing operational evidence and instructions. Their hosted environment
assumptions do not establish a qualified standalone upgrade procedure.
The [standalone operator guide](recovery.md) describes stopped-server backup,
restoration into a new data directory, upgrade/rollback admission, interrupted
repository creation and remote TLS requirements. Its same-binary restore test
must pass at the exact candidate; cross-version upgrade/recovery qualification
remains open in [current status](migration/STATUS.md).

## Hosted forge releases and Git tags

Jeryu does not yet store hosted release resources or uploaded release assets.
Authenticated, repository-authorized `POST /repos/{owner}/{repo}/releases`
returns `501 Not Implemented`; it creates neither a release nor a Git tag.
The compatibility list is empty. Git tags can be pushed and fetched through
Git independently and do not imply a hosted release resource. Durable release
resources, assets and lifecycle operations remain in the parity program (R30).
Publishing signed Jeryu distribution artifacts on GitHub remains a separate
required foundation release gate.

# Boundaries

Jeryu keeps durable product truth behind typed Rust boundaries.

The machine-readable boundary manifest is `agent/boundaries.toml`. It names the
domain, adapter, web, queue, data-truth, and agent-tool seams that local audits
must check before a cross-boundary change is merged.

Only `jeryu-api`, `jeryu-cli`, and `jeryu-split-tool` have source in this
checkout. The component ownership below is family-wide: nonlocal components
arrive through immutable Git release pins and must be changed and source-tested
in their owning repositories. Deploy tests only its typed integration points.

- `jeryu-core` owns domain objects, branch protection, checks, webhooks, and
  repairable domain errors.
- `jeryu-domain` exposes the canonical domain repair route for agents and audit
  tooling.
- `jeryu-gitd` owns Git repository state and protected ref enforcement.
- `jeryu-cache-*` owns content-addressed cache keys, receipts, poisoning
  defenses, and release cache rules.
- `jeryu-runner-*` owns runner trust decisions, sandbox plans, and execution
  receipts.
- `jeryu-runnerd` owns the workcell warm pool, claim/release epoch fencing,
  startup rebase enforcement, branch-budget metadata, and quarantine-first
  tar import/export validation.
- `jeryu-signrail` owns release witnesses, signatures, checksums, rollback
  metadata, and artifact-support stage receipts. Local signing consumes
  `JERYU_SIGNRAIL_ED25519_SEED`; hosted artifact-support signing consumes only
  the `SIGNRAIL_ED25519_SEED` GitHub Actions secret.
- `jeryu-codegraph` owns an auxiliary `codegraph.sqlite` store for symbol,
  reference, impact, and oracle evidence. It is explicitly listed under
  `agent/boundaries.toml` `db.auxiliary_driver_paths`; it never owns the shared
  forge truth DB or `db/migrations/`.

Cross-boundary calls must use typed ids, receipts, or explicit policy decisions;
direct state mutation from another layer is a bug.

## Governed Tool Boundary

Jankurai source identity is generated from the protected `jeryu-tool`
manifest and projected into workflows, API verification, sandbox image
metadata, receipts, and local CI wrappers. Deploy may verify and consume that
projection only. Ordinary lanes require a digest-bound installation receipt;
`JAIN_RELEASE_CI=1` instead requires the root-broker path, mode `0555`, one
physical link, and no caller-provided receipt or test-authority override.
Unavailable or inconsistent authority fails the lane and never falls back to
an ambient binary.

Externally submitted check-runs, commit statuses, and Jankurai score records
are global-admin maintenance operations. Repository write access alone never
grants evidence-publishing authority. Native runners use runner-only APIs and
the server publishes their exact-head checks internally; runner credentials do
not receive the GitHub-compatible check-minting surface. Branch-protection
changes independently require repository-admin or global-admin authority, so a
writer cannot remove the control that consumes those checks.

The committed Cargo release graph is sibling-free. Internal packages declare
immutable Git tags, and its only patches are Git-to-Git unifiers that map
historical Core/proof and Rustjet consumers to reviewed Core
split.5 and Intelligence split.1 tags. The preserved source spellings are
transported through exact checked-in mappings to the hosted Jeryu forge; Cargo
Deny rejects every source outside that closed set. Local development may supply
command-scoped path overrides, but they are never committed or used by release
CI. The
`release_dependencies_are_immutable_git_sources_without_sibling_paths`
integration test rejects path/null lock sources and any duplicate internal
identity.

## Workcells

Workcell claims can only flow through the runnerd control plane. The workcell
manager may claim a warm cell, fence stale heartbeats by epoch, freeze failed
CI runs into immutable snapshots, and mark a cell blocked if the startup
rebase fails. It may not merge, delete branches, or unpack tarball contents
outside approved repo roots. Exported repair PRs are namespaced under
`agents/{id}/workcells/{wc}/<branch>` and carry the changed-file list so the
review path can inspect the actual edit surface instead of flattening agent
ownership into an anonymous branch.

Inside a cell, the in-cell agent driver (`jeryu-agentbridge`) spawns the
code-writing process through the native `jeryu-sandbox-linux` jail
(`spawn_sandboxed`, Landlock filesystem allowlist + seccomp syscall filter +
`no_new_privs`, unprivileged with no Docker or `sudo`). A jailed process cannot
read or write outside its checkout, cannot open direct `AF_INET` sockets, and
cannot escalate privileges; the file-tree boundary is proven by the `jail_demo`
example and the jailgun tar validators reject any path that resolves outside
the approved roots. Network egress is deny-by-default except through the
`jeryu-egress` allowlist proxy: only vetted hosts are reachable, matched on
exact host or a DNS-suffix on a dot boundary (never substring), and a tripped
token budget revokes egress entirely (`DenyBudget`). See `docs/workcell.md`.

Agent jobs are also **fail-closed on resource limits**: the sandbox refuses to
launch (`EnforcementLevel::Unavailable`) unless a delegated cgroup-v2 subtree is
available to enforce the memory/PID caps, so a runaway agent can never run
uncontained. Ordinary CI/build jobs keep the older degrade-don't-refuse posture.

These boundaries are asserted negatively, not just described: a workspace-only
jail provably DENIES reads of decoy `~/.ssh`/`~/.jeryu` secrets and of an
unclaimed sibling repo while still allowing an in-workspace read
(`secret_paths_denied`), a runaway allocator is OOM-killed by its `memory.max`
(`memory_oom_kill`), the egress proxy denies the plain-HTTP forward path and an
empty allowlist by default, and the record-only auto-merge bridge is held by a
7-probe adversarial harness. See `docs/testing.md`.

# jeryu-tool

**Audit status: PENDING revision-bound verification.** See the
[required audit policy](agent/audit-policy.toml).

Agent and contributor orientation starts at [AGENTS.md](AGENTS.md).
The durable operating contracts are [architecture](docs/architecture.md),
[release](docs/release.md), and [testing](docs/testing.md).

The **tool control plane** for the jeryu family. One repo, two jobs, no product
code:

1. **Audit toolchain** — owns the jankurai pin, floors, and forced scoring so
   every other repo and every agent sandbox audits identically (`tool-manifest.toml`).
2. **Reusable-code-tool registry** — the catalogue of shared crates / TS libs /
   React components / Vite plugins / shell libraries that replace copy-pasted
   code across repos, with adoption and LOC-saved bookkeeping
   (`tools-registry.toml` + `tasks/`). The forge "golden box" on `/repos` reads
   this. See `docs/tools-registry.md`.

Discovery of *new* tool candidates (cross-repo repeated-code clusters) lives in
the sibling `jeryu-tool-finder` repo, which files proposals back into this
registry.

## What lives here

| Path | Purpose |
|---|---|
| `tool-manifest.toml` | **Audit source of truth**: governed source commit/tag, source tree/archive and lock digests, closed vendor and builder identities, exact build contract, binary digest, per-profile score floors, and per-tool default modes. |
| `tools-registry.toml` | **Registry source of truth**: one `[[tool]]` per reusable tool — kind, status, adopting/candidate repos, realized + anticipated LOC saved. |
| `tasks/NNNN-*.toml` | Reusable-tool **build queue**: build-this-tool / migrate-these-repos work items. |
| `ops/registry-summary.sh` | Runs the locked Rust validator for the registry + tasks and computes the golden-box summary (`--check` runs in `just check`). |
| `ops/render-tool-manifest.sh` | Propagates the jankurai pin into every family consumer (CI scripts, workflow envs, sandbox Dockerfiles, per-repo `required_tool_version`). `--check` is the drift lane. |
| `ops/build-jankurai-hermetic.sh` | Materializes and verifies the closed Cargo vendor inventory, then builds the immutable source as a non-root user in the digest-pinned read-only OCI builder with network disabled. |
| `ops/install-jankurai.sh` | Verifies the immutable local-forge source, rejects test authority at `/home/ubuntu/.jeryu`, serializes the complete install/rollback/receipt transaction under a custody-checked lock, delegates to the hermetic builder, atomically installs the binary, preserves rollback content, and writes a content-addressed receipt. |
| `ops/qualify-jankurai-candidate.sh` | Builds the exact premerge candidate into a temporary root and persists a content-addressed diagnostic receipt; it can never target the governed host root. |
| `ops/bootstrap-jankurai-root-seal.sh` | Installed-root-only, PR7-specific cycle breaker: authenticates its own installed entrypoint/pin Git blobs and the installed immutable SplitOps authority, consumes one short-lived request, exposes the pinned candidate to one fixed required-check attempt, and restores the protected predecessor on every exit or recovery. It is not executable from a checkout and is not an installer. |
| `ops/test-bootstrap-jankurai-root-seal.sh` | Proves checkout refusal, installed ancestry/blob/config/broker custody, fixed remote/ref/tag/protection, wrong digest/head/tree/receipt, expiry/reuse, pathname replacement, same-inode drift, held execution, signal, recovery, failure-result, and byte-exact restoration. |
| `ops/test-install-jankurai.sh` | Proves governed-root test-mode refusal, identity-bound idempotency, exclusive concurrent transaction custody, and safe refusal for receipt tamper, external sources/redirects, wrong digests, wrong versions, offline cache misses, interrupted installs, and rollback faults. |
| `ops/test-render-tool-manifest.sh` | Proves unscoped rendering is check-only and write mode rejects missing custody, dirty roots, wrong origins, and heads not based on current protected main. |
| `policy/default-audit-policy.toml` | The jeryu-managed fallback policy used to force-score repos that carry no policy of their own. |
| `generated/jankurai-pin.env` | Generated source/build/binary identity, including commit/tag/tree, archive/lock/binary digests, toolchain, target, and exact version. Do not edit by hand. |
| `docs/tools.md` | The jankurai tool-compounding catalog + adoption guidance (live adoption data comes from the forge). |
| `docs/tools-registry.md` | The reusable-tool registry schema, lifecycle, and LOC-saved definition. |

## Quick start

Install the pinned Rust toolchain and `just`, then run:

```bash
just
```

The full local gate runs drift, locked Rust tests and Clippy, the Jankurai
score, security/SBOM, changed-surface proof, coverage, closed-schema contract,
and repair-receipt hostile lanes. `just fast-proof` and `just fast-test` are the
bounded package-only feedback commands; `just required` is the complete gate.

## Upgrading jankurai (the whole family at once)

1. Edit `[jankurai]` in `tool-manifest.toml`.
2. Commit the manifest update, then run `JERYU_FORGE_TOKEN_FILE=<absolute-private-token-path> ops/render-tool-manifest.sh --repo <name> --repo-root <name>=/home/ubuntu/jain-split/jeryu-split/<name> --expected-head <name>=<40-hex-sha>` for every explicitly claimed root. Unscoped invocation is check-only; writes require the exact handed-off clean canonical physical checkout based on current protected `main`. Alternate clones and registered worktrees are read-only fixtures and can never receive generated writes; the token path must name an owner-held mode-0600, single-link regular file.
3. Land every consumer and this manifest through exact-head protected PRs, then require `ops/render-tool-manifest.sh --check` to be drift-free.
4. Host: `ops/install-jankurai.sh` rebuilds through the exact digest-pinned,
   network-disabled OCI contract and atomically installs only when source,
   vendor, build context, binary, path, and receipt all match.
5. Sandbox: rebuild the agent-sandbox image from the same identity and verify its baked binary digest before any network-isolated lane runs.

`ops/render-tool-manifest.sh --check` fails CI if any consumer drifted from the
manifest, so a half-done bump can never ship.

The hosted protected PR gate may qualify an exact premerge candidate only in a
temporary root with `test_mode=true` and a content-addressed receipt. That
diagnostic candidate never grants installation or merge authority. The bounded
PR7 bootstrap described in `docs/release.md` is the only exception to ordinary
broker selection: it lends the exact candidate bytes to one root-seal attempt,
under root-held transaction custody, and restores protected-main authority
before returning. Once an independently reviewed candidate is installed after
protected merge, the same gate automatically requires
`/home/ubuntu/.jeryu/bin/jankurai` plus its production receipt (or can be
forced fail-closed with `JERYU_TOOL_REQUIRE_GOVERNED_HOST=1`). The GitHub
workflow is a static, non-authoritative mirror and is not a release or review
authority.

## Relationship to standalone jankurai

This repo is **additive**. People auditing their own code keep using jankurai's
own installer and per-repo `agent/audit-policy.toml` exactly as before — nothing
here changes the auditor's CLI or on-disk formats. `jeryu-tool` only governs how
the *jeryu forge* pins, installs, forces, and reports on jankurai.

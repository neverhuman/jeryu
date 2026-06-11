# Jeryu

Public portal for the Jeryu split repository family.

The release authority is `neverhuman/jeryu-deploy`. This repository contains
the installer, the split-family clone entrypoint, local CI wrappers, and audit
metadata. Product source lives in the split member repositories listed below.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/neverhuman/jeryu/main/scripts/install.sh | bash
```

Pin a release or install somewhere else:

```bash
JERYU_VERSION=jeryu-v4.0.0-split.0 JERYU_INSTALL_DIR="$HOME/.local/bin" \
  bash scripts/install.sh
```

The installer downloads the `jeryu` binary from
`neverhuman/jeryu-deploy` releases, verifies `SHA256SUMS`, and runs cosign
verification when `jeryu.sig`, `jeryu.pem`, and `cosign` are available.

## Clone The Split Family

```bash
git clone https://github.com/neverhuman/jeryu.git
cd jeryu
scripts/clone-family.sh "$HOME/jeryu-split"
```

Existing checkouts are updated with `git fetch` and `git pull --ff-only`.
The portal repository is skipped by default so the command can be run from an
already-cloned portal checkout.

## Release Evidence

Release receipts, binary checksums, SBOMs, provenance, witness artifacts, and
rollback evidence are published by `neverhuman/jeryu-deploy`:

- https://github.com/neverhuman/jeryu-deploy/releases
- `SHA256SUMS`
- `release-receipt.json`
- `artifact-support-evidence.tar.gz`

## Split Repository Map

| Repository | Role | GitHub | Purpose |
| --- | --- | --- | --- |
| `jeryu` | Public portal | `neverhuman/jeryu` | Public portal, installer, and split-family clone entrypoint. |
| `jeryu-core` | Split member | `neverhuman/jeryu-core` | Forge/domain truth, git storage, read models, TUI, durable DB migrations. |
| `jeryu-ci-runner` | Split member | `neverhuman/jeryu-ci-runner` | CI IR, scheduler, runner fabric, workcells, sandboxing, agent execution substrate. |
| `jeryu-cache` | Split member | `neverhuman/jeryu-cache` | JeryuCache policy, CAS, receipts, and adversarial poisoning tests. |
| `jeryu-intelligence` | Split member | `neverhuman/jeryu-intelligence` | Codegraph, RustJet, MCP intelligence, review, and autonomy analysis. |
| `jeryu-web` | Split member | `neverhuman/jeryu-web` | Vite/React/TypeScript app, rendered UX QA, and generated contract mirror. |
| `jeryu-release-ops` | Split member | `neverhuman/jeryu-release-ops` | Release, signing, governance, observability, and compliance tooling. |
| `jeryu-deploy` | Split member | `neverhuman/jeryu-deploy` | Integration, end-user binary build, split lock, and release bundle logic. |

## Local Commands

- `just fast`
- `just check`
- `just score`
- `just security`
- `just artifact-support`

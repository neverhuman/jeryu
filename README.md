# Jeryu

**A 100% Rust, local-first GitHub replacement built for AI agents.**

Jeryu is your own forge on localhost — repositories, pull requests, checks, CI,
reviews, gated merges, and releases — with agents as first-class users. It
speaks GitHub's REST dialect (the real `gh` CLI works against it), runs your
CI on your own hardware, and pushes merged work back to GitHub when you want a
public mirror.

## Highlights

- **Agents in sandboxed web terminals** — start a session from the web UI and
  an agent runs in a hardened container (read-only rootfs, pid/memory caps,
  no-new-privileges) on its own branch of your repo, with per-session
  credential seeding and live PTY streaming.
- **Full PR lifecycle** — branch protection, required status checks, reviews,
  linear-history gating, and a merge endpoint that refuses to move `main`
  without green checks (`main` only advances through gated merges).
- **GitHub-compatible REST edge** — point `gh`, scripts, or CI at
  `http://127.0.0.1:8787` and they work.
- **Local CI, your runners** — workflows compile to an IR and run host-native
  or in containers; adversarial suites (sandbox-escape and cache-poisoning
  matrices) guard the substrate itself.
- **Content-addressed build cache** with poisoning defenses and receipts.
- **Codegraph / MCP intelligence** — impact oracles, repeated-code clusters,
  and MCP tools served straight from your forge.
- **Signed releases** — SHA256SUMS, cosign signatures, SBOMs, provenance, and
  rollback evidence.
- **Direct GitHub mirroring** — merging into `main` pushes the new tip to
  `github.com/<your-org>` automatically; the outcome is recorded as a
  `jeryu/github-mirror` check-run next to your CI.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/neverhuman/jeryu/main/scripts/install.sh | bash
```

Pin a release or install somewhere else:

```bash
JERYU_VERSION=jeryu-v5.0.0-split.0 JERYU_INSTALL_DIR="$HOME/.local/bin" \
  bash scripts/install.sh
```

The installer downloads the `jeryu` binary from `neverhuman/jeryu-deploy`
releases, verifies `SHA256SUMS`, and runs cosign verification when
`jeryu.sig`, `jeryu.pem`, and `cosign` are available.

## Quickstart

```bash
jeryu serve --bind 127.0.0.1:8787
# then open http://127.0.0.1:8787 — repos, PRs, checks, and agent sessions
```

## Clone The Split Family

Product source lives in the split member repositories; this portal carries the
installer, the clone entrypoint, and audit metadata. To hack on Jeryu itself:

```bash
git clone https://github.com/neverhuman/jeryu.git
cd jeryu
scripts/clone-family.sh "$HOME/jeryu-split"
```

Existing checkouts are updated with `git fetch` and `git pull --ff-only`.
The portal repository is skipped by default so the command can be run from an
already-cloned portal checkout.

## Split Repository Map

| Repository | Role | GitHub | Purpose |
| --- | --- | --- | --- |
| `jeryu` | Public portal | `neverhuman/jeryu` | Public portal, installer, and split-family clone entrypoint. |
| `jeryu-core` | Split member | `neverhuman/jeryu-core` | Forge/domain truth, git storage, read models, TUI, durable DB migrations. |
| `jeryu-ci-runner` | Split member | `neverhuman/jeryu-ci-runner` | CI IR, scheduler, runner fabric, workcells, sandboxing, agent execution substrate. |
| `jeryu-cache` | Split member | `neverhuman/jeryu-cache` | JeryuCache policy, CAS, receipts, and adversarial poisoning tests. |
| `jeryu-intelligence` | Split member | `neverhuman/jeryu-intelligence` | Codegraph, RustJet, MCP intelligence, review, and autonomy analysis. |
| `jeryu-jira` | Split member | `neverhuman/jeryu-jira` | Work Tracker model, SQLite store, generated contracts, and issue bridge DTOs. |
| `jeryu-web` | Split member | `neverhuman/jeryu-web` | Vite/React/TypeScript app, rendered UX QA, and generated contract mirror. |
| `jeryu-release-ops` | Split member | `neverhuman/jeryu-release-ops` | Release, signing, governance, observability, and compliance tooling. |
| `jeryu-deploy` | Split member | `neverhuman/jeryu-deploy` | Integration, end-user binary build, split lock, and release bundle logic. |

The release authority is `neverhuman/jeryu-deploy`. Cross-repo Rust
dependencies are pinned `*-v5.0.0-split.0` git tags; see `docs/architecture.md`
for how the family fits together.

## Release Evidence

Release receipts, binary checksums, SBOMs, provenance, witness artifacts, and
rollback evidence are published by `neverhuman/jeryu-deploy`:

- https://github.com/neverhuman/jeryu-deploy/releases
- `SHA256SUMS`
- `release-receipt.json`
- `artifact-support-evidence.tar.gz`

## Local Commands

- `just fast`
- `just check`
- `just score`
- `just security`
- `just artifact-support`
- `bash ops/ci/pr-ci.sh` — the canonical PR gate (host CI and the hosted
  workflow both run exactly this)

## License

Apache-2.0 — see [LICENSE](LICENSE).

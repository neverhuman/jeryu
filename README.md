# Jeryu

**A 100% Rust, self-hostable GitHub replacement built for AI agents.**

Agent and contributor entrypoint: [AGENTS.md](AGENTS.md).

Jeryu is your own forge — repositories, pull requests, checks, CI, reviews,
gated merges, and releases — with agents as first-class users. It speaks
GitHub's REST dialect, runs CI on your own hardware, and can push merged work
to an explicitly configured public mirror. The authoritative source for this
split family is hosted at `git.neverhuman.org`; a developer's localhost forge
is not the source of truth.

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
- **Optional GitHub mirroring** — when configured, merging into `main` pushes the new tip to
  `github.com/<your-org>` automatically; the outcome is recorded as a
  `jeryu/github-mirror` check-run next to your CI.

## Install

Start from the hosted source checkout, then pin a release or install somewhere
else:

```bash
git clone https://git.neverhuman.org/git/jeryu/jeryu.git
cd jeryu
JERYU_VERSION=jeryu-v5.0.0-split.0 JERYU_INSTALL_DIR="$HOME/.local/bin" \
  bash scripts/install.sh
```

The compatibility installer still downloads the existing `jeryu-deploy`
binary release assets, verifies `SHA256SUMS`, and runs cosign verification when
`jeryu.sig`, `jeryu.pem`, and `cosign` are available. That artifact channel is
separate from hosted source authority; this candidate does not claim that a
`git.neverhuman.org` release feed is active.

## Quickstart

```bash
jeryu serve --bind 127.0.0.1:8787
# then open http://127.0.0.1:8787 — repos, PRs, checks, and agent sessions
```

## Clone The Split Family

Product source lives in the split member repositories; this portal carries the
installer, the clone entrypoint, and audit metadata. To hack on Jeryu itself:

```bash
git clone https://git.neverhuman.org/git/jeryu/jeryu.git
cd jeryu
scripts/clone-family.sh "$HOME/jeryu-split"
```

Use `scripts/clone-family.sh --plan "$HOME/jeryu-split"` to inspect every URL
and destination without writing. Existing checkouts are updated only when
their raw `origin` is the exact hosted authority, their tree is clean, and
their HEAD is attached; the update is fast-forward-only. The portal repository
alone is skipped by default so the command can be run from an already-cloned
portal checkout.

## Split Repository Map

| Repository | Role | Hosted source | Purpose |
| --- | --- | --- | --- |
| `jeryu` | Public portal | `jeryu/jeryu` | Public portal, installer, and split-family clone entrypoint. |
| `jeryu-core` | Split member | `jeryu/jeryu-core` | Forge/domain truth, git storage, read models, TUI, durable DB migrations. |
| `jeryu-ci-runner` | Split member | `jeryu/jeryu-ci-runner` | CI IR, scheduler, runner fabric, workcells, sandboxing, agent execution substrate. |
| `jeryu-cache` | Split member | `jeryu/jeryu-cache` | JeryuCache policy, CAS, receipts, and adversarial poisoning tests. |
| `jeryu-intelligence` | Split member | `jeryu/jeryu-intelligence` | Codegraph, RustJet, MCP intelligence, review, and autonomy analysis. |
| `jeryu-jira` | Split member | `jeryu/jeryu-jira` | Work Tracker model, SQLite store, generated contracts, and issue bridge DTOs. |
| `jeryu-web` | Split member | `jeryu/jeryu-web` | Vite/React/TypeScript app, rendered UX QA, and generated contract mirror. |
| `jeryu-release-ops` | Split member | `jeryu/jeryu-release-ops` | Release, signing, governance, observability, and compliance tooling. |
| `jeryu-deploy` | Split member | `jeryu/jeryu-deploy` | Integration, end-user binary build, split lock, and release bundle logic. |
| `jeryu-tool` | Split member | `jeryu/jeryu-tool` | Governed Jankurai identity, installation custody, and family consumer rendering. |
| `jeryu-tool-finder` | Split member | `jeryu/jeryu-tool-finder` | Tool discovery policy, catalog metadata, and bounded lookup behavior. |

Each hosted source value expands to
`https://git.neverhuman.org/git/<owner>/<repository>.git`. The release
authority is `jeryu/jeryu-deploy`. Cross-repo Rust dependencies retain their
governed pinned URL spellings and immutable split tags; see
`docs/architecture.md` for how the family fits together.

## Release Evidence

Release receipts, binary checksums, SBOMs, provenance, witness artifacts, and
rollback evidence are governed by hosted repository `jeryu/jeryu-deploy`.
Legacy public artifact compatibility currently points at:

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

## Governed auditor

CI invokes only the receipt-verified `/home/ubuntu/.jeryu/bin/jankurai` identity
rendered by `jeryu-tool`. The 1.6.11 auditor cutover is CI authority only; it
does not change this repository's product version, release tag, or artifacts.

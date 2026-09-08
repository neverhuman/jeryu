# jeryu-tool-finder

[![CI](https://img.shields.io/badge/CI-check%20%7C%20score%20%7C%20security%20%7C%20contract%20%7C%20artifact-blue)](.github/workflows/ci.yml)
[![jankurai score](https://img.shields.io/badge/jankurai-0%20caps-brightgreen)](agent/audit-policy.toml)

Agents start at **[AGENTS.md](AGENTS.md)** (the agent entrypoint); deeper docs
are indexed there and under [`docs/`](docs/).

The **tool-discovery** arm of the jeryu family. One Rust CLI scans
**every** repo at once, finds code that is duplicated across **more than one
repo**, and turns the strongest clusters into agent-readable dossiers — the leads
worth extracting into a shared tool.

`jeryu-tool` owns the *registry* of reusable tools; `jeryu-tool-finder` is what
*discovers candidates* for it. The loop:

```
jeryu-tool-finder scan     → cross-repo clusters
jeryu-tool-finder dossier  → one dossier per cluster (files, examples, LOC saved)
  ↓ an agent/human reads a dossier and decides
jeryu-tool-finder propose  → [[tool]] + build task in jeryu-tool (status=proposed)
  ↓ jeryu-tool tracks build + adoption + LOC saved
forge golden box →  shows the payoff on /repos
```

## Rust CLI commands

| Command | What it does |
|---|---|
| `scan` | Uses the linked `jeryu-codegraph` engine across a manifest or the whole system and writes `dossiers/clusters.json`. |
| `dossier` | Enriches clusters with paths, previews, suggested tool identity, and anticipated LOC saved. |
| `propose` | Adds a proposed registry entry and build task to sibling `jeryu-tool`, idempotent on `origin_cluster`. |
| `summary` | Delegates registry summary rendering to the sibling registry owner. |

## Immutable library boundary

The CLI links `jeryu-codegraph` as a Rust library. `Cargo.toml` preserves the
immutable source coordinate `jeryu-intelligence-v5.0.0-split.1`, `Cargo.lock`
binds its exact commit, and governed Git/Cargo configuration transports that
coordinate through `git.neverhuman.org`. No relative Cargo patch or ambient
sibling checkout participates in a normal, test, or release build.

## Quick start

```bash
bash scripts/ci-doctor.sh   # confirm required tooling, including Cargo
just                        # full product gate: check/score/security/contract/artifact
# or without `just`:
bash scripts/ci-local.sh required # canonical gate: check/score/security/contract/artifact
```

Wire the local gate to run before every push:

```bash
git config core.hooksPath ops/git-hooks
```

## Local commands

```bash
just scan       # scan the whole family for cross-repo clusters
just dossier    # render dossiers from the latest scan
just summary    # golden-box registry numbers (from jeryu-tool)
just contract-drift  # compare compiled CLI help with the tracked contract
just artifact-support # bind the release CLI to exact score/security evidence
just            # the full product gate
```

## Docs

- [docs/architecture.md](docs/architecture.md) — components, boundaries, data flow
- [docs/tool-finder.md](docs/tool-finder.md) — dossier schema, LOC-saved definition
- [docs/testing.md](docs/testing.md) — CI lanes, the dossier selftest, CI parity
- [docs/release.md](docs/release.md) — version source, release gate, rollback
- [CHANGELOG.md](CHANGELOG.md) — release notes

Cross-repo cluster discovery and the `repo_count` shape live in
`jeryu-intelligence/crates/jeryu-codegraph` (`tool-build scan-family`). See
`docs/tool-finder.md` for the dossier schema, the LOC-saved definition, and the
known `candidate_repos` caveat.

## Governed auditor

CI invokes only the receipt-verified `/home/ubuntu/.jeryu/bin/jankurai` identity
rendered by `jeryu-tool`. The 1.6.11 auditor cutover is CI authority only; it
does not change this repository's product version, release tag, or artifacts.

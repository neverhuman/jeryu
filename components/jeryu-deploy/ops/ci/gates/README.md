# Per-phase local CI gates

Each script in this directory is a **distinct gate** for one engineering-spec
phase. A gate prints exactly one final line:

```
GATE <name>: PASS | FAIL | PENDING
```

and exits `0` only when the result is `PASS` or an acknowledged `PENDING`.
A gate never reports green for a capability that has not been built yet.

Run all gates and get a summary table:

```bash
bash ops/ci/gates/agent-substrate.sh  # direct in-cell agent substrate gate
bash scripts/ci-phases.sh          # run every gate, print summary, exit 1 on any FAIL
bash scripts/ci-phases.sh --list   # just list the discovered gates
```

The aggregator exits nonzero if **any** gate `FAIL`s (or emits no recognizable
`GATE` line). `PENDING` does **not** fail the run but is always reported
distinctly in the summary, never hidden.

## What "PENDING" means

`PENDING` marks a gate whose **in-repo** portion is green but whose **live**
capability is not yet wired in this environment (it needs a daemon, a sandbox
runtime, or an adversarial service). The runnable tests must still pass; only
the not-yet-buildable live portion is held at `PENDING`. The live portion is
**never** reported as `PASS`.

## Gate -> phase map

| Gate (`ops/ci/gates/*.sh`) | Engineering-spec phase | What runs now | PENDING portion (live capability still to build) |
| --- | --- | --- | --- |
| `agent-substrate.sh` | In-cell agent execution substrate | Deploy-owned `jeryu-api` route tests for `workcell_run_agent` and `agent_runs`, exercised against the immutable agentbridge dependency. | none; live LLM/network calls stay opt-in through the owning repository's budget and secret gates. |
| `foundation.sh` | Cross-cutting baseline | Delegates to `ops/ci/full.sh`: fmt, check, Clippy, workspace tests, repository proof evidence, workflow parity, map coverage, release-receipt contract, score, security, and doctor. | none |
| `github-conformance.sh` | GitHub-compatible forge surface | `cargo test -p jeryu-api --test github_api` (REST shape) **and** domain-vocabulary assertions over the owned `crates/jeryu-api/src`: GitHub terms present, and zero retired domain identifiers / legacy-provider / legacy-CI tokens. | none |
| `ir-determinism.sh` | CI compile -> deterministic IR | `cargo test -p jeryu-ci-ir` (deterministic IR-hash + DAG invariants). | none |
| `proof-gate.sh` | Proof-carrying merges | `cargo test -p jeryu-proof` (no-proof-no-merge, owner/test-map matching, generated-zone enforcement). | none |
| `git-oracle.sh` | gitd as a stock-git-compatible oracle | `cargo test -p jeryu-gitd` plus a local differential oracle comparing a gitd-managed repo with stock bare Git for refs, object types/content, clone, fetch, and push behavior. | none for the local gate; daemon HTTP/SSH transport oracle remains future hardening |
| `runner-sandbox.sh` | Isolated job runners (native + OCI) | Pinned `jeryu-runnerd` tests, Deploy-owned API integration tests, and the live Docker escape matrix. | none; inability to execute the live matrix is a failure, not a cosmetic pass |
| `cache-safety.sh` | Content-addressed cache client boundary | The owned CLI dispatch/self-test contract. Cache implementation and poisoning tests remain release gates of the standalone `jeryu-cache` repository. | none |
| `coverage.sh` | Coverage + mutation evidence for the jankurai coverage audit | Delegates to `ops/ci/coverage.sh`: `cargo llvm-cov` over the three Deploy-owned crates, an API source ratchet, `cargo-mutants` scoped to `jeryu-split-tool`, then `jankurai coverage audit` asserting `hard=0`. | `PENDING` (not `FAIL`) only when pinned coverage/mutation tooling genuinely cannot be installed; a receipt is mandatory |

## Conventions

- Bash with `set -uo pipefail`.
- `grep` usage is ugrep-compatible: newline-delimited output only, no `-Z` / `-0`.
- These phase wrappers are **additive**: each owns a distinct proof surface,
  while `foundation.sh` delegates to the canonical standalone `ops/ci/full.sh`.
- Legacy-provider / legacy-CI token names are hex-decoded at runtime inside
  `github-conformance.sh`, so no gate file contains a literal forbidden token.

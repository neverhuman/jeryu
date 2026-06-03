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
| `agent-substrate.sh` | In-cell agent execution substrate | `cargo test -p jeryu-agentbridge -p jeryu-egress --jobs 40`, including adversarial parallel edit-bot staging and the live egress contract. | none; live LLM/network calls stay opt-in through `jeryu-egress` budget and secret gates. |
| `foundation.sh` | Cross-cutting baseline | Delegates to `ops/ci/full.sh`: fmt, check, clippy, workspace test, zero-evidence guard, docs, release receipt, repo score. | none |
| `github-conformance.sh` | GitHub-compatible forge surface | `cargo test -p jeryu-api --test github_api` (REST shape) **and** domain-vocabulary assertions over `crates/jeryu-core/src` + `crates/jeryu-api/src`: GitHub terms present, and zero retired domain identifiers / legacy-provider / legacy-CI tokens. | none |
| `ir-determinism.sh` | CI compile -> deterministic IR | `cargo test -p jeryu-ci-ir` (deterministic IR-hash + DAG invariants). | none |
| `proof-gate.sh` | Proof-carrying merges | `cargo test -p jeryu-proof` (no-proof-no-merge, owner/test-map matching, generated-zone enforcement). | none |
| `git-oracle.sh` | gitd as a stock-git-compatible oracle | `cargo test -p jeryu-gitd` plus a local differential oracle comparing a gitd-managed repo with stock bare Git for refs, object types/content, clone, fetch, and push behavior. | none for the local gate; daemon HTTP/SSH transport oracle remains future hardening |
| `runner-sandbox.sh` | Isolated job runners (native + OCI) | `cargo test -p jeryu-runner-core -p jeryu-runner-native -p jeryu-runner-oci -p jeryu-runnerd`. | Live seccomp / Landlock / cgroups escape suite — needs the **native sandbox runtime**. |
| `cache-safety.sh` | Content-addressed poisoning-resistant cache | `cargo test -p jeryu-cache-core -p jeryu-cache-service -p jeryu-cache` (+ `jeryu-cache-adversary` when present) plus `tests/cache_poisoning_matrix.sh` local poisoning/false-hit harness. | none for the local gate; networked adversarial service remains future hardening |
| `coverage.sh` | Coverage + mutation evidence for the jankurai coverage audit | Delegates to `ops/ci/coverage.sh`: `cargo llvm-cov` over the five critical engine crates -> `target/llvm-cov/lcov.info`, `cargo-mutants` scoped to one critical crate with a `--timeout` -> `target/mutants/mutants.out/outcomes.json`, then `jankurai coverage audit` asserting `hard=0`. | `PENDING` (not `FAIL`) when `cargo-llvm-cov` / `cargo-mutants` genuinely cannot be installed on the host (coverage.sh exits 3 + writes `target/coverage/skip-receipt.txt`). |

## Conventions

- Bash with `set -uo pipefail`.
- `grep` usage is ugrep-compatible: newline-delimited output only, no `-Z` / `-0`.
- These scripts are **additive**. They do not modify `ops/ci/full.sh`,
  `scripts/ci-local.sh`, or any crate source.
- Legacy-provider / legacy-CI token names are hex-decoded at runtime inside
  `github-conformance.sh`, so no gate file contains a literal forbidden token.

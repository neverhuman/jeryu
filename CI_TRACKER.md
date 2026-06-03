# jeryu — Local CI Tracker (confidence ledger)

Shared living dashboard of **local** test + gate health (maintained by both agents).
Working policy: local validation first, then branch push + PR review through
`https://github.com/neverhuman/jeryu/`. Direct `main` pushes are no longer the
default closeout path; `ci-fast-push.sh` requires explicit `--push-main` or
`JERYU_CI_PUSH_MAIN=1` for that escape hatch.

Run locally: `bash ci-fast-push.sh --full --no-push` (core parity gate) ·
`bash scripts/ci-phases.sh` (per-phase gates) · `./ops/ci/full.sh` (foundation)
· `cargo nextest run --workspace` (raw tests). A gate is **PASS / FAIL / PENDING**
(capability not built yet — never silently green).

Identity law: jeryu reads as a self-hosted GitHub-compatible forge. CI is GitHub-Actions +
native only; zero retired-provider evidence (enforced by the zero-evidence gate).

_Last updated: 2026-06-02 · Latest full parity gates are green with 40 workers in both profiles: local-native `JERYU_CI_ALLOW_RETIRED_PROCESSES=1 JERYU_CI_ALLOW_RETIRED_LISTENERS=1 bash ci-fast-push.sh --full --no-push` passed in **91s**, and GitHub-clean `JERYU_CI_ALLOW_RETIRED_PROCESSES=1 JERYU_CI_ALLOW_RETIRED_LISTENERS=1 JERYU_CI_PROFILE=github JERYU_CI_USE_SCCACHE=0 bash ci-fast-push.sh --full --no-push` passed in **87s**. The explicit retired-state bypass is only for this host because root-owned retired-provider services and the Docker-backed `:2224` listener remain active; authentic `ops/ci/verify-jeryu-env.sh --build-local --release-guard` fails closed until an operator stops them. The full gate verifies the repo-local `jeryu` binary, pins Jankurai 1.6.10, proves the GitHub vanilla profile, installs the pinned open security toolchain, runs workspace clippy and **1175 nextest tests**, phase gates, and every manifest lane in `agent/ci-lanes.toml` (`ci-fast`, `jankurai`, `security`, `proof-evidence`). Today's phase-gate-only evidence is **PASS=10 · PENDING=0 · FAIL=0** including `agent-substrate`; the manifest proof-evidence Jankurai full scan remains **score 92, caps 0**, final Jankurai diff audit **score 83, hard 0, caps 0**, and final changed-file audit **score 83, caps 0**. First-wave local import registered **28 repos/mirrors** under `~/.local/share/jeryu`, `/api/v1/repos` lists them, and the git oracle proves imported repos materialize under `git/repos/OWNER/REPO.git` for clone/fetch. Remote is canonical GitHub only (`git@github.com:neverhuman/jeryu.git`; no local `:2224` forge remote)._

## v4.1 Closeout

The runner spine, 40-slot fleet, and read-only API surfaces are the current baseline. Do not relist them here unless a regression reopens them.

The stale v4.0 deferred rows were retired from this tracker. Remaining closeout work is limited to the live sweep below:

- `jeryu/autonomy` stays advisory until the merge decision is provably safe again: head-pinned, reviewer-verified, base..head diffed, author/fork trust-checked, changed-file evidence populated, and empty/skipped CI fail-closed.
- `agent/repo-score.json` is the live blocker list for the closeout sweep. Regenerate it with the pinned `jankurai 1.6.10` binary before treating any finding as current evidence.
- Release closeout still needs a signed-commit build, SBOM/provenance/rollback evidence, and branch + PR publication before tagging.

## Current Gate Snapshot

- PASS=10 · PENDING=0 · FAIL=0 on the last recorded phase-gate run.
- `agent-substrate` covers `cargo test -p jeryu-agentbridge -p jeryu-egress --jobs 40`: deterministic edit-bot staging is network-denied, while live agent egress is opt-in, allowlisted, secret-explicit, and budget-gated.
- `just fast`, `just ci`, `just full`, `just security`, and `just audit` remain the main local proof surfaces.
- The open audit artifacts live under `target/jankurai/` and `.jankurai/`; do not hand-copy their contents into this tracker.

# JeRyu Release Policy

Owner: `@veox-ai/jeryu-ops`
Source of truth: [`release.policy.toml`](../release.policy.toml)
Companion proof: [`proof-lanes.toml`](../proof-lanes.toml)
Existing pipeline doc: [`docs/release.md`](release.md)

This document defines how code reaches production in JeRyu's agent-first
release process. The canonical pattern, distilled from `tips/release/*.txt`:

> **Agents propose. JeRyu proves. GitHub enforces. Humans approve high-risk
> and production. Canary limits blast radius. Rollback is predeclared.**

## The flow

```
Issue (agent or human)
  → agent grant (capability check)
  → agent/<agent>/<issue>-<slug> branch
  → draft GitHub PR (template-enforced)
  → JeRyu proof pipeline → jeryu/release-ready composite check
  → reviewer-agent advisory check
  → human/CODEOWNER approval (REQUIRED after CI green)
  → GitHub merge queue (merge_group)
  → protected main (linear, no force-push, no bypass)
  → snapshot artifact (immutable, SHA-bound)
  → dogfood (jeryu install smoke)
  → vX.Y.Z-rc.N canary (release.yml workflow)
  → vX.Y.Z stable (environment: release, required reviewer)
  → rollback: flag → previous artifact → revert PR → patch
```

Every transition produces evidence in `ops/releases/<version>/` (schema below).

## The single required check: `jeryu/release-ready`

One composite check is the only required-status on `main`. It is posted by
`.github/workflows/release-ready.yml` invoking:

```
jeryu release ready --pr <num> --emit-status
```

Composition (see `release.policy.toml` `[gate.jeryu_release_ready]`):

| Receipt          | What it proves                                                          |
|------------------|-------------------------------------------------------------------------|
| `intake`         | Issue linked, PR template complete, agent disclosure present            |
| `vti-plan`       | VTI plan emitted, receipt id stable, base/head SHAs bound               |
| `proof-receipt`  | Required lane(s) ran and passed (per proof-lanes.toml change-type)      |
| `risk-gate`      | Capability grant valid for touched paths                                 |
| `reviewer-agent` | Advisory reviewer-agent ran (CI weakening detection blocks)             |
| `rollback-plan`  | Declared rollback path or explicit "docs only" exemption                |
| `ci-checks`      | Underlying granular CI workflows green for HEAD                          |

Granular underlying checks (`Rust / *`, `jankurai / *`) remain visible but are
not individually required — only the composite is required for merge.

## Branch protection settings (manual, on GitHub)

Set on `main`:

- [x] Require a pull request before merging
- [x] Required approvals: 1 (rises to 2 for Tier-3 paths via CODEOWNERS)
- [x] Dismiss stale approvals on new commits
- [x] Require review from someone other than the author
- [x] Require status checks to pass: `jeryu/release-ready`
- [x] Require branches to be up to date before merging (handled by merge queue)
- [x] Require conversation resolution before merging
- [x] Require linear history
- [x] Require merge queue
- [x] Block force pushes
- [x] Block deletions
- [x] Apply rules to administrators
- [x] Restrict who can push directly to matching branches: none
- [ ] Allow bypass: **disabled**

Tag rule for `v*`:

- [x] Restrict creation to the `release` workflow / release manager
- [x] Block force-update and deletion

## GitHub Environment: `release`

Required for the `publish` job in `.github/workflows/release.yml`.

- [x] Required reviewer: release manager or maintainer (not the workflow trigger)
- [x] Prevent self-review
- [x] Wait timer: 0 (optional, set if rolling forward is too fast for canary signal)
- [x] Deployment branches/tags: `v*` tags only
- [x] Environment secrets scoped to release-only (registry tokens, signing keys)
- [x] Custom protection rule: `jeryu/release-doctor`

## Risk tiers

See `release.policy.toml` `[[risk_tier]]`. Summary:

| Tier | Examples                                            | Approvals | CODEOWNER | Canary |
|------|-----------------------------------------------------|-----------|-----------|--------|
| 0    | docs, typo, non-behavioral                          | 1         | no        | no     |
| 1    | small bugfix in `src/**` or `crates/**`             | 1         | no        | no     |
| 2    | feature / API / behavior / TUI                      | 1         | yes       | no     |
| 3    | secrets / release / CI / workflow / agent policy    | 2         | yes       | yes    |
| 4    | emergency prod fix (follow-up PR required)          | 1         | yes       | no     |

## Evidence directory contract

Every release version writes to `ops/releases/<version>/`:

```
release-plan.md          # human-readable plan (what we're shipping)
release-attempt.json     # state-DB row mirror: attempt id, status, timestamps
vti-plan.json            # VTI plan + receipt id, bound to base/head SHAs
proof-receipts.jsonl     # one row per lane run (command, exit, duration, evidence path)
release-doctor.json      # output of `jeryu release doctor`
preflight.json           # SSH / Vault / registry / disk preflight
security-evidence.json   # cargo-deny, secret scan, dep review, jankurai audit
sbom.cdx.json            # CycloneDX SBOM
attestations.json        # GitHub artifact attestations (binaries + SBOM)
canary-report.json       # install smoke, remote doctor, channel exposure
install-smoke.json       # smoke install on dogfood/canary
rollback-target.json     # previous known-good version + rollback path
changelog.md             # snippet for this version
```

Missing entries are surfaced in the TUI Release → Evidence sub-pane.

## Rollback ladder

Always in this order. Never re-tag a published version.

1. **Feature flag off** — fastest, leaves binary intact.
2. **Channel rollback** — point stable → previous known-good artifact.
3. **Revert PR** — through normal merge queue. Publish a patch release.
4. **Incident** — open an incident issue, follow runbook below.

### Emergency runbook (Tier-4)

1. Release manager triggers `jeryu release rollback --version <bad>`.
2. The command walks `release.policy.toml [[rollback_step]]` in order.
3. Records `ops/releases/<bad>/rollback.json` with reason, actor, timestamp.
4. **Within 24h** a follow-up PR captures the root cause + permanent fix.

## CLI commands

| Command                              | Who runs it      | What it does                                       |
|--------------------------------------|------------------|----------------------------------------------------|
| `jeryu agent submit`                 | agent            | Run proof → write capsule → `gh pr create --draft` |
| `jeryu release ready --pr N`         | CI workflow      | Compose gate, post GitHub Check Run                |
| `jeryu release dry-run --version V`  | agent or human   | Run full pipeline locally, no publish              |
| `jeryu release submit --version V`   | release manager  | Tag + push + trigger `release.yml`                 |
| `jeryu release approve --pr N`       | human            | Approve PR after CI green (cannot self-approve)    |
| `jeryu release rollback --version V` | release manager  | Walk rollback ladder, write evidence               |
| `jeryu release status / watch / doctor / preflight / reconcile` | any | Inspect release state (existing) |

All accept `--json` for TUI consumption and `--dry-run` for rehearsal.

## TUI surface

- **Release** tab — three sub-panes (cycle with `1`/`2`/`3` or `h`/`l`):
  - **Pipeline** — horizontal swimlane funnel (Plan → Build → Proof → Canary → Stable). Stacked cards show parallel agent work.
  - **Evidence** — per-version evidence-directory listing with status icons.
  - **Rollback** — declared rollback path and rehearsal status.
- **Approvals** tab — queue of agent PRs awaiting human approval after CI green; `^K approve` calls `jeryu release approve`, `^K reject` posts a structured review.

## DORA metrics

Tracked in state DB and surfaced via `jeryu release status --json`:

- Deployment frequency
- Change lead time (issue → merged main)
- Failed deployment recovery time (canary fail → rollback complete)
- Change failure rate (% releases requiring rollback or patch within 24h)
- Deployment rework rate (% PRs needing a fix-forward within 24h)

## Out of scope (deferred)

- `crates.io` Trusted Publishing (OIDC). Track as follow-up issue once the
  publish job in `release.yml` is stable.
- Real production traffic-split canary. We use **channel canary** only
  (snapshot / rc / stable channels) until JeRyu has hosted services with
  user-facing traffic.
- GitLab MR adapter for `jeryu agent submit`. Existing `agent merge` path on
  GitLab MRs is unaffected and serves hosted-JeRyu users.

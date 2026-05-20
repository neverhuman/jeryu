# Evidence Gate — Formal Specification

> Public name: **Evidence Gate**. Internal brand: **VibeGate Delivery Spine**.
>
> This document is the normative specification for the eight typed objects
> exchanged between agents, the seven non-negotiable laws every conforming
> implementation must obey, and the six-tier risk model used by the gate.
>
> If a behavior described here disagrees with the implementation in
> `/home/ubuntu/jeryu/src`, the implementation is the source of truth for
> the current release and this document is filed as a bug.

## How to read this document

The spec is split into three parts:

1. **The seven laws** — invariants that bind every implementation. No risk
   score, profile, or human can suspend them at runtime.
2. **The risk model** — the six tiers R0 through R5, with the matchers and
   quorum each tier triggers.
3. **The eight typed objects** — JSON Schema definitions, copied verbatim
   from the canonical files under `.jeryu/autonomy/schemas/`, with a short
   preamble explaining why each object exists and how it is produced.

Throughout this document the terms **agent**, **reviewer**, **judge**, and
**release shepherd** refer to the roles defined in
[`docs/llm-reviewers.md`](./llm-reviewers.md). The narrative overview of
the whole system lives in
[`docs/autonomous-delivery.md`](./autonomous-delivery.md).

Acronyms used in this document:

- **MR / PR** — Merge Request (GitLab) / Pull Request (GitHub).
- **SHA** — the 40-hex-character Git commit identifier.
- **SBOM** — Software Bill of Materials (SPDX or CycloneDX).
- **SLSA** — Supply-chain Levels for Software Artifacts.
- **SAST** — Static Application Security Testing.
- **TTL** — Time To Live (a freshness window for a signed object).
- **CI** — Continuous Integration (the pipeline that runs on every change).
- **LLM** — Large Language Model.

---

## Part 1 — The seven non-negotiable laws

These are copied verbatim from the canonical design at
`/home/ubuntu/jeryu/tips/fullauto/tip1.txt:42-68`. A conforming
implementation must obey all seven. Failing to enforce any one of them
voids the trust model.

### Law 1 — No durable change without a PR/MR

> Agents may create branches, commits, draft PRs/MRs, evidence, and
> reviews. They may not push directly to `main`, protected release
> branches, production manifests, or stable tags.

### Law 2 — No self-approval

> The agent that authored a change cannot approve, merge, deploy, or bless
> that same change. Approval must come from independent reviewers: another
> agent role, a human, or both.

### Law 3 — Policy comes from the target branch, never the PR branch

> A malicious or mistaken agent must not be able to weaken
> `.jeryu/autonomy/**`, CI, CODEOWNERS, security policy, or deployment rules
> inside the same branch it wants approved.

### Law 4 — Every decision is exact-SHA-bound

> Approvals, evidence packs, gate verdicts, merge passports, release
> passports, and deployment receipts must bind to the exact commit SHA,
> artifact digest, policy version, and evidence digest.

### Law 5 — Hard stops beat risk scores

> A low aggregate risk score cannot override protected paths, missing
> evidence, changed tests without explanation, removed scanners,
> auth/crypto changes, secret exposure, destructive migrations, production
> policy edits, or release-control edits.

### Law 6 — Build once, promote the same artifact

> Do not rebuild different binaries for dev, staging, and prod. Build once
> from a certified source SHA, generate SBOM/provenance, sign/attest the
> artifact, and promote that artifact through environments.

### Law 7 — Autonomous production requires automatic rollback

> Full autonomy to production is allowed only when rollback is already
> proven, telemetry is wired, the blast radius is bounded, and the release
> agent can stop or roll back faster than a human.

### Conformance test summary

A conforming implementation must satisfy every row in the table below.
Failure of any row is non-conforming.

| Law | Test | Where enforced today |
| --- | --- | --- |
| 1 | A direct push to a protected branch by an agent identity is rejected at the platform layer. | `.jeryu/autonomy/policies/protected-paths.yml`, branch protection on the hosting platform. |
| 2 | An `AgentApprovalReceipt` whose `agent_id` matches `EvidencePack.author_agent` is dropped from quorum. | `src/approval/quorum.rs` `no_self_approval` check. |
| 3 | The `policy_sha` on every receipt and verdict is loaded from the target branch's commit, not the PR branch. | `src/autonomy/policy_yaml.rs` (`PolicyBundle::from_dir`), `src/agent_review/judge.rs:42-50`. |
| 4 | A `VibeGateVerdict` is valid only when `head_sha`, `policy_sha`, and `evidence_pack_digest` all match the current MR/PR head. Drift → `RequireHuman`. | `src/approval/sha_bind.rs`, `src/agent_review/judge.rs:46-52`. |
| 5 | Any triggered hard-stop in `.jeryu/autonomy/policies/approvals.yml::hard_stops` overrides every passing reviewer. | `src/autonomy/conditions.rs`, `src/agent_review/judge.rs:55-67`. |
| 6 | The artifact digest in `ReleasePassport.artifact_digest` is reused unchanged across all `allowed_environments`. | `.jeryu/autonomy/schemas/release-passport.schema.json`. |
| 7 | Deploys to `prod` require a non-empty `ReleasePassport.rollback_plan.tested == true`. | `.jeryu/autonomy/schemas/release-passport.schema.json`. |
| 5b | The single visible required check posted to the git host is named **exactly** `vibegate/merge-passport`. No per-reviewer check-run is required by branch protection. | `src/git_host/mod.rs` (`VIBEGATE_MERGE_PASSPORT_CHECK_NAME`), `src/git_host/github.rs` (`GitHubClient::post_merge_passport_check`). See "Visible required check name" below. |

---

## Part 2 — The risk model

The risk model has six tiers, R0 through R5. Each MR/PR is classified
into exactly one tier by the rules in
`/home/ubuntu/jeryu/.jeryu/autonomy/policies/risk.yml`. The classifier walks
the tiers top-down (R5 first) and the first matching tier wins. This
intentionally implements **veto semantics**: a hard-stop tier (R5 or R4)
cannot be undone by a lower tier matcher applying later.

### The tiers at a glance

| Tier | Description | Auto-merge | Required reviewer roles | Human required |
| --- | --- | --- | --- | --- |
| **R0** | Docs, comments, formatting, harmless metadata | Yes | none | No |
| **R1** | Small isolated code change with strong targeted tests | Yes | `test_integrity` | No |
| **R2** | Normal product change (default catch-all) | Yes | `test_integrity`, `security` | No |
| **R3** | Large, novel, dependency, performance, data, or broad behavior change | No | `test_integrity`, `security`, `runtime`, `lockfile` | Yes |
| **R4** | Auth, crypto, secrets, infra, CI, policy, release, prod, prompt/judge rules | No | n/a | Yes (fail-closed without human) |
| **R5** | Missing/tampered evidence, suspicious behavior, emergency, unknown blast radius | No | n/a | Yes (fail-closed) |

### Matchers used by the classifier

Every matcher referenced under `tiers[].matchers[]` in
`risk.yml` resolves to one of these forms:

- `paths_match: ["glob", ...]` — at least one changed file matches one
  of the gitignore-style globs.
- `paths_only_in: ["glob", ...]` — every changed file matches at least
  one of the globs. (Stronger than `paths_match`.)
- `any_path_matches_protected: true` — any changed file matches the
  union of globs in `.jeryu/autonomy/policies/protected-paths.yml`.
- `lines_changed_gte: N` / `lines_changed_lte: N` — total added plus
  removed lines across the diff.
- `all_files_have_targeted_tests: true` — every changed source file has
  a matching test in `EvidencePack.tests.targeted`.
- `conditions: [name, ...]` — every named condition resolves to a
  vetted Rust function in `src/autonomy/conditions.rs`. Unknown names
  fail closed to R5. (See "The named-condition registry" below.)
- `default: true` — the catch-all matcher used by R2.

### The named-condition registry

Veto and risk-escalation conditions are not free-form expressions. They
are vetted Rust functions registered in
`/home/ubuntu/jeryu/src/autonomy/conditions.rs`. This is decision #3 of
the project handshake (YAML-only policy, no DSL): the policy file
references a fixed set of names; new names are added in code and
themselves go through R4 review. There is no runtime expression
evaluator, no Rego sandbox, no string `eval`.

The registry today contains the following names. The ones marked
"local" fire from the `(EvidencePack, &[AgentApprovalReceipt])` tuple
alone; the ones marked "external" require richer context (head SHA
drift, CODEOWNERS lookups, freeze windows) that the judge or
orchestrator injects.

| Name | Kind | Origin |
| --- | --- | --- |
| `evidence_missing` | local | `cond_evidence_missing` |
| `evidence_signature_invalid` | local | `cond_evidence_signature_invalid` |
| `secret_scan_failed` | local | `cond_secret_scan_failed` |
| `secret_scan_missing` | local | `cond_secret_scan_missing` |
| `sast_failed` | local | `cond_sast_failed` |
| `dependency_scan_failed` | local | `cond_dependency_scan_failed` |
| `reviewer_blocked` | local | `cond_reviewer_blocked` |
| `reviewer_abstained_required` | local | `cond_reviewer_abstained_required` |
| `lockfile_only_change` | local | `cond_lockfile_only_change` |
| `prompt_injection_suspected` | local | `cond_prompt_injection_suspected` |
| `sha_drift` | external | judge supplies |
| `policy_sha_drift` | external | judge supplies |
| `missing_required_review_role` | external | judge supplies |
| `missing_evidence_pack` | external | judge supplies |
| `codeowners_not_satisfied` | external | git-host adapter supplies |
| `freeze_window_active` | external | freeze policy supplies |
| `budget_exceeded` | external | router supplies |
| `training_use_required_but_disallowed` | external | router supplies |
| `lockfile_diff_without_manifest_diff` | external | diff analyzer supplies |
| `judge_signature_invalid` | external | judge supplies |
| `changes_security_scanner_config` | external | diff analyzer supplies |
| `changes_release_or_deploy_policy` | external | diff analyzer supplies |
| `changes_agent_prompts_or_judge_policy` | external | diff analyzer supplies |
| `touches_secret_handling` | external | diff analyzer supplies |
| `destructive_database_change` | external | diff analyzer supplies |
| `removes_or_weakens_tests` | external | diff analyzer supplies |
| `introduces_new_external_code_source` | external | diff analyzer supplies |
| `dependency_count_delta_gte_5` | external | diff analyzer supplies |
| `all_files_have_targeted_tests` | external | diff analyzer supplies |

Conformance: an implementation that references a name not in this
registry must fail closed at policy-load time. See
`src/autonomy/conditions.rs:95-117` (`ConditionRegistry::evaluate`)
for the reference implementation.

### Quorum per tier

The quorum table is loaded from
`/home/ubuntu/jeryu/.jeryu/autonomy/policies/approvals.yml`. Each entry names
the reviewer roles that must emit `decision == "pass"` for the verdict
to be `allow_merge`. `human_required: true` forces `RequireHuman` even
when every reviewer passes.

```yaml
quorum:
  R0: { approvals_needed: 0, roles: [], human_required: false }
  R1: { approvals_needed: 1, roles: [test_integrity], human_required: false }
  R2: { approvals_needed: 2, roles: [test_integrity, security], human_required: false }
  R3:
    approvals_needed: 4
    roles: [test_integrity, security, runtime, lockfile]
    human_required: true
  R4: { approvals_needed: 0, roles: [], human_required: true, fail_closed_without_human: true }
  R5: { approvals_needed: 0, roles: [], human_required: true, fail_closed: true }
```

### Verdict freshness

A `VibeGateVerdict` is valid only against `(head_sha, policy_sha)` for
the TTL declared in `approvals.yml::verdict_ttl_minutes` (default 60
minutes). The verdict is automatically re-issued when any of these
re-judge triggers fire:

```yaml
re_judge_on:
  - merge_train_rebase
  - target_branch_advance
  - policy_change_on_target
  - new_commit_on_pr
```

This mechanism is how Laws 3 and 4 stay enforced through a merge train
rebase: when the queue rebases the head SHA, the verdict's
`rebind_on_train: true` flag triggers a fresh judge call before the
merge lands.

---

## Part 3 — The eight typed objects

Every durable autonomy decision is a JSON object that conforms to one
of the eight schemas below. The schemas live at
`/home/ubuntu/jeryu/.jeryu/autonomy/schemas/*.schema.json` and are loaded at
deserialization time via the `SchemaTag<T>` type — an unknown or
mismatched `schema` field is a hard parse error, not a recovery path.

The objects are presented in the order they appear in a normal
end-to-end flow.

### 3.1 Intent Card

**Why this exists.** The Intent Card is the agent's declaration of what
it intends to do *before* any code is written. It scopes the work,
records the agent's identity and version, and seeds the Launch Ledger.
Without an Intent Card there is no anchor for the Capability Lease, no
self-stated claim for reviewers to verify against, and no way to
distinguish "the agent did what it set out to do" from "the agent
quietly expanded scope mid-flight."

The Intent Card is signed by the agent's ed25519 key (currently a stub
in this implementation; full ed25519 lands in Phase 8). Estimated risk
is the agent's own guess — the real classification happens later from
the diff.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "vibegate.intent_card.v1",
  "title": "IntentCard",
  "description": "Declared intent of an agent before any code change.",
  "type": "object",
  "required": ["schema", "id", "agent_id", "repo", "summary", "created_at"],
  "additionalProperties": false,
  "properties": {
    "schema": { "const": "vibegate.intent_card.v1" },
    "id": { "type": "string", "pattern": "^intent_[0-9A-HJKMNP-TV-Z]{26}$" },
    "agent_id": { "type": "string" },
    "repo": { "type": "string" },
    "target_branch": { "type": "string" },
    "summary": { "type": "string", "maxLength": 500 },
    "linked_issue": { "type": ["string", "null"] },
    "estimated_risk": { "enum": ["R0", "R1", "R2", "R3", "R4", "R5"] },
    "expected_changed_paths": {
      "type": "array",
      "items": { "type": "string" }
    },
    "claims": {
      "type": "array",
      "description": "Human-readable claims the agent intends to prove with evidence.",
      "items": { "type": "string", "maxLength": 280 }
    },
    "created_at": { "type": "string", "format": "date-time" },
    "signature": {
      "type": "object",
      "required": ["key_id", "algo", "value"],
      "properties": {
        "key_id": { "type": "string" },
        "algo": { "const": "ed25519" },
        "value": { "type": "string", "description": "base64-encoded signature over the canonical JSON of all other fields." }
      }
    }
  }
}
```

### 3.2 Capability Lease

**Why this exists.** Agents must not hold long-lived broad credentials.
The Capability Lease is a short, signed grant from the control plane
to one agent for one task, scoped to a list of allowed actions, allowed
write refs, and denied paths. The lease answers the question "what is
this agent allowed to touch right now?" and expires automatically
(`ttl_seconds` between 60 and 14400, i.e. one minute to four hours).

A lease binds to its `intent_id`. An agent cannot reuse a lease from a
different intent, and the control plane never issues a lease whose
`allowed_write_refs` overlap with protected branches.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "vibegate.capability_lease.v1",
  "title": "CapabilityLease",
  "description": "Short-lived, scope-limited authority grant for one agent task.",
  "type": "object",
  "required": ["schema", "id", "intent_id", "agent_id", "scope", "ttl_seconds", "issued_at", "expires_at", "policy_sha", "signature"],
  "additionalProperties": false,
  "properties": {
    "schema": { "const": "vibegate.capability_lease.v1" },
    "id": { "type": "string", "pattern": "^lease_[0-9A-HJKMNP-TV-Z]{26}$" },
    "intent_id": { "type": "string", "pattern": "^intent_[0-9A-HJKMNP-TV-Z]{26}$" },
    "agent_id": { "type": "string" },
    "scope": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "allowed_actions": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Names from src/capability.rs::AgentIntent (e.g. ProposePatch, RunTests, RequestMerge, MintReceipt)."
        },
        "denied_actions": {
          "type": "array",
          "items": { "type": "string" }
        },
        "allowed_write_refs": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Git refspec patterns the agent may push to. NEVER includes protected branches."
        },
        "denied_paths": {
          "type": "array",
          "items": { "type": "string" }
        }
      }
    },
    "ttl_seconds": { "type": "integer", "minimum": 60, "maximum": 14400 },
    "issued_at": { "type": "string", "format": "date-time" },
    "expires_at": { "type": "string", "format": "date-time" },
    "policy_sha": { "type": "string", "pattern": "^[0-9a-f]{40,64}$" },
    "signature": {
      "type": "object",
      "required": ["key_id", "algo", "value"],
      "properties": {
        "key_id": { "type": "string", "description": "Typically 'autonomy-control-plane'." },
        "algo": { "const": "ed25519" },
        "value": { "type": "string" }
      }
    }
  }
}
```

### 3.3 Evidence Pack

**Why this exists.** The Evidence Pack is the center of the design.
Everything else is bookkeeping around it. An Evidence Pack is a single
machine-readable document that says, for one MR/PR: what changed (with
risk tags per file), what claims the agent makes, what tests ran, what
security and supply-chain scans say, what the rollback plan is, and a
SHA-256 digest over the whole structure for downstream binding.

Agent opinions are secondary. A reviewer is only useful because it
checks the Evidence Pack and emits a signed, exact-SHA-bound receipt.
The Evidence Pack carries a `policy_sha` field that names the
target-branch policy commit so reviewers and the judge enforce Law 3
(policy from target branch, never PR branch).

The `gate_receipts` array carries required receipt records from
`src/release/gate.rs` verbatim, binding the Evidence Pack to the
`release.policy.toml` gate.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "vibegate.evidence_pack.v1",
  "title": "EvidencePack",
  "description": "Machine-readable proof bundle for one MR/PR. Center of the Evidence Gate design.",
  "type": "object",
  "required": ["schema", "id", "repo", "source_branch", "target_branch", "head_sha", "base_sha", "policy_sha", "risk", "tests", "security", "supply_chain", "rollback", "created_at", "evidence_digest"],
  "additionalProperties": false,
  "properties": {
    "schema": { "const": "vibegate.evidence_pack.v1" },
    "id": { "type": "string", "pattern": "^evp_[0-9A-HJKMNP-TV-Z]{26}$" },
    "intent_id": { "type": ["string", "null"] },
    "repo": { "type": "string" },
    "source_branch": { "type": "string" },
    "target_branch": { "type": "string" },
    "head_sha": { "type": "string", "pattern": "^[0-9a-f]{40}$" },
    "base_sha": { "type": "string", "pattern": "^[0-9a-f]{40}$" },
    "policy_sha": { "type": "string", "pattern": "^[0-9a-f]{40,64}$", "description": "SHA of the target-branch policy bundle. Tip1 Law 3 (policy from target, never PR branch)." },
    "author_agent": { "type": ["string", "null"] },
    "risk": { "enum": ["R0", "R1", "R2", "R3", "R4", "R5"] },
    "changed_files": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["path", "risk_tags", "lines_added", "lines_removed"],
        "properties": {
          "path": { "type": "string" },
          "risk_tags": { "type": "array", "items": { "type": "string" } },
          "lines_added": { "type": "integer", "minimum": 0 },
          "lines_removed": { "type": "integer", "minimum": 0 }
        }
      }
    },
    "claims": { "type": "array", "items": { "type": "string", "maxLength": 280 } },
    "tests": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "targeted": { "type": "array", "items": { "type": "string" } },
        "full_required": { "type": "boolean" },
        "skipped": { "type": "array", "items": { "type": "string" } },
        "coverage_delta": { "type": ["number", "null"] }
      }
    },
    "security": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "sast": { "enum": ["passed", "failed", "skipped", "missing"] },
        "dependency_scan": { "enum": ["passed", "failed", "skipped", "missing"] },
        "secret_scan": { "enum": ["passed", "failed", "skipped", "missing"] }
      }
    },
    "supply_chain": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "dependency_changes": { "type": "array", "items": { "type": "object" } },
        "external_code_sources": { "type": "array", "items": { "type": "string" } },
        "lockfile_only_change": { "type": "boolean", "default": false }
      }
    },
    "rollback": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "strategy": { "enum": ["revert_commit", "feature_flag", "data_migration_reverse", "redeploy_previous", "manual"] },
        "feature_flag": { "type": ["string", "null"] },
        "data_migration_reversible": { "type": ["boolean", "null"] }
      }
    },
    "gate_receipts": {
      "type": "array",
      "description": "Slice carrying required src/release/gate.rs::Receipt entries.",
      "items": {
        "type": "object",
        "required": ["id", "status", "detail"],
        "properties": {
          "id": { "type": "string" },
          "status": { "enum": ["pass", "fail", "skipped", "pending"] },
          "detail": { "type": "string" },
          "evidence": { "type": ["string", "null"] }
        }
      }
    },
    "evidence_digest": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" },
    "created_at": { "type": "string", "format": "date-time" },
    "signature": {
      "type": "object",
      "required": ["key_id", "algo", "value"],
      "properties": {
        "key_id": { "type": "string" },
        "algo": { "const": "ed25519" },
        "value": { "type": "string" }
      }
    }
  }
}
```

### 3.4 Agent Approval Receipt

**Why this exists.** A receipt is one reviewer's signed verdict over
one Evidence Pack. There is exactly one receipt per (reviewer role,
Evidence Pack) pair per attempt. Each receipt carries the reviewer's
`agent_id` and version, the `prompt_sha` of the system prompt it ran
under, the `provider` and `model` it used, the temperature and seed,
and a SHA-256 of the raw model response. This is the audit surface that
makes contested approvals replayable: re-running the same agent with
the same prompt and seed on the same Evidence Pack should produce the
same finding set.

Receipts also record the head SHA and policy SHA they were issued
against. The judge drops any receipt whose SHAs do not match the
Evidence Pack at fusion time. See `src/agent_review/judge.rs:44-52`
for the SHA-bind filter.

The `not_author` field is always `true` for valid receipts and is how
the no-self-approval rule (Law 2) is enforced at fusion time.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "vibegate.agent_approval_receipt.v1",
  "title": "AgentApprovalReceipt",
  "description": "One reviewer's signed verdict over one EvidencePack. Compromising one reviewer cannot escalate: veto > approval count.",
  "type": "object",
  "required": ["schema", "id", "evidence_pack_id", "role", "agent_id", "decision", "head_sha", "policy_sha", "created_at", "signature"],
  "additionalProperties": false,
  "properties": {
    "schema": { "const": "vibegate.agent_approval_receipt.v1" },
    "id": { "type": "string", "pattern": "^aar_[0-9A-HJKMNP-TV-Z]{26}$" },
    "evidence_pack_id": { "type": "string", "pattern": "^evp_[0-9A-HJKMNP-TV-Z]{26}$" },
    "role": { "enum": ["security", "test_integrity", "runtime", "lockfile", "judge", "release_shepherd", "nightwatch"] },
    "agent_id": { "type": "string", "description": "Includes version, e.g. reviewer-security.v1" },
    "prompt_sha": { "type": ["string", "null"], "pattern": "^[0-9a-f]{64}$" },
    "provider": { "type": ["string", "null"] },
    "model": { "type": ["string", "null"] },
    "temperature": { "type": ["number", "null"] },
    "seed": { "type": ["integer", "null"] },
    "raw_response_sha": { "type": ["string", "null"], "pattern": "^sha256:[0-9a-f]{64}$" },
    "head_sha": { "type": "string", "pattern": "^[0-9a-f]{40}$" },
    "policy_sha": { "type": "string", "pattern": "^[0-9a-f]{40,64}$" },
    "decision": { "enum": ["pass", "concern", "block", "abstain"] },
    "reason": { "type": ["string", "null"], "maxLength": 1000 },
    "findings": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["severity", "class", "file", "range"],
        "additionalProperties": false,
        "properties": {
          "severity": { "enum": ["info", "low", "medium", "high", "critical"] },
          "class": { "type": "string" },
          "file": { "type": "string" },
          "range": {
            "type": "array",
            "items": { "type": "integer", "minimum": 1 },
            "minItems": 2,
            "maxItems": 2
          },
          "evidence": { "type": "string", "maxLength": 500 },
          "recommendation": { "type": "string", "maxLength": 500 }
        }
      }
    },
    "not_author": { "type": "boolean", "default": true, "description": "Always true for the reviewer; no-self-approval rule." },
    "tokens": {
      "type": "object",
      "properties": {
        "prompt": { "type": "integer", "minimum": 0 },
        "completion": { "type": "integer", "minimum": 0 }
      }
    },
    "created_at": { "type": "string", "format": "date-time" },
    "signature": {
      "type": "object",
      "required": ["key_id", "algo", "value"],
      "properties": {
        "key_id": { "type": "string" },
        "algo": { "const": "ed25519" },
        "value": { "type": "string" }
      }
    }
  }
}
```

### 3.5 VibeGate Verdict

**Why this exists.** The Verdict is the fused decision over an Evidence
Pack and its receipts, emitted by the Judge agent. The Judge is pure
policy fusion: it never reads code, never calls an LLM, never makes a
discretionary judgement. It walks the hard-stop list against the
condition registry, then evaluates quorum, then emits one of
`allow_merge`, `require_human`, or `reject`.

Because the Judge has no LLM and no discretion, compromising one
reviewer cannot escalate. Three reviewers voting "pass" cannot overrule
one valid `reviewer_blocked` hit. This is the **veto-not-average** trust
property described in tip1 section 7.

The Verdict's `key_id` is fixed at `judge.ed25519` (see schema below).
That is the single signing identity that downstream platform layers
require to honor a verdict as a passing status check.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "vibegate.gate_verdict.v1",
  "title": "VibeGateVerdict",
  "description": "Fused decision from hard rules + reviewer receipts. Emitted by the Judge (pure policy, no LLM).",
  "type": "object",
  "required": ["schema", "id", "evidence_pack_id", "head_sha", "policy_sha", "risk", "decision", "expires_at", "created_at", "signature"],
  "additionalProperties": false,
  "properties": {
    "schema": { "const": "vibegate.gate_verdict.v1" },
    "id": { "type": "string", "pattern": "^vgv_[0-9A-HJKMNP-TV-Z]{26}$" },
    "evidence_pack_id": { "type": "string" },
    "merge_request": { "type": ["string", "null"], "description": "e.g. '!123' on GitLab or '#123' on GitHub" },
    "repo": { "type": "string" },
    "target_branch": { "type": "string" },
    "head_sha": { "type": "string", "pattern": "^[0-9a-f]{40}$" },
    "policy_sha": { "type": "string", "pattern": "^[0-9a-f]{40,64}$" },
    "evidence_pack_digest": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" },
    "risk": { "enum": ["R0", "R1", "R2", "R3", "R4", "R5"] },
    "hard_stops": {
      "type": "array",
      "description": "Names of triggered conditions from src/autonomy/conditions.rs registry. Any non-empty list = block.",
      "items": { "type": "string" }
    },
    "required_reviews": {
      "type": "array",
      "items": { "enum": ["security", "test_integrity", "runtime", "lockfile"] }
    },
    "approval_receipts": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["role", "agent_id", "receipt_digest", "decision", "not_author"],
        "properties": {
          "role": { "type": "string" },
          "agent_id": { "type": "string" },
          "receipt_digest": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" },
          "decision": { "enum": ["pass", "concern", "block", "abstain"] },
          "not_author": { "type": "boolean" }
        }
      }
    },
    "decision": { "enum": ["allow_merge", "require_human", "reject"] },
    "valid_for_head_sha_only": { "const": true },
    "rebind_on_train": { "type": "boolean", "default": true },
    "expires_at": { "type": "string", "format": "date-time" },
    "created_at": { "type": "string", "format": "date-time" },
    "signature": {
      "type": "object",
      "required": ["key_id", "algo", "value"],
      "properties": {
        "key_id": { "const": "judge.ed25519" },
        "algo": { "const": "ed25519" },
        "value": { "type": "string" }
      }
    }
  }
}
```

### 3.6 Merge Passport

**Why this exists.** A Verdict says "this exact SHA is allowed to
merge." A Merge Passport carries that authorization forward into the
merge queue. Merge trains rebase queued MRs against the latest target;
the passport's `rebind_on_train: true` flag tells the platform adapter
to re-judge the rebased head before allowing the merge to land. This
closes the "approved the PR branch but not the merge-result branch"
gap described in tip1 section 16.

The passport carries optional `conditions` that must still hold at
merge time (for example `no_new_commits_after_verdict`), and records
the final `merge_sha` post-merge for the Launch Ledger.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "vibegate.merge_passport.v1",
  "title": "MergePassport",
  "description": "Exact-SHA authorization to enter the merge queue / merge train.",
  "type": "object",
  "required": ["schema", "id", "verdict_id", "head_sha", "target_branch", "issued_at", "expires_at", "signature"],
  "additionalProperties": false,
  "properties": {
    "schema": { "const": "vibegate.merge_passport.v1" },
    "id": { "type": "string", "pattern": "^mpa_[0-9A-HJKMNP-TV-Z]{26}$" },
    "verdict_id": { "type": "string" },
    "repo": { "type": "string" },
    "merge_request": { "type": "string" },
    "head_sha": { "type": "string", "pattern": "^[0-9a-f]{40}$" },
    "target_branch": { "type": "string" },
    "conditions": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Named conditions that must still hold at merge time (e.g. 'no_new_commits_after_verdict')."
    },
    "rebind_on_train": { "type": "boolean", "default": true },
    "merge_sha": { "type": ["string", "null"], "pattern": "^[0-9a-f]{40}$", "description": "Populated post-merge for the ledger." },
    "issued_at": { "type": "string", "format": "date-time" },
    "expires_at": { "type": "string", "format": "date-time" },
    "consumed_at": { "type": ["string", "null"], "format": "date-time" },
    "signature": {
      "type": "object",
      "required": ["key_id", "algo", "value"],
      "properties": {
        "key_id": { "const": "judge.ed25519" },
        "algo": { "const": "ed25519" },
        "value": { "type": "string" }
      }
    }
  }
}
```

### 3.7 Release Passport

**Why this exists.** A Release Passport authorizes one *artifact*
(not one commit) to flow through one or more environments. It binds
the artifact's content-addressed digest to its source SHA, SBOM digest,
SLSA provenance digest, and build-log digest. This is how Law 6 (build
once, promote the same artifact) is made explicit: every environment in
`allowed_environments` deploys the exact same `artifact_digest`.

The `rollback_plan` is required and must declare a strategy.
Production deploys additionally require `rollback_plan.tested == true`,
which enforces Law 7 (autonomous production requires automatic
rollback).

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "vibegate.release_passport.v1",
  "title": "ReleasePassport",
  "description": "Artifact-level authorization to deploy/promote. Built once, promoted through environments.",
  "type": "object",
  "required": ["schema", "id", "artifact_digest", "sbom_digest", "provenance_digest", "source_sha", "build_logs_digest", "issued_at", "signature"],
  "additionalProperties": false,
  "properties": {
    "schema": { "const": "vibegate.release_passport.v1" },
    "id": { "type": "string", "pattern": "^rpa_[0-9A-HJKMNP-TV-Z]{26}$" },
    "release_id": { "type": ["string", "null"] },
    "artifact_digest": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" },
    "artifact_kind": { "enum": ["container", "rust_binary", "wasm_module", "deb", "rpm", "tarball"] },
    "sbom_digest": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" },
    "provenance_digest": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" },
    "source_sha": { "type": "string", "pattern": "^[0-9a-f]{40}$" },
    "build_logs_digest": { "type": "string", "pattern": "^sha256:[0-9a-f]{64}$" },
    "allowed_environments": {
      "type": "array",
      "items": { "enum": ["dev", "staging", "canary", "prod"] }
    },
    "rollback_plan": {
      "type": "object",
      "required": ["strategy"],
      "properties": {
        "strategy": { "enum": ["revert_artifact", "feature_flag", "data_migration_reverse"] },
        "tested": { "type": "boolean", "default": false }
      }
    },
    "issued_at": { "type": "string", "format": "date-time" },
    "signature": {
      "type": "object",
      "required": ["key_id", "algo", "value"],
      "properties": {
        "key_id": { "type": "string", "description": "Typically 'release-shepherd.ed25519'." },
        "algo": { "const": "ed25519" },
        "value": { "type": "string" }
      }
    }
  }
}
```

### 3.8 Launch Ledger Entry

**Why this exists.** Every durable autonomy decision writes one ledger
entry. The ledger is append-only and searchable. It is how the
"Bypass Ledger" and "Autonomy Overview" dashboards from tip1 section 20
are populated, and how every decision in the system can be replayed
and audited months later.

The `kind` enum is closed: a conforming implementation must not invent
new kinds without bumping the schema version. The `subject_id` ties
each entry back to its primary object (Intent Card, Evidence Pack,
Verdict, etc.). The signature ties the entry to the actor that
produced it (agent id or human handle).

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "vibegate.launch_ledger_entry.v1",
  "title": "LaunchLedgerEntry",
  "description": "One append-only event in the Launch Ledger. All durable autonomy decisions write one of these.",
  "type": "object",
  "required": ["schema", "id", "kind", "subject_id", "recorded_at", "actor", "signature"],
  "additionalProperties": false,
  "properties": {
    "schema": { "const": "vibegate.launch_ledger_entry.v1" },
    "id": { "type": "string", "pattern": "^lle_[0-9A-HJKMNP-TV-Z]{26}$" },
    "kind": {
      "enum": [
        "intent_declared",
        "lease_issued",
        "lease_expired",
        "evidence_pack_created",
        "review_started",
        "review_completed",
        "verdict_issued",
        "merge_passport_issued",
        "merge_passport_consumed",
        "merge_passport_invalidated",
        "release_passport_issued",
        "deployment_started",
        "deployment_promoted",
        "rollback_initiated",
        "rollback_completed",
        "human_escalation_requested",
        "human_decision_recorded",
        "autonomy_pack_edit_proposed",
        "autonomy_pack_edit_merged"
      ]
    },
    "subject_id": { "type": "string", "description": "The id of the object this event is about (intent_*, lease_*, evp_*, aar_*, vgv_*, mpa_*, rpa_*)." },
    "repo": { "type": ["string", "null"] },
    "payload": {
      "type": "object",
      "description": "Free-form per-kind payload; the wrapping signature still applies."
    },
    "recorded_at": { "type": "string", "format": "date-time" },
    "actor": { "type": "string", "description": "Agent id or human handle." },
    "signature": {
      "type": "object",
      "required": ["key_id", "algo", "value"],
      "properties": {
        "key_id": { "type": "string" },
        "algo": { "const": "ed25519" },
        "value": { "type": "string" }
      }
    }
  }
}
```

---

## Appendix A — Identifier prefixes

Every typed object's `id` has a stable three-letter prefix so logs and
the Launch Ledger remain greppable. Reference implementations must use
these exact prefixes.

| Object | Prefix | Pattern (after prefix) |
| --- | --- | --- |
| Intent Card | `intent_` | 26-char Crockford base32 |
| Capability Lease | `lease_` | 26-char Crockford base32 |
| Evidence Pack | `evp_` | 26-char Crockford base32 |
| Agent Approval Receipt | `aar_` | 26-char Crockford base32 |
| VibeGate Verdict | `vgv_` | 26-char Crockford base32 |
| Merge Passport | `mpa_` | 26-char Crockford base32 |
| Release Passport | `rpa_` | 26-char Crockford base32 |
| Launch Ledger Entry | `lle_` | 26-char Crockford base32 |

The schemas pin these with regex `^<prefix>_[0-9A-HJKMNP-TV-Z]{26}$`.

## Appendix B — Signature format

Every typed object carries a `signature` block:

```json
{
  "key_id": "<role>.ed25519",
  "algo":   "ed25519",
  "value":  "<base64-encoded signature over canonical JSON of all other fields>"
}
```

In the current implementation the signing is a SHA-256 HMAC stub. Real
ed25519 lands in Phase 8 alongside `ed25519-dalek` and per-agent key
custody in `.jeryu/autonomy/keys/`. Until then, the
`evidence_signature_invalid` and `judge_signature_invalid` hard-stops
fire whenever the algo is `stub`, which forces enforcement mode to
fail closed. See `src/autonomy/conditions.rs:133-147` for the check.

## Appendix C — Cross-reference: laws to schemas

| Law | Schemas that enforce it |
| --- | --- |
| Law 1 | Capability Lease (`scope.allowed_write_refs` never includes protected branches) |
| Law 2 | Agent Approval Receipt (`not_author: true`), Judge quorum check |
| Law 3 | Evidence Pack (`policy_sha`), Verdict (`policy_sha`) |
| Law 4 | Evidence Pack (`head_sha`, `evidence_digest`), Receipt (`head_sha`, `policy_sha`), Verdict (`head_sha`, `policy_sha`, `evidence_pack_digest`), Merge Passport (`head_sha`) |
| Law 5 | Verdict (`hard_stops` list) |
| Law 6 | Release Passport (`artifact_digest`, `allowed_environments`) |
| Law 7 | Release Passport (`rollback_plan.strategy`, `rollback_plan.tested`) |

## Appendix D — Reading order for implementers

If you are building a new conforming implementation, read in this order:

1. The seven laws in Part 1.
2. The risk model in Part 2 (especially the named-condition registry).
3. Evidence Pack (3.3) first, then Agent Approval Receipt (3.4), then
   VibeGate Verdict (3.5) — the data flow runs in that order.
4. Intent Card (3.1) and Capability Lease (3.2) — these wrap the flow
   on the input side.
5. Merge Passport (3.6) and Release Passport (3.7) — these wrap the
   flow on the output side.
6. Launch Ledger Entry (3.8) — the audit substrate everything writes
   into.

Once the data shapes are in your head, read the narrative overview in
[`docs/autonomous-delivery.md`](./autonomous-delivery.md) for how they
flow together, and the per-role behavior in
[`docs/llm-reviewers.md`](./llm-reviewers.md) for how reviewers produce
receipts.

## Appendix E — Visible required check name

The Evidence Gate posts **exactly one** required status check to the git
host per PR. Its canonical name is, character for character:

```
vibegate/merge-passport
```

This is locked in code by the
`VIBEGATE_MERGE_PASSPORT_CHECK_NAME` constant in
`/home/ubuntu/jeryu/src/git_host/mod.rs` and asserted by the lib test
`git_host::github::tests::merge_passport_check_name_is_canonical`.

Any deviation from this exact string — `vibegate/passport`,
`merge-passport`, `vibegate-merge-passport`, capitalization changes,
extra prefixes — is a **spec violation**. The string is part of the
contract with every consumer's GitHub branch-protection rule; renaming
it silently disables required-status enforcement across the fleet.

The `GateDecision → check status` mapping is also normative and is
enforced by `gate_decision_to_check_status` in
`src/git_host/github.rs`:

| `GateDecision` | host status |
| --- | --- |
| `AllowMerge` | `success` |
| `RequireHuman` | `action_required` |
| `Reject` | `failure` |

Internal per-reviewer or per-approval check-runs may exist for debug
purposes but **MUST NOT** be wired into branch protection. The org-level
GitHub setup that materializes this invariant is documented in
[`docs/autonomous-delivery.md`](./autonomous-delivery.md) under
"Required Check Setup (GitHub)".

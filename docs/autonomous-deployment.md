# Autonomous Deployment with jeryu

A complete guide to using jeryu's Evidence Gate, autonomous decision-making, and policy-driven deployment pipeline to ship code to production with minimal human intervention.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Key Concepts](#key-concepts)
3. [Configuration & Setup](#configuration--setup)
4. [Deployment Scenarios](#deployment-scenarios)
5. [Examples](#examples)
6. [Monitoring & Rollback](#monitoring--rollback)
7. [Troubleshooting](#troubleshooting)

---

## Architecture Overview

jeryu is a **single-binary CI/CD control plane** that orchestrates autonomous release decisions using:

- **Evidence Gate**: A security-first review spine that collects structured evidence (test results, supply-chain scans, performance metrics, security findings) and routes it to reviewers (human or LLM).
- **Autonomous Judgment**: LLM-powered or rule-based decision engines (called "judges") that evaluate evidence against policy and issue structured verdicts.
- **Release Pipeline**: A batching train system that groups commits into release candidates, applies approval gates, and promotes to production.
- **Kill Bell**: A global pause mechanism that instantly freezes all deployments when security or operational issues are detected.
- **Ledger**: An append-only event log of all decisions, approvals, and deployments for audit and forensics.
- **Event bus**: Embedded [Jansu](https://github.com/neverhuman/jansu) broker decouples the HTTP webhook handler from inline event work. The webhook handler returns `202 Accepted` after enqueueing; a consumer loop in the autonomy daemon drains records into the existing dispatch path. In-process by design — no separate Kafka/queue infra to operate. Set `JERYU_WEBHOOK_SYNC=1` to force the legacy synchronous path for ops/debug. Compile with `--no-default-features` to drop the broker entirely.
- **Storage**: SQLite via [sqlx](https://github.com/launchbadge/sqlx) today, migrating to [RedlineDB](https://github.com/neverhuman/RedlineDB) (100% Rust SQLite-compatible engine) in staged Wave 11.C+ work. The async wrapper crate [`redlinedb-tokio`](https://github.com/neverhuman/RedlineDB/pull/7) is the migration foundation — see `docs/redline-jansu-issues.md::R-1` for the staged plan.

### Key Components

```
┌─────────────────────────────────────────────────────────────┐
│                    Webhook Entry Point                      │
│         (GitHub Push / Merge / GitLab Pipeline)            │
│   POST /hooks → returns 202 Accepted after enqueueing       │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│         Embedded Jansu Broker (in-process, no TCP)          │
│  Topics: jeryu.webhook.{jobs,pipelines,pushes}              │
│  Message key: X-Gitlab-Webhook-UUID (idempotent redelivery) │
│  Feature: jansu-broker (default-on, see Cargo.toml)         │
└──────────────────────┬──────────────────────────────────────┘
                       │ consumer-loop drains records
                       ▼
┌─────────────────────────────────────────────────────────────┐
│           Evidence Collector (Autonomy Module)              │
│  - Test Results  - Security Scans  - Performance Metrics    │
│  - Dependency Audits  - Code Coverage  - VTI Selection      │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│          Judge (LLM or Rule-Based Decision Engine)          │
│  - Reads evidence + policy  - Issues verdict (approve/hold) │
│  - Logs reasoning + confidence score                        │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│         Approval Gate (Human or Policy Override)            │
│  - Escalation to on-call  - Policy-driven auto-approve      │
│  - Kill Bell pause / resume                                 │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│       Release Train (Batching + Environment Promotion)      │
│  - Candidate queuing  - Canary promotion  - Rollback        │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│            Docker / Kubernetes / Infrastructure             │
└─────────────────────────────────────────────────────────────┘
```

---

## Key Concepts

### Evidence Pack

A data structure containing all relevant signals about a release candidate:

```json
{
  "id": "evp_abc123def456",
  "source_sha": "abc123def...",
  "tests": {
    "passed": 487,
    "failed": 0,
    "coverage_pct": 89.2
  },
  "security": {
    "cves_found": 0,
    "supply_chain_risk": "low",
    "secrets_scan": "clean"
  },
  "performance": {
    "p99_latency_ms": 142,
    "throughput_rps": 8200
  },
  "dependencies": {
    "direct_count": 32,
    "transitive_count": 487,
    "outdated": 2
  }
}
```

### Policy Bundle

A YAML document defining decision rules and thresholds:

```yaml
# Approval policy: what makes a release auto-approvable
approval:
  quorum:
    R0:  # Risk tier 0 (lowest)
      human_required: false
      approvals_needed: 0  # Autonomous!
      policy_override:
        - >
          tests.coverage_pct >= 85 AND
          security.cves_found == 0 AND
          security.supply_chain_risk in ["low", "minimal"]
    R1:  # Risk tier 1 (moderate)
      human_required: false
      approvals_needed: 1
      policy_override:
        - >
          tests.coverage_pct >= 80 AND
          security.cves_found <= 1 AND
          security.supply_chain_risk != "critical"
    R2:  # Risk tier 2 (high)
      human_required: true
      approvals_needed: 2

# Risk classification: which commits map to which tier
risk:
  classify:
    high_risk_paths:
      - "src/secrets/**"
      - "src/exec/**"
      - "src/sandbox/**"
    moderate_risk_paths:
      - "src/autonomy/**"
      - "src/release/**"
    default_tier: "R1"

# Escalation: when to interrupt humans
escalation:
  on_events:
    - "security_finding_critical"
    - "kill_bell_engaged"
    - "performance_regression_exceeds_10pct"
```

### Verdict

The judge's structured output:

```json
{
  "id": "vrd_xyz789",
  "evidence_pack_id": "evp_abc123def456",
  "decision": "Approve",
  "confidence_score": 0.94,
  "reasoning": "All evidence thresholds met: tests (89.2% coverage), security (0 CVEs), supply chain (low risk)",
  "tier": "R0",
  "issued_at": "2026-05-16T14:32:00Z",
  "issuer": "evidence-gate-v1.4.1"
}
```

### Kill Bell

A **global pause mechanism** that stops all deployments immediately:

```rust
// Anywhere in the stack can trigger:
kill_bell.pause("Security advisory XYZ requires immediate review", 3600); // 1 hour pause
kill_bell.resume("Advisory mitigated, deployments resume");
```

When paused:
- All pending release candidates are held
- No new promotions to canary or prod
- On-call is automatically notified
- Decision logs are frozen (audit trail preserved)

---

## Configuration & Setup

### 1. `.autonomy/autonomy.yml` (Global Profile)

```yaml
profile: "sovereign_plus"  # Full autonomous control (no human gates by default)

providers:
  llm:
    default: "openrouter"
    fallback: "rule_based"  # If LLM is unavailable, use hardcoded rules

evidence:
  # Which signals to collect before judging
  required:
    - test_results
    - security_scan
    - supply_chain_audit
  optional:
    - performance_baseline
    - dependency_check

judge:
  # Decision engine configuration
  engine: "vibe_gate_v1"
  timeout_sec: 30
  retry_attempts: 2

escalation:
  enabled: true
  on_call_channel: "slack://jeryu-oncall"
  pagerduty_integration: true
  critical_threshold: "security_critical"

kill_bell:
  enabled: true
  default_pause_sec: 3600
  require_human_resume: false  # Autonomous resume after timeout
```

### 2. `.autonomy/policies/approvals.yml` (Approval Rules)

Define quorum and auto-approval thresholds:

```yaml
---
version: "1.0"
description: "Approval gates for R0/R1/R2 risk tiers"

approvals:
  R0:
    # Tier 0: Cosmetic, test-only, docs changes
    human_required: false
    quorum_needed: 0
    auto_approve_rule:
      - type: "path_match"
        pattern: "^(docs|tests|README|CHANGELOG)/"
      - type: "criteria_match"
        expression: |
          tests.passed == tests.total AND
          tests.coverage_pct >= 85 AND
          security.cves_found == 0

  R1:
    # Tier 1: Core library changes, safe refactors
    human_required: false
    quorum_needed: 1
    auto_approve_rule:
      - type: "criteria_match"
        expression: |
          tests.passed == tests.total AND
          tests.coverage_pct >= 80 AND
          security.supply_chain_risk in ["low", "minimal"] AND
          performance.p99_latency_change_pct <= 5

  R2:
    # Tier 2: Security, storage, deployment logic
    human_required: true
    quorum_needed: 2
    approval_timeout_sec: 7200  # 2 hours
    escalation_channels:
      - "slack://security-team"
      - "pagerduty"
```

### 3. `.autonomy/policies/risk.yml` (Risk Classification)

Map file paths to risk tiers:

```yaml
---
version: "1.0"
description: "Risk tier assignment for commit paths"

risk_tiers:
  R0:  # Cosmetic
    paths:
      - "docs/**"
      - "README.md"
      - "CHANGELOG.md"
      - "tests/**"
    description: "No production code impact"

  R1:  # Moderate
    paths:
      - "src/cache/**"
      - "src/tui/**"
      - "src/gateway/**"
    description: "Internal APIs, observability, non-critical services"

  R2:  # High
    paths:
      - "src/secrets/**"
      - "src/exec/**"
      - "src/sandbox/**"
      - "src/release/**"
      - "src/autonomy/**"
    description: "Security, privilege escalation, release logic"

default_tier: "R1"
```

### 4. `.jeryu/jeryu-delivery.yml` (dougx Integration)

Tell jeryu's daemon how to deploy dougx:

```yaml
---
version: "1.0"

pipelines:
  dougx_release:
    repo: "veox-ai/dougx"
    trigger: "push:main"
    
    intake:
      # Autonomy mode gate: require PR body or label to declare mode
      gate: "autonomy_mode"
      modes:
        - "autonomous"      # Let agents decide
        - "human_driven"    # Require human approval
        - "canary_only"     # Deploy to staging, pause before prod
      default: "autonomous"

    evidence:
      collect:
        - type: "github_checks"
          include: ["build", "test", "lint"]
        - type: "security_scan"
          tools: ["cargo-deny", "trivy"]
        - type: "performance"
          baseline: "main"

    judge:
      policy_path: ".autonomy/policies/approvals.yml"
      escalate_on:
        - "test_failure"
        - "security_finding_critical"

    release:
      environments:
        - name: "staging"
          approval: "automatic"
        - name: "canary"
          approval: "policy"
          canary_traffic_pct: 10
        - name: "production"
          approval: "policy"
          rollback_on:
            - "error_rate_exceeds_1pct"
            - "latency_exceeds_2x_baseline"
```

---

## Deployment Scenarios

### Scenario 1: High-Trust R0 Changes (100% Autonomous)

**Trigger**: A developer pushes a documentation update.

```
Developer pushes: docs/deployment.md
                 ↓
Evidence Collector: runs tests (passes), checks security (clean)
                 ↓
Judge evaluates: "path is docs/*, tests pass, security clean"
                 ↓
Verdict: "Approve (confidence: 0.98)"
                 ↓
Approval gate: POLICY_OVERRIDE triggered (R0 rule matched)
                 ↓
Release train: candidate enqueued, departs immediately
                 ↓
Production: deployed autonomously, zero human touchpoints
```

**Result**: Documentation live in <5 minutes, no Slack messages, no human approval.

---

### Scenario 2: Moderate-Risk R1 Change (Agent-Driven with Guardrails)

**Trigger**: A developer opens a PR touching `src/cache/` (refactor).

```
GitHub PR opened: "refactor cache eviction logic"
                 ↓
CI runs: tests (94% coverage), clippy (clean), deny (clean)
                 ↓
Evidence Pack created:
  - tests.coverage_pct: 94
  - security.cves_found: 0
  - performance.p99_latency_change: +2%  (within 5% threshold)
                 ↓
Judge (LLM) reads policy + evidence:
  "R1 requires: coverage ≥80 (✓), no CVEs (✓), latency ≤5% (✓)"
                 ↓
Verdict: "Approve (confidence: 0.87)"
                 ↓
Approval gate: auto-approve rule matched, no quorum needed
                 ↓
Release train: candidate batches with 1-2 other R1/R0 changes
                 ↓
Canary: 10% traffic for 5 minutes, error rate monitored
                 ↓
Production: promoted automatically if canary is clean
```

**Result**: Code ships to prod in ~20 minutes, LLM reasoning logged, humans notified (read-only).

---

### Scenario 3: High-Risk R2 Change (Security Code, Requires Human)

**Trigger**: A developer opens a PR touching `src/secrets/` and `src/exec/`.

```
GitHub PR opened: "add hardware security module support"
                 ↓
CI runs: tests (91% coverage), security scan (finds 1 low CVE), deny (1 advisory)
                 ↓
Evidence Pack created:
  - tests: passed
  - security.cves_found: 1 (low severity)
  - supply_chain_risk: "low"
  - exec_paths_changed: true
                 ↓
Risk classifier: "This commit touches src/secrets/ AND src/exec/ → R2 (high)"
                 ↓
Judge (LLM) reads policy:
  "R2 requires human_required=true, quorum_needed=2"
                 ↓
Verdict: "Escalate to human (reason: high-risk paths, requires quorum)"
                 ↓
Escalation:
  - Slack message to #security-team with evidence summary
  - PagerDuty incident created
  - 2-hour timeout window starts (auto-reject if not approved)
                 ↓
On-call security engineer:
  1. Reviews evidence in jeryu TUI
  2. Reads PR description + linked threat model
  3. Approves (click "Review Passed" in jeryu TUI)
                 ↓
Second reviewer:
  1. Reads evidence + first approval comment
  2. Approves (click "Sign Off")
                 ↓
Approval gate: quorum = 2 (satisfied)
                 ↓
Release train: candidate enqueued
                 ↓
Canary: 5% traffic for 15 minutes (extended observation period)
                 ↓
Production: promoted with audit trail linking to both reviewer approvals
```

**Result**: Code ships to prod in ~40 minutes (includes review + extended canary), full audit trail captured.

---

## Examples

### Example 1: 100% Autonomous Production Deployment

**Goal**: Set up jeryu to deploy all R0/R1 changes to production without human approval, and escalate R2 changes to on-call (but resume automatically after 2 hours if no response).

**Configuration Files**:

**`.autonomy/autonomy.yml`**:
```yaml
profile: "sovereign_plus"

providers:
  llm:
    default: "openrouter"
    fallback: "rule_based"

evidence:
  required:
    - test_results
    - security_scan
    - supply_chain_audit

judge:
  engine: "vibe_gate_v1"
  timeout_sec: 30
  retry_attempts: 2

escalation:
  enabled: true
  on_call_channel: "slack://oncall"
  critical_threshold: "security_critical"

kill_bell:
  enabled: true
  default_pause_sec: 3600
  require_human_resume: false  # Auto-resume after timeout
```

**`.autonomy/policies/approvals.yml`**:
```yaml
---
version: "1.0"

approvals:
  R0:
    human_required: false
    quorum_needed: 0
    auto_approve_rule:
      - type: "criteria_match"
        expression: |
          tests.coverage_pct >= 85 AND
          security.cves_found == 0

  R1:
    human_required: false
    quorum_needed: 0  # <-- KEY: R1 is autonomous
    auto_approve_rule:
      - type: "criteria_match"
        expression: |
          tests.coverage_pct >= 80 AND
          tests.passed == tests.total AND
          security.cves_found <= 1

  R2:
    human_required: true
    quorum_needed: 1
    approval_timeout_sec: 7200  # 2-hour window
    escalation_channels:
      - "slack://security-team"
    auto_reject_on_timeout: false  # Resume after 2 hours
```

**`.autonomy/policies/risk.yml`**:
```yaml
---
version: "1.0"

risk_tiers:
  R0:
    paths:
      - "docs/**"
      - "tests/**"
      - "README.md"

  R1:
    paths:
      - "src/tui/**"
      - "src/cache/**"
      - "src/gateway/**"

  R2:
    paths:
      - "src/secrets/**"
      - "src/exec/**"
      - "src/sandbox/**"
      - "src/autonomy/**"
      - "src/release/**"

default_tier: "R1"
```

**Result**: 
- All R0 changes deploy autonomously within 5 minutes.
- All R1 changes deploy autonomously within 20 minutes (canary 5 min + prod).
- R2 changes escalate to on-call; if no approval within 2 hours, kill bell resumes and deployment proceeds (risky but transparent).

---

### Example 2: Conservative Deployment (Human-In-Loop for R1+)

**Goal**: Human reviewers approve all R1+ changes before production, but R0 is autonomous.

**`.autonomy/policies/approvals.yml`**:
```yaml
---
version: "1.0"

approvals:
  R0:
    human_required: false
    quorum_needed: 0
    auto_approve_rule:
      - type: "criteria_match"
        expression: |
          tests.coverage_pct >= 85 AND
          security.cves_found == 0

  R1:
    human_required: true  # <-- KEY: humans required
    quorum_needed: 1
    approval_timeout_sec: 3600  # 1 hour, then escalate/reject

  R2:
    human_required: true
    quorum_needed: 2
    approval_timeout_sec: 7200
```

**Result**:
- R0 deploys autonomously.
- R1/R2 wait for human approval in jeryu TUI; if no approval within timeout, escalate or auto-reject.

---

### Example 3: Canary-First with Rollback

**Goal**: All changes (R0-R2) go through an extended canary period with automatic rollback on error spike.

**`.jeryu/jeryu-delivery.yml`**:
```yaml
---
pipelines:
  my_service:
    repo: "myorg/myservice"
    
    release:
      environments:
        - name: "canary"
          approval: "policy"
          canary_traffic_pct: 20  # 20% traffic
          canary_duration_sec: 600  # 10 minutes
          rollback_on:
            - "error_rate_exceeds_0.5pct"
            - "latency_p99_exceeds_2x_baseline"
            - "cpu_exceeds_90pct"
          
        - name: "production"
          approval: "policy"
          full_rollout: true
          production_soak_sec: 300  # 5 min monitoring before declaring success
```

**Result**: Every change bakes in canary for 10 minutes; if error rate spikes, instant rollback. Only proceeds to prod if canary is clean for full duration.

---

## Monitoring & Rollback

### Real-Time Dashboard (jeryu TUI)

Launch the TUI to see:
- **Release tab**: Current train candidates, their evidence, verdicts, approval status
- **Delivery tab**: Release pipeline stages (intake → canary → prod), with per-change status
- **Agents tab**: Evidence collectors and judges, their health, last execution time
- **Mission tab**: High-level metrics (% autonomous vs. escalated, deployment velocity, rollback rate)

```bash
jeryu tui
```

### Viewing Verdicts

```bash
# List recent verdicts
jeryu verdicts list --limit 20

# Show detailed verdict (with reasoning)
jeryu verdicts show <verdict-id>

# Export verdicts as JSON for external auditing
jeryu verdicts export --format json --output verdicts.jsonl
```

### Triggering Kill Bell (Emergency Pause)

```bash
# Pause all deployments immediately
jeryu kill-bell pause --reason "Security advisory CVE-2026-1234"

# Resume deployments
jeryu kill-bell resume --reason "Advisory mitigated"

# Check current status
jeryu kill-bell status
```

### Rollback

```bash
# Rollback a specific release to the previous version
jeryu release rollback <release-id> --reason "Performance regression detected"

# View rollback history
jeryu release history <service-name>
```

---

## Troubleshooting

### Issue: Judge Returns "Escalate" for R0 Changes

**Symptom**: Documentation changes are being escalated to humans instead of auto-approving.

**Diagnosis**:
1. Check the evidence pack: `jeryu verdicts show <verdict-id> --verbose`
2. Verify the policy rule matches the evidence:
   ```bash
   # Is the coverage threshold being met?
   echo "Coverage: $(git diff origin/main --stat | grep test | awk '{print $NF}')"
   ```

**Fix**: 
- Adjust the auto-approve threshold in `.autonomy/policies/approvals.yml`:
  ```yaml
  R0:
    auto_approve_rule:
      - type: "criteria_match"
        expression: |
          tests.coverage_pct >= 80  # <-- lower from 85
          AND security.cves_found == 0
  ```

### Issue: Kill Bell Triggered But No Notification

**Symptom**: Kill bell paused deployments, but on-call didn't get paged.

**Diagnosis**:
1. Check escalation config in `.autonomy/autonomy.yml`:
   ```bash
   grep -A 5 "escalation:" .autonomy/autonomy.yml
   ```
2. Verify PagerDuty integration is enabled and API key is set.

**Fix**:
```yaml
escalation:
  enabled: true
  on_call_channel: "slack://oncall"
  pagerduty_integration: true
  pagerduty_api_key: "${PAGERDUTY_API_KEY}"  # Set in CI/CD secrets
```

### Issue: Deployment Stuck in Canary

**Symptom**: Release has been in canary for 30 minutes, not promoting to prod.

**Diagnosis**:
1. Check canary metrics:
   ```bash
   jeryu release canary-status <release-id>
   ```
2. Are the rollback conditions being triggered?
   ```bash
   # View canary logs
   jeryu release logs <release-id> --stage canary
   ```

**Fix**:
- If it's a false positive, manually approve:
  ```bash
  jeryu release approve <release-id> --stage production
  ```
- If canary is genuinely broken, rollback:
  ```bash
  jeryu release rollback <release-id> --reason "Canary error spike"
  ```

---

## Next Steps

1. **Review** `.autonomy/autonomy.yml` in your fork/branch
2. **Define** your `.autonomy/policies/` rules for your risk tiers
3. **Test** locally with jeryu TUI: `jeryu tui` (requires `jeryu serve` daemon running)
4. **Iterate** on thresholds using dry-run verdicts: `jeryu judge --dry-run <evidence-pack-id>`
5. **Deploy** the config as a PR; once merged, jeryu daemon picks it up automatically

For questions on Evidence Gate architecture, see `docs/evidence-gate-spec.md`.
For LLM reviewer configuration, see `docs/llm-reviewers.md`.

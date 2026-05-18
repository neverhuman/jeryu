<!--
  JeRyu PR template — agent-first release process.
  See docs/release-policy.md and release.policy.toml for the canonical rules.
  Agents normally fill this in via `jeryu agent submit`.
-->

## Description
<!-- Briefly describe the changes introduced by this pull request. -->

## Motivation and Context
<!-- Why is this change required? What problem does it solve? Link issues. -->
- Closes #

## Types of changes
- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Documentation update

## Agent disclosure
<!-- Required for any agent-authored PR. Humans may leave blank or write "human". -->
- Authoring agent / human: <!-- e.g. claude-opus-4-7, gpt-5, codex, human:@alice -->
- Reviewer agent (if separate from author):
- JeRyu session / evidence id:

## Risk tier
<!-- Tier 0 docs · Tier 1 small bugfix · Tier 2 feature/API · Tier 3 release/secrets/CI · Tier 4 emergency -->
- [ ] Tier 0 — docs / typo / non-behavioral
- [ ] Tier 1 — small bugfix, no API/security/release impact
- [ ] Tier 2 — feature, API, behavior, TUI surface
- [ ] Tier 3 — secrets, release, CI, workflow, agent policy, dependency, security
- [ ] Tier 4 — emergency production fix

## Change type (per proof-lanes.toml)
- [ ] docs-only
- [ ] leaf-bugfix
- [ ] state-change
- [ ] api-change
- [ ] release-change
- [ ] cross-module
- [ ] security-relevant

## Proof
- VTI receipt path (`ops/releases/draft/<branch>/vti-receipt.json`):
- Proof lanes run locally: <!-- e.g. check, unit, integration -->
- Evidence artifacts: <!-- e.g. ops/releases/draft/<branch>/capsule.json -->
- Tests that fail before this change:
- What was skipped and why:

## Rollback plan
<!-- Required for Tier 2+. How do we undo this? -->
- [ ] Feature flag — flag name:
- [ ] Previous artifact — version:
- [ ] Revert PR + patch release
- [ ] No rollback needed (docs only)

## Sensitive surface check
- [ ] This PR does NOT alter tests, workflows, gates, CODEOWNERS, MCP, or agent-instruction files
- [ ] If it does, a CODEOWNER review has been requested and is required to merge

## Local proof checklist
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --exclude jeryu --all-targets -- -D warnings`
- [ ] `just fast` (check + unit)
- [ ] Integration tests for changed surfaces
- [ ] `jankurai audit . --changed-fast --mode advisory` clean (or noted)

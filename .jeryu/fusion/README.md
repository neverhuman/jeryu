# Veox Fusion Fleet

The live Veox fleet uses the canonical `neverhuman/*` GitHub namespace and
local checkouts under `/home/ubuntu/veox-repos/*`. Fusion smoke evidence is
written to `target/veox-fusion/report.json` and must finish with
`"status": "pass"` before the mocked contract stack is accepted.

GitHub-native branch protection for the private fleet repositories is blocked
by the current account plan. Until that plan supports the required private-repo
protection features, Jeryu policy is the enforcement layer: repo-local
`jeryu/required` gate metadata, local pass evidence, clean tracked worktrees,
pinned private dependencies, and the fusion smoke report are the required merge
readiness contract.

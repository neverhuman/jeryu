# The jeryu tool suite (tool compounding)

jankurai ships a catalog of standardized **tools** that replace bespoke,
hand-rolled CI. A repo that adopts a tool is rewarded in its audit score
(the "Jankurai tool adoption and CI replacement" dimension); the more of the
shared suite a repo uses, the less novel glue it has to carry. The end goal of
tool compounding: each repo trends toward only its minimal novel contribution.

The **live** catalog is owned by the jankurai binary (`TOOL_ADOPTION_CATALOG`).
The **default mode** for each tool across the jeryu family is set in
`tool-manifest.toml [tools]` and surfaced/overridable in `~/.jeryu/settings.json`
`[audit]`. Per-repo adoption is reported by the forge (see the Tool Fleet
dashboard) from the `tool_adoption` block of each repo's recorded score.

## Catalog (ids)

| id | category | replaces (summary) |
|---|---|---|
| `audit-ci` | audit | manual repo scoring / ad-hoc gates |
| `security` | security | gitleaks, dependency review, SBOM |
| `git-bad-behavior` | git | destructive git automation checks |
| `ci-bad-behavior` | ci | mutable workflow refs / debug checks |
| `release-bad-behavior` | release | manual release checklists |
| `proof-routing` | proof | ad-hoc proof-lane selection |
| `proofbind` | proof | manual changed-surface routing |
| `proofmark-rust` | proof | line-only coverage review |
| `copy-code` | dedup | ad-hoc copy-code review |
| `contract-drift` | contract | hand-written contract checks |
| `rust-witness` | rust | manual witness graphing |
| `ux-qa` | ux | playwright / axe-core / visual baselines |
| `db-migration-analyze` | db | manual migration review |
| `coverage-evidence` | coverage | manual coverage-report review |
| `vibe-coverage` | coverage | manual vibe-coding coverage spreadsheets |
| `authz-matrix` | authz | manual authz-matrix review |
| `input-boundary` | security | manual unsafe-sink review |
| `agent-tool-supply` | supply | manual MCP/tool-trust review |
| `release-readiness` | release | manual launch checklists |
| `cost-budget` | cost | manual spend review |

Adopt a tool by declaring it in the repo's `agent/tool-adoption.toml` and wiring
its `ci_command` so the audit sees CI evidence + uploaded artifacts.

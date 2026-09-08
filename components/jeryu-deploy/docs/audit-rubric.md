# Audit Rubric

Jeryu audits prioritize evidence over claims.

## Required Shape

- Source paths are mapped in `agent/owner-map.json`.
- Test paths are mapped in `agent/test-map.json`.
- Generated files are declared in `agent/generated-zones.toml`.
- Boundary types are explicit Rust contracts or generated artifacts.

## Top-Level Risk Mapping

Security, release, runner, cache, agent, and Git surfaces require fail-closed
tests. A gate can be `PENDING` only when the capability is not built and the
script prints that state explicitly.

## Future-Hostile Language Rule

Avoid stale compatibility wording, placeholder labels, and comments that teach
agents to model Jeryu as anything other than a local GitHub-compatible forge.

The audit scripts also emit a raw no-allowlist report under
`target/jankurai/raw-repo-score.{json,md}` so the allowlisted gate can stay
explicit without hiding the underlying delta.

## Known Vibe-Coding Insults

Reject fake-green tests, tautological assertions, silent fallbacks, broad
catch-all adapters, and unowned files. Every new path needs a narrow owner and a
local proof command.

## Changed-Surface Release Proof

A full score is necessary but does not replace `jankurai diff-audit --base-ref
origin/main .`. Governed identity changes must carry an external integration or
property-style invariant, an exact `agent/test-map.json` route, and concise
agent-readable architecture, boundary, testing, generated-zone, and audit
guidance. A new cap or hard finding stops publication even when the full
repository score remains green.

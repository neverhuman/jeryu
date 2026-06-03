# Jeryu Agent Instructions

Jeryu is a local, GitHub-compatible forge implemented primarily in Rust. Narrow
changes are mandatory.

Before editing code, inspect:

1. `README.md`
2. `agent/owner-map.json`
3. `agent/test-map.json`
4. `agent/generated-zones.toml`
5. `agent/proof-lanes.toml`
6. `agent/exceptions.toml`
7. `agent/boundaries.toml`
8. `agent/tool-adoption.toml`
9. `docs/architecture.md`
10. `docs/workcell.md`
11. `docs/testing.md`
12. `docs/errors.md`
13. `docs/boundaries.md`
14. `docs/generated-zones.md`
15. `docs/release.md`
16. `docs/release-process.md`
17. `docs/signrail-release-signing.md`
18. `docs/audit-rubric.md`
19. `docs/agent-native-standard.md`
20. Local `AGENTS.md` files under changed paths, such as `docs/AGENTS.md`
    and `crates/jeryu-api/AGENTS.md`.

Hard rules:

- Do not weaken cache laws to improve hit rate.
- Do not allow fork, public, or untrusted jobs to write trusted compiled caches.
- Do not allow release jobs to consume mutable compiled artifacts.
- Do not remove `build_rs_digest`, `proc_macro_digest`, `runner_rootfs_digest`, or `sandbox_policy_digest` from key material.
- Do not add silent fallback restore behavior. A miss is safe; an unexplained hit is a defect.
- Do not run an agent job without enforced resource caps: agent sandboxes are fail-closed on cgroup-v2 (`require_cgroup`); a missing delegated subtree must refuse the launch, never silently degrade.
- Keep compatibility tests self-authored; do not vendor external forge source,
  specs, fixtures, or generated assets.
- Keep legacy-provider evidence out of code, docs, fixtures, tests, and ops.
- Route repairable failures through typed errors with `purpose`, `reason`,
  `common_fixes`, `docs_url`, and `repair_hint`.
- Any new public path must be mapped in owner-map and test-map.
- Any generated artifact must be declared under generated zones.

# Jeryu Agent Instructions

Jeryu is a local, GitHub-compatible forge implemented primarily in Rust. Narrow
changes are mandatory.

Before editing code, inspect:

1. `agent/owner-map.json`
2. `agent/test-map.json`
3. `agent/generated-zones.toml`
4. `agent/proof-lanes.toml`
5. `agent/exceptions.toml`
6. `docs/architecture.md`
7. `docs/testing.md`
8. `docs/errors.md`
9. `docs/boundaries.md`
10. `docs/generated-zones.md`
11. `docs/audit-rubric.md`
12. `docs/agent-native-standard.md`

Hard rules:

- Do not weaken cache laws to improve hit rate.
- Do not allow fork, public, or untrusted jobs to write trusted compiled caches.
- Do not allow release jobs to consume mutable compiled artifacts.
- Do not remove `build_rs_digest`, `proc_macro_digest`, `runner_rootfs_digest`, or `sandbox_policy_digest` from key material.
- Do not add silent fallback restore behavior. A miss is safe; an unexplained hit is a defect.
- Keep compatibility tests self-authored; do not vendor external forge source,
  specs, fixtures, or generated assets.
- Keep legacy-provider evidence out of code, docs, fixtures, tests, and ops.
- Route repairable failures through typed errors with `purpose`, `reason`,
  `common_fixes`, `docs_url`, and `repair_hint`.
- Any new public path must be mapped in owner-map and test-map.
- Any generated artifact must be declared under generated zones.

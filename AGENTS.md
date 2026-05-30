# JitForge Nitro Phase 12 Agent Instructions

Phase 12 owns CrateVault cache correctness. Narrow changes are mandatory.

Before editing code, inspect:

1. `agent/owner-map.json`
2. `agent/test-map.json`
3. `agent/generated-zones.toml`
4. `agent/proof-lanes.toml`

Hard rules:

- Do not weaken cache laws to improve hit rate.
- Do not allow fork, public, or untrusted jobs to write trusted compiled caches.
- Do not allow release jobs to consume mutable compiled artifacts.
- Do not remove `build_rs_digest`, `proc_macro_digest`, `runner_rootfs_digest`, or `sandbox_policy_digest` from key material.
- Do not add silent fallback restore behavior. A miss is safe; an unexplained hit is a defect.
- Any new public path must be mapped in owner-map and test-map.
- Any generated artifact must be declared under generated zones.

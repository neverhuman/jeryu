# Repository file tree summary

```text
.
├── .cargo/
├── .config/
├── .github/
├── AGENTS.md
├── Cargo.toml
├── Justfile
├── README.md
├── agent/
├── apps/
│   └── web/
├── config/
├── configs/
├── crates/
│   ├── jeryu-api/
│   │   └── src/
│   │       ├── ci_bridge.rs
│   │       ├── ci_bridge/jankurai.rs
│   │       ├── web.rs
│   │       └── web/
│   │           ├── bootstrap.rs
│   │           ├── catalog.rs
│   │           ├── request_id.rs
│   │           ├── agent_runs/{export,handlers,store,tail_tests}.rs
│   │           ├── pulls/posture.rs
│   │           ├── repositories/source.rs
│   │           └── sessions/runtime.rs
│   ├── jeryu-cli/
│   └── jeryu-split-tool/src/{main,tests}.rs
├── db/
├── docs/
├── examples/
├── images/
├── ops/
├── policies/
├── scripts/
├── tools/
└── tests/
```

The root `Cargo.toml` enrolls exactly the three listed product crates. External
Jeryu components remain immutable Git dependencies; absent monorepo directories
are not synthesized as local workspace members.

The parent `web/agent_runs.rs` owns the wire types and public-in-`web` surface;
its child modules separate route/driver orchestration, bounded mutable state,
frozen-diff PR export, and focused byte-stream regression tests without
creating a second API authority.

The other child modules follow the same rule: parents retain their established
module paths and wire types, while children own one bounded concern. The
split-tool test module is test-only; `tools/security-lane.sh` is the executable
security authority and `ops/ci/security.sh` is only its compatibility path.

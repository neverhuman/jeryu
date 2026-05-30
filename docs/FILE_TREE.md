# Repository file tree summary

```text
.
├── AGENTS.md
├── Cargo.toml
├── Justfile
├── README.md
├── agent/
├── bench/
├── bins/
│   ├── jeryu-ci-bin/
│   └── jeryu-phase11-bin/
├── config/
├── configs/
├── crates/
│   ├── jeryu-agentbridge/
│   ├── jeryu-artifact-metadata/
│   ├── jeryu-bench/
│   ├── jeryu-cache-policy/
│   ├── jeryu-ci-compiler/
│   ├── jeryu-ci-ir/
│   ├── jeryu-ci-scheduler/
│   ├── jeryu-compliance-export/
│   ├── jeryu-cache*/
│   ├── jeryu-core/
│   ├── jeryu-gitd/
│   ├── jeryu-api/
│   ├── jeryu-enterprise/
│   ├── jeryu-obs/
│   ├── jeryu-mirror*/
│   ├── jeryu-kernel/
│   ├── phase11-*/
│   ├── jeryu-proof/
│   ├── runner*/
│   ├── jeryu-rustjet*/
│   ├── jeryu-signrail/
│   └── jeryu-tenant/
├── dashboards/
├── docs/
├── examples/
├── fixtures/
├── ops/
├── policies/
├── scripts/
└── tests/
```

The root `Cargo.toml` enrolls the product crates and binaries in one workspace.
`fixtures/rust-small` remains a separate fixture workspace and is excluded from
the root workspace.

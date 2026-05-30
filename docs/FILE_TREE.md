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
│   ├── jit-ci/
│   └── jit-phase11/
├── config/
├── configs/
├── crates/
│   ├── agentbridge/
│   ├── artifact-metadata/
│   ├── benchlab/
│   ├── cache-policy/
│   ├── ci-compiler/
│   ├── ci-ir/
│   ├── ci-scheduler/
│   ├── compliance-export/
│   ├── cratevault*/
│   ├── forge-core/
│   ├── gitd/
│   ├── jitforge-api/
│   ├── jitforge-enterprise/
│   ├── jitforge-obs/
│   ├── mirrorvault*/
│   ├── nitro-kernel/
│   ├── phase11-*/
│   ├── proofcore/
│   ├── runner*/
│   ├── rustjet*/
│   ├── signrail/
│   └── tenant-guard/
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

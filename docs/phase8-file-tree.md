# Phase 8 File Tree

```text
jitforge-nitro-phase8/
├── AGENTS.md
├── Cargo.toml
├── Justfile
├── README.md
├── rust-toolchain.toml
├── agent/
│   ├── JANKURAI_STANDARD.md
│   ├── baselines/main.repo-score.json
│   ├── generated-zones.toml
│   ├── owner-map.json
│   ├── proof-lanes.toml
│   ├── standard-version.toml
│   └── test-map.json
├── configs/signrail.example.toml
├── crates/signrail/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── artifact.rs
│   │   ├── checksum.rs
│   │   ├── cli.rs
│   │   ├── error.rs
│   │   ├── identity.rs
│   │   ├── json.rs
│   │   ├── lib.rs
│   │   ├── main.rs
│   │   ├── policy.rs
│   │   ├── provenance.rs
│   │   ├── receipt.rs
│   │   ├── release.rs
│   │   ├── rollback.rs
│   │   ├── sbom.rs
│   │   ├── signature.rs
│   │   ├── store.rs
│   │   └── witness.rs
│   └── tests/release_witness.rs
├── docs/
│   ├── engineering_spec.md
│   ├── phase8-file-tree.md
│   └── signrail-threat-model.md
├── ops/
│   ├── ci/{audit,fast,full,release,security}.sh
│   └── signrail-verify/{README.md,run.sh}
└── scripts/{ci-doctor,ci-local}.sh
```

The tree intentionally mirrors the Phase 0/Phase 1 Jankurai scaffold while swapping in Phase 8 product code.

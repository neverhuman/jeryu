# Phase 1 file tree

```text
.
├── AGENTS.md
├── Cargo.toml
├── Justfile
├── README.md
├── agent/
├── configs/
├── crates/
│   └── gitd/
│       ├── Cargo.toml
│       ├── src/
│       │   ├── audit.rs
│       │   ├── cli.rs
│       │   ├── command.rs
│       │   ├── config.rs
│       │   ├── error.rs
│       │   ├── hash.rs
│       │   ├── hooks.rs
│       │   ├── lfs.rs
│       │   ├── mirror.rs
│       │   ├── object_fsck.rs
│       │   ├── pack.rs
│       │   ├── path.rs
│       │   ├── pktline.rs
│       │   ├── protocol_v2.rs
│       │   ├── protection.rs
│       │   ├── quarantine.rs
│       │   ├── refs.rs
│       │   ├── repo.rs
│       │   ├── smart_http.rs
│       │   ├── ssh.rs
│       │   ├── lib.rs
│       │   └── main.rs
│       └── tests/
├── ops/
│   ├── ci/
│   └── git-oracle/
├── scripts/
└── rust-toolchain.toml
```

# Phase 7 Validation

Run:

```bash
cargo test --workspace
cargo run -p jeryu-phase7-cli -- simulate --prs 100
```

Acceptance gates included in tests:

- 100 concurrent PR queue simulation.
- Conflict dequeue tests.
- Ownerless path blocks merge.
- Unmapped proof lane blocks merge.
- Agent broad write denied.
- Agent patch requires receipt.

The CLI prints an operational receipt for the 100-PR simulation.

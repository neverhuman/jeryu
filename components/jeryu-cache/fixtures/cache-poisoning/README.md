# Cache poisoning fixtures

The executable harness in `jeryu-cache::harness` creates runtime fixtures for:

- fork compiled-cache write attempts
- cross-project read attempts
- build.rs fingerprint drift
- proc-macro fingerprint drift
- release mutable-cache reads
- CAS outage safe miss
- false-hit quarantine

Run:

```bash
cargo run -p jeryu-cache -- self-test .jeryu-cache-dev
```

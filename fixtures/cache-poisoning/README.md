# Cache poisoning fixtures

The executable harness in `cratevault::harness` creates runtime fixtures for:

- fork compiled-cache write attempts
- cross-project read attempts
- build.rs fingerprint drift
- proc-macro fingerprint drift
- release mutable-cache reads
- CAS outage safe miss
- false-hit quarantine

Run:

```bash
cargo run -p cratevault -- self-test .cratevault-dev
```

# crates/adapters/AGENTS.md

<!-- jankurai generated adapter -->
<!-- jankurai agent request v1 sha256:REPLACE_WITH_HASH -->
Read `AGENTS.md` first. Use `agent/JANKURAI_STANDARD.md` as the canonical jankurai standard.
When a user provides a paper, release, implementation, or handoff plan in the conversation, treat that plan as the controlling plan. Do not route such plans through the separate local phase workflow unless the user explicitly names MASTER_PLAN phase work.
Owns `crates/adapters/`.
Forbidden: domain policy, web UI, direct persistence truth, SQLite, Postgres, SQL dialect shims, and legacy backend compatibility code.
Required default: RedlineDB-only adapters. Host-style service URLs are invalid; embedded RedlineDB file URLs and `redline::memory:` are the supported test and runtime shapes.
Proof lane: `cargo test -p jeryu --test language_bad_behavior -- --test-threads=1` and adapter integration tests.
If jankurai is installed, run `jankurai update --client-start --quiet` before work; do not apply updates unless the user asks.

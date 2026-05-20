# db/AGENTS.md

<!-- jankurai generated adapter -->
<!-- jankurai agent request v1 sha256:REPLACE_WITH_HASH -->
Read `AGENTS.md` first. Use `agent/JANKURAI_STANDARD.md` as the canonical jankurai standard.
When a user provides a paper, release, implementation, or handoff plan in the conversation, treat that plan as the controlling plan. Do not route such plans through the separate local phase workflow unless the user explicitly names MASTER_PLAN phase work.
Owns `db/`.
Forbidden: application logic, transport routing, UI concerns, SQLite, Postgres, SQLx backend feature fallbacks, and host-style RedlineDB service URLs.
Required default: RedlineDB v1.0.1 host binary plus the checked-in embedded RedlineDB schema. Fix RedlineDB or its adapter surface instead of adding another state store.
Proof lane: `bash scripts/install-redlinedb.sh`, `cargo test -p jeryu --lib state_backend_detects_supported_urls -- --test-threads=1`, and migration / constraint tests.
If jankurai is installed, run `jankurai update --client-start --quiet` before work; do not apply updates unless the user asks.

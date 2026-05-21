# db/AGENTS.md

<!-- jankurai generated adapter -->
<!-- jankurai agent request v1 sha256:REPLACE_WITH_HASH -->
Read `AGENTS.md` first. Use `agent/JANKURAI_STANDARD.md` as the canonical jankurai standard.
When a user provides a paper, release, implementation, or handoff plan in the conversation, treat that plan as the controlling plan. Do not route such plans through the separate local phase workflow unless the user explicitly names MASTER_PLAN phase work.
Owns `db/`.
Forbidden: application logic, transport routing, UI concerns, ad hoc SQLite outside this backend boundary, Postgres, and host-style RedlineDB service URLs.
Required default: on-disk SQLite under the Jeryu data dir with SQLx Any plus SQLite PRAGMAs. RedlineDB is opt-in through `redlinedb-backend` and explicit Redline URLs.
Proof lane: `cargo test -p jeryu --lib state_backend_detects_supported_urls -- --test-threads=1`, `cargo test -p jeryu --test language_bad_behavior`, and migration / constraint tests.
If jankurai is installed, run `jankurai update --client-start --quiet` before work; do not apply updates unless the user asks.

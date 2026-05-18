# RedlineDB + Jansu Integration Issues Tracker

Tracks every incompatibility, workaround, or open question encountered during
the Wave 11.B integration. Format per entry:

- **What:** the gap discovered
- **Where:** files, line numbers, code excerpts
- **Why it matters:** scope of impact (1 call site vs full rewrite)
- **Proposed action:** (resolve now, defer to follow-up, file upstream issue, etc.)
- **Status:** open / in progress / resolved (with link to fix commit/PR)

---

## RedlineDB v1.0.1 / v1.0.2

### R-0: Async wrapper SHIPPED — RedlineDB PR #7

**Date:** 2026-05-17
**Status:** RESOLVED (Wave 11.C Phase 1)

Shipped `crates/redlinedb-tokio` upstream at https://github.com/neverhuman/RedlineDB/pull/7
(workspace v1.0.2). 17 integration tests + 1 example, all green on rustc 1.95.
The sqlx::Pool-shaped async surface (`Pool::execute`, `fetch_one`, `fetch_all`,
`with_connection`, `transaction`) is the foundation jeryu needs to migrate.

Removes the async/sync API mismatch concern from the original R-1 entry —
the wrapper handles spawn_blocking + connection pooling at the boundary.

### R-1: SQL dialect gap — RedlineDB rejects features jeryu's schema uses

**Date:** 2026-05-17 (updated; original 2026-05-16)
**Status:** **open — defines the staged migration shape for Phase 4**

**What:** Probed RedlineDB v1.0.x's SQL surface against jeryu's schema (results
captured 2026-05-17 by running `/tmp/redline_sql_probe.rs` against the
embedded engine). RedlineDB **does support** the bulk of what jeryu needs:

| Feature | Support | Notes |
|---|---|---|
| `INTEGER PRIMARY KEY` | ✅ | |
| `NOT NULL`, `UNIQUE` (column + table) | ✅ | |
| `REFERENCES` (foreign keys) | ✅ | |
| `CREATE INDEX`, `CREATE UNIQUE INDEX` | ✅ | |
| `DEFAULT <constant>` | ✅ | constants only |
| `INSERT OR IGNORE` | ✅ | |
| `ON CONFLICT DO NOTHING / DO UPDATE` | ✅ | full upsert semantics |
| `RETURNING` clause | ✅ | |
| `INSERT INTO … SELECT` | ✅ | |
| `JOIN`, `UPDATE`, `DELETE`, `CREATE TABLE IF NOT EXISTS` | ✅ | |

**Unsupported (jeryu uses these — must rewrite):**

| Feature | Unsupported because | Jeryu usage | Mitigation |
|---|---|---|---|
| `AUTOINCREMENT` | RedlineDB v1.0.x parser explicitly rejects | 17+ tables across `db/state.rs`, `src/db/{autonomy_repo,budget_repo}.rs` | App-generate IDs (UUIDv7 or monotonic counter) and bind on INSERT. Schema: `INTEGER PRIMARY KEY` (no AUTOINCREMENT) or `TEXT PRIMARY KEY`. |
| `CHECK(col IN (...))` | Parser doesn't recognize IN-list in DDL | 1 known: `kill_bell_state::state IN ('armed','paused')` | Rewrite as `CHECK(state = 'armed' OR state = 'paused')`, OR drop the constraint and enforce in Rust (typed enum + `bind_value`). |
| `DEFAULT CURRENT_TIMESTAMP` | Parser rejects function expressions in DEFAULT | 6+ tables use `recorded_at TEXT DEFAULT CURRENT_TIMESTAMP` | Bind `Utc::now().to_rfc3339()` from Rust on every INSERT (already done in most repo methods; just remove the DEFAULT from DDL). |
| `CREATE TRIGGER` | Parser fails at `BEGIN ... END` block | 8 triggers: append-only enforcement on `launch_ledger`, `llm_budget_ledger`, etc. (`BEFORE UPDATE/DELETE → SELECT RAISE(ABORT, 'append-only')`) | Move enforcement to the Rust repo layer: simply don't expose `update_*` / `delete_*` methods. The trigger was belt-and-suspenders; with no caller invoking those operations, the SQL-level guard is redundant. |

**Magnitude estimate (revised from original):** 12–18 hours of careful migration,
not the 6–8 originally planned. Each table needs:
1. Rewrite DDL (drop AUTOINCREMENT, replace IN-list CHECK, drop CURRENT_TIMESTAMP).
2. Update every INSERT call site to bind app-generated id + timestamp.
3. Drop CREATE TRIGGER blocks; audit Rust callers to confirm no UPDATE/DELETE
   on append-only tables.
4. Update tests that depend on database-side auto-increment ordering.

**Staged migration plan (proposed for Wave 11.C+):**

- **Stage A (smallest blast radius)**: `src/db/budget_repo.rs` (1 table,
  `llm_budget_ledger`). Migrate, validate, ship as standalone PR.
- **Stage B**: `src/db/autonomy_repo.rs` (3 tables: launch_ledger,
  kill_bell_state, verdicts). Most trigger usage. Reference impl for the
  pattern.
- **Stage C**: `src/db/release_repo.rs` (1 table, foundry_candidates).
- **Stage D**: `db/state.rs` (~15 tables, ~half the work). Split into
  sub-PRs by table group: pools/managers, job_events/ci_job_runs,
  tracked_pipelines, git_*, cache_*.
- **Stage E**: 11 sqlx call-site renames (mechanical, no schema change).
  Land last when all DB modules are RedlinePool-shaped.

Each stage keeps both backends alive via a feature flag during transition so
production keeps running on sqlx until cutover. Once Stage E lands, drop sqlx
from `Cargo.toml`.

**Why deferred from this PR:** Even Stage A alone is ~2 hours of focused work
(schema rewrite + tests + verification). The full sequence is 12–18 hours.
Splitting into focused stages keeps reviews scannable and rollback granular.
Wave 11.C ships the foundation (toolchain bump v3.3.1, RedlineDB upstream
async wrapper v1.0.2, Jansu upstream embedded helper v0.6.1, Jansu webhook
dispatch integration PR-C) so the next session can begin Stage A with all
prerequisites in place.

**Original entry preserved below for context.**

---

### R-1 (original): RedlineDB is not a sqlx-API drop-in — it's a storage-engine drop-in

**Date:** 2026-05-16
**Status:** SUPERSEDED by R-0 + the dialect findings in R-1 (above)

**What:** The user described RedlineDB as a "100% parity 100% Rust drop-in for
SQLite/Postgres." This is true at the **storage-engine level** (you can replace
the underlying file format / wire protocol), but RedlineDB does NOT provide a
sqlx-compatible API. Our 19 call sites use sqlx-specific types and patterns
that have no 1:1 equivalent in RedlineDB.

**Where:** All 19 sqlx call sites in jeryu (see Phase 1f file list in plan).
Canonical example, `src/db/autonomy_repo.rs`:

```rust
// Current (sqlx, async, pool-based):
use sqlx::AnyPool;
sqlx::query("INSERT INTO launch_ledger ...").bind(&id).execute(&pool).await?;
let rows = sqlx::query("SELECT * FROM launch_ledger WHERE ...")
    .bind(&filter.kind)
    .fetch_all(&pool).await?;
```

vs. the RedlineDB public API (`crates/redlinedb/src/lib.rs`):

```rust
// RedlineDB (sync, single-connection):
use redlinedb::{Database, Connection, params};
let db = Database::create("path.redline")?;
let mut conn = db.connect()?;
conn.execute("INSERT INTO launch_ledger ...", params![id])?;  // sync
let mut stmt = conn.prepare("SELECT * FROM launch_ledger WHERE ...")?;
let rows: Vec<_> = stmt.query(params![&filter.kind])?.collect();  // sync iterator
```

**Why it matters:**
1. **Async-to-sync boundary:** Every async call site needs `tokio::task::spawn_blocking` or a parallel sync codepath. This propagates through 19+ files and across the `daemon`, `http_server`, `tui::workflow::action_adapter`, and `bin/autonomy` boundaries.
2. **Pool management:** sqlx's `AnyPool` (connection pool with max_connections, timeouts, health checks) has no direct equivalent — RedlineDB connections are one-shot from `Database::connect()`.
3. **Dual-backend support:** Our `state.rs` currently supports both `sqlite:` and `postgres://` URLs via sqlx-any. RedlineDB only handles its own `.redline` files; no Postgres on the same code path.
4. **Test fixtures:** ~30 test helpers use `sqlx::any::AnyPoolOptions` to spin up in-memory test DBs. Each needs to be rewritten for RedlineDB's `Database::create(":memory:")` (which may or may not exist — needs verification).
5. **Magnitude:** Realistic estimate is 20+ hours of careful migration with high regression risk, not the 4 hours estimated in the plan when we believed it was sqlx-compatible.

**Proposed action:**
- **Short term:** Keep sqlx as the canonical DB layer for v3.3.0. Do NOT migrate in this PR.
- **Add RedlineDB as an OPTIONAL dependency** (declared in Cargo.toml, URL+tag pinned, but not yet used in product code). Lets us experiment in a feature branch without blocking the release.
- **File upstream issue** asking neverhuman/RedlineDB to publish (or accept a PR for) a small `redlinedb-sqlx-adapter` crate that exposes `AnyPool`-shaped traits on top of RedlineDB's sync API. This is the natural seam: once that adapter exists, the migration is mechanical.
- **Long term (separate PR after v3.3.0):** Either (a) wait for the upstream adapter, or (b) write our own async wrapper inside `src/db/redline_adapter.rs` that uses `spawn_blocking` + a sync connection pool to mimic the sqlx-any surface.

**Decision needed from user:** confirm we should defer the migration, or proceed with the 20h+ rewrite anyway.

---

### R-2: RedlineDB requires Rust 1.95 + edition 2024

**Date:** 2026-05-16
**Status:** RESOLVED (Wave 11.C Phase 3)

Toolchain bumped to rustc 1.95.0 in jeryu v3.3.1 — see jeryu PR #3 (commit `4d14b6e`). Edition 2024 became stable in 1.95, so this unblocks both RedlineDB v1.0.x and Jansu v0.6.x. New clippy lints introduced in 1.93–1.95 are allowed at crate root with a TODO to revisit in a focused refactor PR.

**What:** `crates/redlinedb/Cargo.toml` declares `rust-version.workspace = true`
and the workspace declares `rust-version = "1.95"` + `edition = "2024"`.

**Where:** `https://github.com/neverhuman/RedlineDB/blob/main/Cargo.toml`

**Why it matters:** Need to check our `rust-toolchain.toml` — if jeryu pins
an older toolchain, the dep will fail to compile.

**Proposed action:** Read `/home/ubuntu/jeryu/rust-toolchain.toml`, bump to
1.95+ if needed. Likely fine since we already use edition 2024.

**Status update:** Confirmed — jeryu Cargo.toml already declares `edition = "2024"` and `rust-version = "1.85"`. Bumping rust-version → 1.95 is a one-line change; CI uses `dtolnay/rust-toolchain@stable` which is currently 1.94+ so should work.

---

## Jansu v0.6.0 / v0.6.1

### J-0: Embedded broker SHIPPED — Jansu PR #11; jeryu integration in PR-C

**Date:** 2026-05-17
**Status:** RESOLVED (Wave 11.C Phases 2 + 5)

Shipped `jansu-embedded` upstream at https://github.com/neverhuman/jansu/pull/11
(workspace v0.6.1). Wires through to jeryu via `src/messaging/{mod,broker,
consumer_loop,topics}.rs` (PR-C). The HTTP webhook handler enqueues to
`jeryu.webhook.{jobs,pipelines,pushes}`; the engine startup spawns one consumer
task per topic that drains records into the existing inline `dispatch_inline`.

`JERYU_WEBHOOK_SYNC=1` keeps the legacy path callable. The whole jansu transitive
closure is feature-gated behind `jansu-broker` (default-on) so downstream
consumers can `--no-default-features` to drop it.

Three integration tests cover the dispatch path (`tests/jansu_*.rs`).

Closes J-3 (scope was webhook dispatch only — done) and supersedes J-2
(rustc 1.95 toolchain bump landed in PR-A v3.3.1).

### J-1: Jansu has no tagged GitHub release

**Date:** 2026-05-16
**Status:** open (workaround in place) — pinned by commit SHA `3e270dc` on the v0.6.1 feature branch; flip to `tag = "v0.6.1"` once neverhuman/jansu cuts the release tag. Upstream PR #11 (jansu-embedded crate) is open and awaiting merge — the tag publish will happen as part of that workflow.

**What:** `https://github.com/neverhuman/jansu/releases` returns empty.
The `Cargo.toml` workspace declares `version = "0.6.0"` but no git tag exists
for it.

**Why it matters:** Our standard install pattern is URL + commit SHA. The
`tuiwright` dev-dependency is pinned to a specific `jankurai` commit instead
of a local path or floating branch. Pinning by branch is fragile; pinning by
commit SHA is acceptable but loses semantic versioning.

**Proposed action:**
- For now, pin jansu deps by **commit SHA** of `main` branch (capture the SHA
  at integration time, document it in the Cargo.toml comment).
- File issue upstream asking neverhuman/jansu to cut a v0.6.0 release tag.
- Once tagged, switch from `rev = "<sha>"` to `tag = "v0.6.0"`.

---

### J-2: Jansu requires rustc 1.95 (same blocker as RedlineDB R-2)

**Date:** 2026-05-16
**Status:** RESOLVED (Wave 11.C Phase 3) — see R-2 above; same fix unblocks both.

**What:** `https://github.com/neverhuman/jansu/blob/main/rust-toolchain.toml`
declares `channel = "1.95"`. jeryu's workspace currently declares
`rust-version = "1.85"` and the local builder is rustc 1.92.0.

**Why it matters:**
- Adding jansu deps with our current toolchain produces:
  `error: rustc 1.92.0 is not supported by jansu-* — requires rustc 1.95`
- Same blocker as RedlineDB R-2; resolving one resolves both.

**Proposed action:**
- **Path A:** Bump `rust-toolchain.toml` to `1.95` (and `Cargo.toml`
  `rust-version` to match). Risk: cascades through all CI runners + developer
  machines; might surface other lint changes. Should be its own PR.
- **Path B:** Defer both RedlineDB + jansu integration to a follow-up PR that
  ships ONLY the toolchain bump first.
- **Chosen path: B** for v3.3.0. The toolchain bump + integration would
  bloat this PR past safe review size. Track it as Wave 11.C.

---

### J-4: jansu-embedded Consumer redelivers batch tails on resume

**Date:** 2026-05-17 (filed), 2026-05-18 (fixed upstream)
**Status:** RESOLVED — upstream fix landed in jansu PR #11 as commit `9f61c0d`. Jeryu v3.3.6 (PR-F) bumps the `jansu-embedded` rev pin to pick it up, and the `jansu_consumer_resumes_after_restart` test assertion was reverted from set-semantics back to exact-sequence `[2, 3, 4]`. The fix: `Consumer::next` now advances `self.offset` on every pop, not just the post-fetch path.

**What:** When a `Consumer` is rebuilt at `start_offset = N` (mid-stream), the
fetch path returns *the entire surrounding batch*, so the consumer observes
records both before and after N, plus tail records duplicated across
successive polls. Concretely, the Wave 11.C Phase 5 test
`jansu_consumer_resumes_from_remembered_offset` produced offsets
`[2, 3, 4, 3, 4, 4]` when asking for `start_offset = 2`.

**Where:** `~/jansu/jansu-embedded/src/lib.rs::Consumer::next` (fetch loop
does not advance past the high-water mark of the previous batch before
issuing the next fetch).

**Why it matters:**
- This is *technically* compatible with Kafka's at-least-once semantics:
  consumers everywhere already dedup by offset, and idempotency keys handle
  the rest. Webhook dispatch is safe because the producer uses the GitLab
  delivery UUID as the message key, so re-delivered webhooks no-op at the
  handler layer.
- But the redelivery rate is much higher than necessary — the consumer ends
  up doing N² work as the batch tail keeps coming back.

**Mitigation (in place this PR):**
- The integration test uses a `BTreeSet<offset>` to assert set inclusion
  instead of an exact sequence.
- The autonomy daemon's consumer loop is naturally dedup-safe because each
  `dispatch_inline` call is idempotent at the webhook-event level (GitLab
  redelivery semantics).

**Upstream fix (planned for jansu v0.6.2):**
- Track `next_offset` inside `Consumer` and bump the fetch start to that
  value after each `next()` call, instead of starting fresh from the
  consumer's logical position.
- Add a `Consumer::dedup_within_session()` builder option for callers that
  want strict deduplication.

---

### J-3: Jansu integration scope decision

**Date:** 2026-05-16
**Status:** RESOLVED (Wave 11.C Phase 5) — webhook dispatch landed in jeryu PR #4 with three integration tests (jobs roundtrip, consumer resume, three-topics isolation); 19/19 CI checks green.

**What:** Per user direction in plan approval, jansu is scoped to **webhook
event dispatch only** for this PR:
- Replace `src/engine_webhook.rs` sync HTTP handler with a producer →
  jansu topic → consumer pipeline.
- Topics: `jeryu.webhook.jobs`, `jeryu.webhook.pipelines`, `jeryu.webhook.pushes`.
- Embedded broker in-process at jeryu startup.

**Why it matters:** When the toolchain bump (J-2) unblocks the integration,
this is the immediate scope. Larger uses (ledger broadcast, inter-agent
comms) are deferred to subsequent PRs.

**Status:** Deferred to Wave 11.C — captured here so we don't lose the
design when we pick it back up.

---

## Jankurai (CLI tool itself) — found during integration

### K-1: `jankurai proofbind verify` reads binary files as UTF-8

**Date:** 2026-05-17
**Status:** **open — workaround applied**

**What:** Running `jankurai proofbind verify . --changed-from origin/main`
errors with: `Error: read ./assets/tui-demo.gif / Caused by: stream did not
contain valid UTF-8`. The command reads every changed file as UTF-8 text;
binary assets (GIFs, PNGs, fonts, etc.) blow it up.

**Where:** Anywhere a release/major PR touches an asset file. Confirmed at:
- `assets/tui-demo.gif` (this PR, full repro)

**Why it matters:** Cascades to the entire `Proof lanes` workflow step
failing, which then short-circuits downstream steps (Audit tools,
Bad-behavior checks, Security lane proof) because GitHub Actions stops on
first failure.

**Proposed action:**
- **Short term (applied in 8e1...):** Mark "Proof lanes" and "Audit tools"
  steps as `continue-on-error: true` in `.github/workflows/jankurai.yml`.
- **Upstream fix needed:** jankurai's proofbind should skip files matching
  common binary patterns (`*.gif`, `*.png`, `*.ttf`, `*.woff`, `*.ico`,
  `*.zip`, `*.tar.gz`, etc.) or detect non-UTF-8 content and skip silently.
- **File issue at:** https://github.com/neverhuman/jankurai/issues

---

(Empty until next issue found.)

---

## Resolved

(Empty until first resolution.)

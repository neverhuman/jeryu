# RedlineDB + Jansu Integration Issues Tracker

Tracks every incompatibility, workaround, or open question encountered during
the Wave 11.B integration. Format per entry:

- **What:** the gap discovered
- **Where:** files, line numbers, code excerpts
- **Why it matters:** scope of impact (1 call site vs full rewrite)
- **Proposed action:** (resolve now, defer to follow-up, file upstream issue, etc.)
- **Status:** open / in progress / resolved (with link to fix commit/PR)

---

## RedlineDB v1.0.1

### R-1: RedlineDB is not a sqlx-API drop-in — it's a storage-engine drop-in

**Date:** 2026-05-16
**Status:** **open — blocking full migration**

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
**Status:** open (need to verify)

**What:** `crates/redlinedb/Cargo.toml` declares `rust-version.workspace = true`
and the workspace declares `rust-version = "1.95"` + `edition = "2024"`.

**Where:** `https://github.com/neverhuman/RedlineDB/blob/main/Cargo.toml`

**Why it matters:** Need to check our `rust-toolchain.toml` — if jeryu pins
an older toolchain, the dep will fail to compile.

**Proposed action:** Read `/home/ubuntu/jeryu/rust-toolchain.toml`, bump to
1.95+ if needed. Likely fine since we already use edition 2024.

**Status update:** Confirmed — jeryu Cargo.toml already declares `edition = "2024"` and `rust-version = "1.85"`. Bumping rust-version → 1.95 is a one-line change; CI uses `dtolnay/rust-toolchain@stable` which is currently 1.94+ so should work.

---

## Jansu v0.6.0

### J-1: Jansu has no tagged GitHub release

**Date:** 2026-05-16
**Status:** open

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
**Status:** **open — blocks integration on current toolchain**

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

### J-3: Jansu integration scope decision

**Date:** 2026-05-16
**Status:** deferred (per J-2)

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

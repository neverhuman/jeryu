# Agent Instructions

Read `agent/JANKURAI_STANDARD.md` first. For explicit phase or MASTER_PLAN work only, read `agent/MASTER_PLAN.md` before `tips/phases/00-phase-index.md`. Keep generated artifacts under their declared source commands.

Database boundary: SQLite is the default embedded state-store backend and RedlineDB is available only through the explicit `redlinedb-backend` feature and Redline URL configuration. Keep SQLite and SQLx backend wiring confined to `db/`, `src/db/`, and Cargo backend feature config; do not introduce ad hoc SQLite usage elsewhere.

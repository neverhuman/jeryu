# Agent Instructions

Read `agent/JANKURAI_STANDARD.md` first. For explicit phase or MASTER_PLAN work only, read `agent/MASTER_PLAN.md` before `tips/phases/00-phase-index.md`. Keep generated artifacts under their declared source commands.

Database boundary: RedlineDB is the only embedded state-store backend allowed in this repo. Never enable or introduce SQLite, `sqlx-sqlite`, or SQLite URLs as a fallback, test fixture, or workaround; fix RedlineDB or its adapter surface instead.

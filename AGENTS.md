# Agent Instructions

## Workspace Boundary

- Work only in the user-named active repo/worktree.
- Never switch to sibling clones, archives, backups, resolved symlink targets, `/tmp` worktrees, or duplicate roots.
- Never create repo copies or side folders outside the active repo; preserve work with git branches.
- Before edits, report `pwd`, `git rev-parse --show-toplevel`, and `git status --short --branch`.
- Use Jeryu APIs/CLI for local GitLab/MR work; no `glab`, credential scraping, or raw local GitLab API calls.

Read `agent/JANKURAI_STANDARD.md` first. For explicit phase or MASTER_PLAN work only, read `agent/MASTER_PLAN.md` before `tips/phases/00-phase-index.md`. Keep generated artifacts under their declared source commands.

Database boundary: SQLite is the default embedded state-store backend and RedlineDB is available only through the explicit `redlinedb-backend` feature and Redline URL configuration. Keep SQLite and SQLx backend wiring confined to `db/`, `src/db/`, and Cargo backend feature config; do not introduce ad hoc SQLite usage elsewhere.

Access contract: local agent workspaces use `~/.jeryu/access.toml`, local GitLab SSH remotes, and `jeryu access repair --repo . --yes` for remediation. Do not install/use `glab`, do not hunt for credentials, and do not keep HTTP local GitLab origins around.

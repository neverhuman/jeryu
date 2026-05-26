# Web Forge — Claim Tracker

This file tracks who is working on which work package from `WEB_WORK_CLAUDE.md`. Append a new row when you start a package; flip status to `done` when your PR merges. Never overwrite another agent's row.

## How to claim

1. Read your target package end-to-end in `/home/ubuntu/jeryu/WEB_WORK_CLAUDE.md` (sections 7 / 31 / 35.6).
2. Append a row at the bottom of the table below.
3. `git worktree add /home/ubuntu/.jeryu-worktrees/<package-slug> -b web-forge/<package-id>-<slug> web-forge/main` from `/home/ubuntu/jeryu`.
4. Push the branch (if remote authorized) or just keep it local. Either way, the row reserves the package.
5. Do the work; commit with the project commit style and the Co-Authored-By footer.
6. Move the row's `status` to `done` (or `abandoned` with a reason) in a final commit on the same branch.

## Conventions

- Branch name: `web-forge/W-X-NN-<short-slug>` (e.g. `web-forge/W-F-01-cargo-deps`).
- Commit subject: `<type>(<scope>): <imperative>` ; e.g. `feat(api): add repository DTOs`.
- Co-Authored-By footer: `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`
- Run `just fast` locally before push.

## Claims table

| ID | Agent | Started (UTC) | Status | Branch | Notes |
|---|---|---|---|---|---|
| W-F-00 | Claude Opus 4.7 | 2026-05-26 | done | web-forge/main | seed planning artifacts + claim tracker |

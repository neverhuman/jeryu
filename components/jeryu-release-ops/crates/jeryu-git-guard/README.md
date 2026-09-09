# jeryu-git-guard

Deny-by-default `git` command allowlist for confined coding agents.

A sandboxed agent works **only** on the single branch jeryu assigned it
(`JERYU_BRANCH`). It may inspect and edit that branch locally, but it must not
reach the network, spawn worktrees, create or switch branches, rewrite history,
or inject config/aliases that would escape the guard. Publishing is mediated by
jeryu — never by the agent calling `git push`.

## Shape

- [`git_command_decision`](src/lib.rs) — a **pure**, side-effect-free verdict
  function. The single source of truth, exhaustively unit-tested with no real
  git. Modeled on `jeryu-egress`'s `egress_decision`.
- `bin/jeryu-git` — the thin wrapper installed as the **only** `git` on the
  agent's `PATH`. It consults the verdict, then either `exec`s the real git
  (`JERYU_REAL_GIT`, default `/usr/bin/git`) or prints typed repair guidance and
  exits non-zero.

## Policy

| Category | Examples | Verdict |
| --- | --- | --- |
| Branch-local read/edit | `status` `diff` `log` `show` `add` `commit` `revert` `reset` `restore` `rm` `mv` `stash` | allow |
| Network / other repo | `push` `fetch` `pull` `clone` `remote` | deny (`git_network_denied`) |
| Extra checkouts | `worktree` `submodule` | deny (`git_worktree_denied`) — "more worktrees → fire up more runners" |
| History / integration | `merge` `rebase` `cherry-pick` `update-ref` `filter-branch` | deny (`git_integration_denied`) — jeryu integrates |
| Branch mutation / switch away | `branch <new>` `branch -d` `checkout -b` `switch other` | deny (`git_branch_denied`) |
| Config writes | `config user.x v` `config alias.x !sh` | deny (`git_config_write_denied`) |
| Config-injecting globals | `-c …` `--git-dir` `--work-tree` `--exec-path=` | deny (`git_global_flag_denied`) |
| Unknown / aliases | anything off the allowlist | deny (`git_subcommand_denied`) |

Network denial is belt-and-suspenders: the cell is also `--network none` with a
socket-blocking seccomp profile, so even a guard bypass cannot egress.

## Test

```
cargo test -p jeryu-git-guard --jobs 4
```

Owner: `platform-security`. See `agent/owner-map.json` / `agent/test-map.json`.

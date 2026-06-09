# Workcell — folder-jailed code editing

A **workcell** is a ready-to-go cell in which any code-writing actor (Jeryu's own
agents, `jekko`/`jnoccio-router`, Claude, Codex, or a `jailgun` tar drop) edits a
repository **confined to a single file tree**. The actor cannot read or write
outside the cell's checkout, cannot open direct network sockets, and cannot
escalate privileges; the only controlled network path is an allowlisted egress
proxy. When its work is ready it leaves the cell only as a **pull request**.

This is the foundation of the workcell north-star: *all* code editing happens
server-side inside the jail, and the only egress for the result is a reviewed PR.

## Security model — native, unprivileged jail

The cell jail is the production `jeryu-sandbox-linux` launch path. It needs **no
Docker and no `sudo`**: it composes unprivileged Linux kernel primitives.

| Primitive | Enforces |
| --- | --- |
| **Landlock** (filesystem LSM) | reads/writes are allowed only under the cell checkout (+ read-only system roots for the loader/libc); everything else is `EACCES` |
| **seccomp-bpf** | syscall allowlist; `AF_INET`/`AF_INET6` sockets are denied (`EPERM`) while `AF_UNIX`/`AF_NETLINK` are permitted |
| **`no_new_privs`** | a jailed process can never gain privileges via `exec` |
| **cgroups v2** | CPU / memory / PID pressure caps; **agent jobs are fail-closed** — they refuse to launch unless a delegated cgroup-v2 subtree is available to enforce the caps (a `setrlimit` fallback applies `RLIMIT_AS` as memory defence-in-depth) |

Resource confinement is **fail-closed for agent jobs**: `SandboxPlan.require_cgroup` makes `enforcement_level` return `Unavailable` (refusing the launch) when no delegated cgroup-v2 subtree exists, so a runaway in-cell agent can never run uncontained. Ordinary CI/build jobs keep `require_cgroup = false` (degrade, don't refuse). Deploy `ops/security/jeryu-runnerd.service` (`Delegate=memory pids cpu`) so the runner owns a writable subtree.

The launch path is `SandboxPlan::from_decision(workspace, &decision)` ->
`spawn_sandboxed(job, plan, caps, env)` -> `verify_enforcement(pid, level)`. When
a host genuinely lacks a primitive, the level degrades and the missing primitive
is reported as **skipped** — it is never silently treated as enforced.

## Rung 1 — live jail demo

`crates/jeryu-sandbox-linux/examples/jail_demo.rs` drives that exact launch path
against a throwaway checkout and has a sandboxed child attempt four operations:

| Attempt | Expected | Enforced by |
| --- | --- | --- |
| write a file **inside** the checkout | ALLOWED | Landlock (workspace rule) |
| write a file **outside** the checkout | DENIED | Landlock (`EACCES`) |
| read `/etc/shadow` | DENIED | Landlock (`EACCES`) |
| open an `AF_INET` TCP socket | DENIED | seccomp (`EPERM`) |

It prints the `/proc/<pid>/status` enforcement proof (`NoNewPrivs:1`,
`Seccomp:2` filter mode, `landlock` applied) and exits non-zero if any attempt
fails its expected verdict. A primitive the host lacks is honestly reported as
`skipped`, never faked as `DENIED`.

```sh
cargo run -p jeryu-sandbox-linux --example jail_demo
```

Run it on a fleet node (Landlock abi4 + seccomp present) to see all four enforced
for real.

## Rung 2 — jailgun tar round-trip

`jailgun` moves code in and out of a cell as a quarantine-first `tar.gz`.
`crates/jeryu-runnerd/tests/jailgun_roundtrip.rs` round-trips the public
validators `validate_import_archive` / `validate_export_paths`:

- a clean `File`/`Directory` subtree under an approved repo root imports **and**
  exports cleanly; while
- every adversarial entry — `../` parent traversal, an absolute path, a `Symlink`,
  a `CharacterDevice`, and a traversal smuggled into an otherwise-clean batch — is
  rejected with reason `workcell_tar_path_denied`, as is an export that resolves
  outside the approved roots.

```sh
cargo test -p jeryu-runnerd jailgun
```

## Rung 4 — in-cell agent driver

`crates/jeryu-agentbridge` drives a code-writing process **inside** a cell. The
`AgentDriver` builds a `JobRequest` confined to the cell workspace, spawns the
process via `spawn_sandboxed`, and supervises it:

- **watchdog** — a wall-clock deadline; a runaway is killed (`timed_out`).
- **output/token budget** — total captured stdout+stderr bytes are capped; the
  instant the budget is exceeded the child is killed and `budget_exceeded` is
  flagged (a placeholder for a richer token budget).
- **structured events** — `AgentEvent` (`Started`/`Stdout`/`Stderr`/`Budget`/
  `Finished`) is emitted through the `AgentEventSink` trait, so a WebSocket sink
  can stream live in-cell output to operators (WS wiring is the cell-surface lane).

The driver ships a deterministic edit-bot (`jeryu-editbot`) that writes a bounded
file inside the cell — the placeholder for a real `claude`/`codex` CLI, which
runs through the same jailed path.

## Cell Surface Run Route

`POST /api/v1/workcells/{id}/run_agent` is the HTTP control-plane entrypoint for
that in-cell driver. The request names the `workcell_id`, `runner_epoch`,
optional `repo_root`, staged program, args, environment, timeout, output budget,
and whether cgroup enforcement is required. The API only accepts claimed, held,
or repairing workcells; mismatched ids, stale epochs, missing repo roots, and
programs outside the claimed repo root return typed repair bodies.

The response is agent-readable evidence: selected `repo_root`, serialized
`Started`/`Stdout`/`Stderr`/`Budget`/`Finished` events, and the final outcome
with exit code, timeout flag, budget flag, captured bytes, enforcement level,
elapsed milliseconds, and `succeeded`. A host that cannot provide the required
sandbox returns `workcell_run_sandbox_unavailable` instead of pretending the run
was proven.

Proof lane:

```sh
cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent
```

## Agent Run Control Surface

`POST /api/v1/agent-runs` is the high-level repair-agent entrypoint. It keeps
`POST /api/v1/workcells/{id}/run_agent` as the deterministic command route, but
adds a live driver registry for real agents. Requests default to
`io_mode: "pty"` and may set `io_mode: "pipe"` for noninteractive capture.

The production route currently accepts `source.kind: "workcell"` with
`workcell_id` and `runner_epoch`. The workcell must be held or repairing after a
failed CI run; stale epochs, missing repo roots, and programs outside the
claimed repo-root slice return typed repair bodies. The launch remains
fail-closed on cgroup-v2 resource caps, so a host without the required delegated
subtree refuses the run instead of falling back to an unbounded process.

Workcell-sourced runs inject failure context when the held snapshot has it:
`JERYU_WORKCELL_ID`, `JERYU_RUNNER_EPOCH`, `JERYU_CI_RUN_ID`,
`JERYU_FAILED_RUN_ID`, `JERYU_FAILED_RECEIPT_ID`, and
`JERYU_FAILURE_LOG_DIGEST`.

`POST /api/v1/agent-runs/{id}/control` records the control envelope and forwards
PTY-capable commands to the live driver: `send_input`, `inject_prompt`,
`interrupt`, `terminate`, `resize_pty`, and `raise_budget`. Finished runs,
missing run ids, and pipe-mode controls return typed repair responses with
`purpose`, `reason`, `common_fixes`, `docs_url`, and `repair_hint`.

`GET /api/v1/agent-runs/{id}/events?after_seq=N&limit=M` returns cursor-safe
run events and broker-shaped `AgentTtyEvent` entries for resume-capable
subscribers. Start/status responses also include `events_url`, the live
`agent_run.{id}` WebSocket scope, `tty_topic`, `control_topic`, and
`export_pr_url`. `POST /api/v1/agent-runs/{id}/export_pr` exports finished
workcell-backed runs through the frozen-diff slice gate; unfinished and
non-workcell-backed runs fail with typed repair bodies.

`GET /api/v1/agent-runs/{id}/tty/stream?after_seq=N` is the
jpmc-subscribable Server-Sent-Events push transport for one run's raw TTY byte
stream. An outside service opens it once and is pushed raw bytes as they reach
the single publish point, with no cursor-polling of `agent_work.tail`. On
connect it replays the retained ring slice past `after_seq` (so a reconnect
catches up byte-for-byte), then hands over the live broadcast. Each `data:`
frame carries the same JSON shape a tail event does (`seq` + `stream` +
`bytes_b64`, raw non-UTF8 bytes ride through base64 intact). It mirrors the
WebSocket scope-membership rule: an unknown or non-member run returns the same
typed not-found. When a slow subscriber overflows the live broadcast, one
`event: resync` frame carrying `oldest_retained_seq` is pushed so the client
re-pulls the ring through `agent_work.tail` instead of stalling. The cursor-pull
`agent_work.tail` tool stays intact; this transport is additive.

MCP and CLI subscribers use the same run ids. The MCP tools are
`jeryu.agent_work.start`, `jeryu.agent_work.status`,
`jeryu.agent_work.control`, `jeryu.agent_work.events`, and
`jeryu.agent_work.export_pr`; the CLI grammar is
`jeryu agent auth|run|status|control|follow|export-pr`. Live CLI commands use
`--api-url` or `JERYU_API_URL`.
`jeryu-agent-stream` defines the broker-compatible `jeryu.agent.tty.v1` and
`jeryu.agent.control.v1` schemas, and `jeryu-agent-auth` imports only portable
native CLI auth into Jeryu-owned per-run homes.

Proof lane:

```sh
cargo test -p jeryu-api --features web --jobs 40 agent_runs
```

## Rung 4 — egress allowlist proxy

`crates/jeryu-egress` is a host-allowlist forward proxy (HTTP `CONNECT` + plain
HTTP). The decision is a pure, unit-tested function:

```rust
egress_decision(host, &allowlist, budget_exceeded) -> Allow | DenyNotAllowlisted | DenyBudget
```

- **Allowlist** — only vetted hosts (LLM APIs, `crates.io` family, the forge git
  hosts) are reachable; matching is exact-host **or** a true DNS-suffix on a dot
  boundary, never `str::contains` (so `crates.io.attacker.com` is denied).
- **Budget kill switch** — a shared `Budget` flag; once tripped, *every* request
  is denied (`DenyBudget`), including otherwise-allowlisted hosts, so a cell that
  blows its token budget loses egress immediately.
- A denied request gets a `403` and **no upstream connection is attempted**.

## Rung ladder

The workcell is built and demonstrated as a ladder of independently shippable
rungs, each landing as its own create-only PR through the self-hosted runner
fleet:

| Rung | Capability |
| --- | --- |
| R0 | jail + control-plane proven on the fleet (Landlock/seccomp deny matrix; `jeryu-runnerd` workcell control-plane) |
| **R1** | **live jail demo (this doc)** |
| **R2** | **jailgun tar round-trip (this doc)** |
| R3 | cell lifecycle surface — `claim`/`heartbeat`/`release` over HTTP + `workcell.{id}`/`agent.{id}` WS scopes + startup rebase on `origin/main` |
| **R4** | **in-cell agent driver + allowlist egress proxy (this doc)** |
| R5 | jailed agent: rebase -> edit -> namespaced branch (`agents/{id}/workcells/{wc}/<branch>`) -> jailgun-export -> PR -> green CI, host FS provably untouched; proof lane: `cargo test -p jeryu-api --features web --jobs 40 r5_jail_loop` |

## Export Slice Gate

Workcell export no longer trusts caller-supplied `changed_files`. The API derives
the PR changed-file evidence from the frozen `base_sha..head_sha` diff in the
managed bare repository, then checks it against the lease's allowed repo roots
before any pull request is created.

A whole-repo lease strips to an empty prefix and permits the diff. A restrictive
lease drops that empty workspace-root prefix and keeps only the specific repo
root prefixes, so a commit outside the slice is denied with
`workcell_export_slice_denied` and leaves the PR list unchanged.

Proof lanes:

- `cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent`
- `cargo test -p jeryu-api --features web --jobs 40 workcell_export_slice`
- `cargo test -p jeryu-api --features web --jobs 40 r5_jail_loop`

## Repair

- Jail demo verdict `FAIL` (an attempt did not match its expected kernel verdict):
  inspect the printed `/proc` proof; a real escape is a sandbox regression — see
  `crates/jeryu-sandbox-linux/src/launch.rs` and the `escape_suite` integration
  test. Rerun: `cargo run -p jeryu-sandbox-linux --example jail_demo`.
- Jailgun validator failure: a path that should round-trip was denied, or an
  adversarial path was admitted — see
  `jeryu_runnerd::workcell::validate_import_archive` / `validate_export_paths`.
  Rerun: `cargo test -p jeryu-runnerd jailgun`.
- Driver test failure: inspect the `AgentEvent` trace; a real out-of-cell write
  that *succeeds* is a sandbox regression — see `crates/jeryu-sandbox-linux`
  `escape_suite`. Rerun: `cargo test -p jeryu-agentbridge`.
- Run-agent route failure: inspect the typed body. `workcell_run_path_denied`
  means the staged program or requested repo root is outside the claimed slice;
  `workcell_run_sandbox_unavailable` means the host cannot prove the required
  sandbox. Rerun:
  `cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent`.
- Agent-run route failure: inspect the typed body. `agent_run_workcell_state_denied`
  means the selected workcell is not held or repairing; `workcell_epoch_fenced`
  means the request used a stale runner epoch; `agent_run_control_unsupported`
  means an interactive control was sent to a non-PTY run. Rerun:
  `cargo test -p jeryu-api --features web --jobs 40 agent_runs`.
- Egress denial of an expected host: extend the `Allowlist`; a denial of a
  non-allowlisted host is correct. Rerun: `cargo test -p jeryu-egress`.
- Export slice denial: confirm the failed diff path is intended to be outside
  the workcell's allowed repo roots. To widen the slice, reclaim the workcell
  with the required `repo_roots`; to keep the slice narrow, move the edit back
  under an allowed root. Rerun:
  `cargo test -p jeryu-api --features web --jobs 40 workcell_export_slice`.

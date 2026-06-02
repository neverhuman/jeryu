# Workcell — folder-jailed code editing

A **workcell** is a ready-to-go cell in which any code-writing actor (Jeryu's own
agents, `jekko`/`jnoccio-rtouer`, Claude, Codex, or a `jailgun` tar drop) edits a
repository **confined to a single file tree**. The actor cannot read or write
outside the cell's checkout, cannot open the network except through an
allowlisted egress proxy, and cannot escalate privileges; when its work is ready
it leaves the cell only as a **pull request**.

This is the foundation of the workcell north-star: *all* code editing happens
server-side inside the jail, and the only egress for the result is a reviewed PR.

## Security model — native, unprivileged jail

The cell jail is the production `jeryu-sandbox-linux` launch path. It needs **no
Docker and no `sudo`**: it composes unprivileged Linux kernel primitives.

| Primitive | Enforces |
| --- | --- |
| **Landlock** (filesystem LSM) | reads/writes allowed only under the cell checkout (+ read-only system roots for the loader/libc); everything else is `EACCES` |
| **seccomp-bpf** | syscall allowlist; `AF_INET`/`AF_INET6` sockets are denied (`EPERM`) while `AF_UNIX`/`AF_NETLINK` are permitted |
| **`no_new_privs`** | a jailed process can never gain privileges via `exec` |
| **cgroups v2** (when delegated) | CPU / memory / PID pressure caps |

The launch path is `SandboxPlan::from_decision(workspace, &decision)` ->
`spawn_sandboxed(job, plan, caps, env)` -> `verify_enforcement(pid, level)`. When
a host genuinely lacks a primitive, the level degrades and the missing primitive
is reported as **skipped** — it is never silently treated as enforced.

## In-cell agent driver (Rung 4)

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

## Egress allowlist proxy (Rung 4)

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
| R1 | live jail demo (`jeryu-sandbox-linux` `jail_demo`) |
| R2 | jailgun tar round-trip (`validate_import_archive` / `validate_export_paths`) |
| R3 | cell lifecycle surface — `claim`/`heartbeat`/`release` over HTTP + `workcell.{id}`/`agent.{id}` WS scopes + startup rebase on `origin/main` |
| **R4** | **in-cell agent driver + allowlist egress proxy (this PR)** |
| R5 | jailed agent: rebase -> edit -> namespaced branch (`agents/{id}/workcells/{wc}/<branch>`) -> jailgun-export -> PR -> green CI -> safe auto-merge, host FS provably untouched |

## Repair

- Driver test failure: inspect the `AgentEvent` trace; a real out-of-cell write
  that *succeeds* is a sandbox regression — see `crates/jeryu-sandbox-linux`
  `escape_suite`. Rerun: `cargo test -p jeryu-agentbridge`.
- Egress denial of an expected host: extend the `Allowlist`; a denial of a
  non-allowlisted host is correct. Rerun: `cargo test -p jeryu-egress`.

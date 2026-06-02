# Workcell — folder-jailed code editing

A **workcell** is a ready-to-go cell in which any code-writing actor (Jeryu's own
agents, `jekko`/`jnoccio-rtouer`, Claude, Codex, or a `jailgun` tar drop) edits a
repository **confined to a single file-tree**. The actor cannot read or write
outside the cell's checkout, cannot open the network, and cannot escalate
privileges; when its work is ready it leaves the cell only as a **pull request**.

This is the foundation of the workcell north-star: *all* code editing happens
server-side inside the jail, and the only egress for the result is a reviewed PR.

## Security model — native, unprivileged jail

The cell jail is the production `jeryu-sandbox-linux` launch path. It needs **no
Docker and no `sudo`**: it composes unprivileged Linux kernel primitives.

| Primitive | Enforces |
| --- | --- |
| **Landlock** (filesystem LSM) | reads/writes are allowed only under the cell checkout (+ read-only system roots for the loader/libc); everything else is `EACCES` |
| **seccomp-bpf** | syscall allowlist; e.g. `AF_INET`/`AF_INET6` sockets are denied (`EPERM`) while `AF_UNIX`/`AF_NETLINK` are permitted |
| **`no_new_privs`** | a jailed process can never gain privileges via `exec` |
| **cgroups v2** (when delegated) | pids/memory pressure caps |

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
| R4 | in-cell agent driver (deterministic edit-bot, then a real CLI) behind an allowlist egress proxy |
| R5 | jailed agent: rebase -> edit -> namespaced branch (`agents/{id}/workcells/{wc}/<branch>`) -> jailgun-export -> PR -> green CI, host FS provably untouched |

## Repair

- Jail demo verdict `FAIL` (an attempt did not match its expected kernel verdict):
  inspect the printed `/proc` proof; a real escape is a sandbox regression — see
  `crates/jeryu-sandbox-linux/src/launch.rs` and the `escape_suite` integration
  test. Rerun: `cargo run -p jeryu-sandbox-linux --example jail_demo`.
- Jailgun validator failure: a path that should round-trip was denied, or an
  adversarial path was admitted — see `jeryu_runnerd::workcell::validate_import_archive`
  / `validate_export_paths`. Rerun: `cargo test -p jeryu-runnerd jailgun`.

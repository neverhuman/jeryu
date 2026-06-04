# Agent Edit Master Plan

This is the active Codex implementation plan for Jeryu's agent-edit substrate.
The scope is contract-first and fail-closed until live tool execution is proven
inside the protected runner model.

## Goal

Jeryu should let API, CLI, and MCP callers request code work against a managed
repo, local import, or scratch repo. Jeryu claims or creates a workcell, prepares
per-run auth and home material, launches Codex, Claude, or Jekko only through the
existing sandboxed agent runner, streams terminal output/control through Kafka,
and exports permitted diffs as namespaced pull requests.

Existing guarantees remain load-bearing:

- Workcells, jailed `jeryu-agentbridge` runs, cgroup fail-closed enforcement,
  egress allowlisting, MCP workcell primitives, and export-slice PR gates are
  preserved.
- Fork, public, and untrusted jobs never write trusted compiled caches.
- Release jobs never consume mutable compiled artifacts.
- Agent jobs do not run without enforced resource caps.
- Native CLI bypass modes are allowed only behind the outer Jeryu sandbox,
  cgroup, egress proxy, export gate, and secret scanning.

## Public Surfaces

API:

- `POST /api/v1/agent-runs`
- `GET /api/v1/agent-runs/:id`
- `POST /api/v1/agent-runs/:id/control`
- `POST /api/v1/agent-runs/:id/export_pr`

MCP:

- `agent_work.start`
- `agent_work.status`
- `agent_work.control`
- `agent_work.export_pr`

CLI:

- `jeryu agent auth import|doctor`
- `jeryu agent run --repo owner/name --agent codex|claude|jekko --model MODEL --effort xhigh --task-file TASK`
- `jeryu agent status RUN_ID`
- `jeryu agent control RUN_ID --stdin "continue with ..."` or `--interrupt` or `--terminate`
- `jeryu agent export-pr RUN_ID --title TITLE`

## Shared Request Contract

Required fields:

- `source`: `repo`, `local_path`, or `scratch`
- `agent`: `codex`, `claude`, or `jekko`
- `prompt`
- `model`
- `base_ref`

Defaults:

- `effort = xhigh`
- `allowed_paths = [""]`
- `branch_suffix = agent-edit`
- `budget.wall_secs = 7200`
- `budget.output_bytes = 20971520`
- `stream.required = true`

Response fields:

- `agent_run_id`
- `workcell_id`
- `runner_id`
- `runner_epoch`
- `status_url`
- `control_topic`
- `tty_topic`
- `export_pr_url`

## Kafka Contract

Output topic: `jeryu.agent.tty.v1`, keyed by `agent_run_id`.

Control topic: `jeryu.agent.control.v1`, keyed by `agent_run_id`.

Agent events carry:

- `schema_version`
- `event_id`
- `seq`
- `occurred_at_ms`
- `repo`
- `workcell_id`
- `agent_run_id`
- `agent`
- `model`
- `direction`
- `stream`
- `text`
- `bytes_b64`
- `truncated`
- `budget`
- `exit_code`
- `enforcement_level`

Control kinds:

- `stdin_text`
- `continue_prompt`
- `interrupt`
- `terminate`
- `resize_pty`

## Implementation Stages

1. Create this plan and map it in owner/test maps.
2. Add typed agent-run contracts, state, and repair errors.
3. Add an agent-edit tool manifest and runner doctor evidence for Codex,
   Claude, Jekko, and Jankurai versions.
4. Add `jeryu-agent-auth` for portable auth import, doctor checks, per-run
   homes, strict permissions, and host-bound-auth typed denials.
5. Add `jeryu-agent-stream` for in-memory tests and Kafka-backed event/control
   adapters.
6. Extend the agentbridge launch path toward PTY/stdin control while preserving
   sandbox `pre_exec` enforcement.
7. Add `NetworkPolicy::EgressProxyOnly`; direct `AF_INET` remains denied unless
   the configured proxy guard is attached and proven.
8. Wire API/MCP/CLI orchestration: resolve repo, claim workcell, materialize
   auth/home, verify tool/Kafka/egress/cgroup/Landlock/seccomp, launch, stream,
   heartbeat, freeze diff, and export through existing slice gates.
9. Inject Jeryu/Jankurai prompt guidance, run `jankurai doctor`, and run
   `jankurai diff-audit --changed-from <base_ref>` before PR export when
   available.

Until every launch preflight is proven, public start surfaces must deny with a
typed repair body. A missing Kafka stream, missing auth, missing tool, missing
netguard, or missing sandbox enforcement is a hard denial, not a degraded run.

## Required Proof Lanes

- `cargo test -p jeryu-agent-auth --jobs 40`
- `cargo test -p jeryu-agent-stream --jobs 40`
- `cargo test -p jeryu-agentbridge -p jeryu-egress --jobs 40`
- `cargo test -p jeryu-sandbox-linux --jobs 40`
- `cargo test -p jeryu-runnerd workcell --jobs 40`
- `cargo test -p jeryu-api --features web --jobs 40`
- `cargo test -p jeryu-mcp --jobs 40`
- `cargo test -p jeryu-cli --jobs 40`
- `./scripts/check-owner-test-map.sh`
- `./scripts/check-agent-maps.sh`
- `cargo run -q -p jeryu-mapcheck -- generated-zones`

Live smoke lanes are opt-in only with both `JERYU_ALLOW_NETWORK_TOOLS=1` and
`JERYU_AGENT_LIVE_SMOKE=codex|claude|jekko`, and must publish budget receipts
plus a STOP-file kill switch before they are considered supported.

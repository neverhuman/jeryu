# Docs Agent Guidance

Owns:
- Architecture, testing, error repair, boundary, generated-zone, audit, and
  release-control documentation.
- Keeping root `AGENTS.md` and `README.md` routed to the same canonical docs.
- Workcell export-slice documentation in `docs/workcell.md`, including the
  release and testing proof commands for typed no-PR denial evidence.
- Workcell run-agent documentation in `docs/workcell.md`, including the route
  proof command for typed path denial and structured event evidence.
- Agent-run control documentation in `docs/workcell.md`, including typed control
  denials, live PTY controls, failed-CI workcell source rules, and proof lane.
- Codegraph oracle route/tool documentation, including schema-v3 MCP/API proof
  commands.
- Codegraph tool-build insight documentation, including fast cluster polling,
  feedback suppression, and MCP/API/CLI proof commands.

Forbidden:
- Hosted-provider or retired review-request terminology.
- Aspirational release claims without executable gate evidence.
- Generated artifact edits outside `agent/generated-zones.toml`.

Proof lane:
- `cargo run -q -p jeryu-mapcheck -- docs`
- `cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent`
  when workcell run-agent route docs change.
- `cargo test -p jeryu-api --features web --jobs 40 agent_runs`
  when agent-run control route docs change.
- `bash ops/ci/codegraph-oracle.sh`
  when codegraph oracle API/MCP docs change.
- `cargo test -p jeryu-api --features web --jobs 40 workcell_export_slice`
  when workcell export-slice docs change.
- `bash ci-fast-push.sh --no-push` before release-facing docs are signed.

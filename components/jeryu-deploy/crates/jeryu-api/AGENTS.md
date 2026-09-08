# jeryu-api Agent Guidance

Owns:
- GitHub-compatible REST response shapes.
- Guided GraphQL repair responses.
- Local Axum web/API edge under the `web` feature.
- Push-to-CI bridge behavior, including the local `main` handoff to
  `jeryu-wsversion` for workspace version bump commits.
- Workcell export PR gates, including frozen-diff changed-file evidence and
  typed no-PR denial for out-of-slice workcell repairs.
- Workcell run-agent route behavior, including epoch fencing, claimed-repo-root
  confinement, structured run events, and sandbox-unavailable repair guidance.
- High-level agent-run route behavior, including held/repairing failed-CI
  workcell sources, PTY controls, live event capture, cgroup fail-closed
  launches, and typed control denials.
- Codegraph oracle REST facade behavior and typed repair responses.

Forbidden:
- Broad GraphQL execution without a narrow conformance test.
- Provider-source fixtures or copied external API specs.
- String-only errors without `documentation_url` or `jeryu_repair_hint` for
  guided compatibility gaps.

Proof lane:
- `cargo test -p jeryu-api --features web --jobs 40`
- `cargo test -p jeryu-api --features web --jobs 40 ci_bridge`
- `cargo test -p jeryu-api --features web --jobs 40 workcell_run_agent`
- `cargo test -p jeryu-api --features web --jobs 40 agent_runs`
- `cargo test -p jeryu-api --features web --jobs 40 codegraph`
- `cargo test -p jeryu-api --features web --jobs 40 workcell_export_slice`
- `cargo clippy -p jeryu-api --features web --all-targets --jobs 40 -- -D warnings`

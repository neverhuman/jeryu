# Agent Index

Generated: `2026-05-04T06:27:09.384845098+00:00`

| Module | Change Type | Proof Commands | Owner |
|---|---|---|---|
| `src/admission.rs` | `security-relevant` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo test -p jeryu -- secrets exec honeypot admission | Git Hook Admission Control |
| `src/agent.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Autonomous Agent System |
| `src/agent_surface.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Agent Surface |
| `src/bootstrap.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Bootstrap subsystem |
| `src/buildkit.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | BuildKit Configuration (Per-Trust-Namespace Rootless Builders) |
| `src/cache.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | SmartCache & Disk Management |
| `src/cache_brain.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Cache Decision Brain (Trust + Taint + Epoch Integration) |
| `src/cache_proxy.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Cache Proxy (sccache TCP Proxy) |
| `src/capability.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Capability API (Structured AgentIntent Payloads) |
| `src/capsule.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Failure Capsule subsystem |
| `src/cargo_cache.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Cargo cache layout and local agent helpers |
| `src/cli.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | CLI Definitions |
| `src/commands/git.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | CLI Git wrappers |
| `src/commands/host.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | - |
| `src/commands/install.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | Install command wrappers |
| `src/commands/job.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | - |
| `src/commands/mirror.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | Mirror command wrappers |
| `src/commands/mod.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | - |
| `src/commands/pipeline.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | - |
| `src/commands/pool.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | - |
| `src/commands/release.rs` | `release-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | - |
| `src/commands/remote.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | Remote command wrappers |
| `src/commands/repo.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Repo-local maintenance command wrappers |
| `src/commands/secrets.rs` | `security-relevant` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo test -p jeryu -- secrets exec honeypot admission | - |
| `src/commands/settings.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | Settings repair/reset commands |
| `src/commands/system.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | - |
| `src/commands/test.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | - |
| `src/config.rs` | `cross-module` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo nextest run -p jeryu | Configuration & Templates subsystem |
| `src/decision.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Agent Decision Engine (Risk Gates, Supersedence, Impact Classification) |
| `src/dispatch.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | CLI Dispatch |
| `src/docker.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Docker Control Plane subsystem |
| `src/engine.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | Engine Core (Webhook + Reconciliation) |
| `src/epoch.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Epoch-Based Cache Invalidation |
| `src/exec.rs` | `security-relevant` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo test -p jeryu -- secrets exec honeypot admission | Custom Executor & Sandbox Isolation |
| `src/explain.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Pipeline Explain subsystem |
| `src/gateway/cargo.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Cache Gateway subsystem — Cargo registry proxy |
| `src/gateway/git.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Cache Gateway subsystem — Git objects proxy |
| `src/gateway/mod.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Cache Gateway subsystem (module root) |
| `src/gateway/npm.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Cache Gateway subsystem — npm registry proxy |
| `src/gateway/oci.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Cache Gateway subsystem — OCI image proxy |
| `src/gateway/singleflight.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Cache Gateway subsystem — singleflight deduplication |
| `src/git/classify.rs` | `cross-module` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo nextest run -p jeryu | Git command classification |
| `src/git/event.rs` | `cross-module` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo nextest run -p jeryu | Git event record model |
| `src/git/executor.rs` | `cross-module` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo nextest run -p jeryu | Git passthrough execution and event recording |
| `src/git/invocation.rs` | `cross-module` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo nextest run -p jeryu | Git invocation model |
| `src/git/mirror.rs` | `cross-module` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo nextest run -p jeryu | Git mirror helper |
| `src/git/mod.rs` | `cross-module` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo nextest run -p jeryu | Git event plane and passthrough executor |
| `src/git/policy.rs` | `cross-module` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo nextest run -p jeryu | Git execution policy |
| `src/git/receipt.rs` | `cross-module` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo nextest run -p jeryu | Git execution receipts |
| `src/git/shim.rs` | `cross-module` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo nextest run -p jeryu | Git shim helpers |
| `src/git/snapshot.rs` | `cross-module` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo nextest run -p jeryu | Git repository state snapshot |
| `src/git/store.rs` | `cross-module` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo nextest run -p jeryu | Git event persistence |
| `src/git/system.rs` | `cross-module` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo nextest run -p jeryu | System Git resolution |
| `src/gitlab_client.rs` | `cross-module` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo nextest run -p jeryu | GitLab REST Client subsystem |
| `src/honeypot.rs` | `security-relevant` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo test -p jeryu -- secrets exec honeypot admission | Supply-Chain Detonation / Honey Token Detection |
| `src/host.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | - |
| `src/impact.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Change Impact Analysis |
| `src/install.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | Local installer and guided bootstrap UX |
| `src/install_demo.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Install demo renderer |
| `src/lib.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | jeryu crate root (see module map below) |
| `src/local.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Local agent command wrappers |
| `src/logs.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Logging & Observability subsystem |
| `src/main.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | CLI dispatcher — no business logic |
| `src/mcp.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | MCP adapter for external coding agents |
| `src/mcp/core.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | MCP adapter for external coding agents |
| `src/mcp/http.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | MCP adapter for external coding agents |
| `src/mcp/tests.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | MCP adapter for external coding agents |
| `src/mcp/tools.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | MCP adapter for external coding agents |
| `src/policy.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Trust Policy (TrustTier, Cache Promotion Gates) |
| `src/pool.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | Runner Fleet / Pool Management |
| `src/reclaim.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Storage Audit & GC |
| `src/redact.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | cross-cutting redaction helpers |
| `src/release.rs` | `release-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | Release Pipeline |
| `src/remote.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | Remote SSH install and day-two management UX |
| `src/repo.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | - |
| `src/sandbox.rs` | `security-relevant` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo test -p jeryu -- secrets exec honeypot admission | Workload Sandbox (Network-Namespace Isolation) |
| `src/sccache_mgr.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | sccache Management subsystem |
| `src/secrets.rs` | `security-relevant` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo test -p jeryu -- secrets exec honeypot admission | Secrets & Vault Lifecycle |
| `src/settings.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | User settings subsystem |
| `src/shadow.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Shadow Remote Mirroring |
| `src/state.rs` | `state-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | State Store (Postgres primary, SQLite fallback) |
| `src/taint.rs` | `security-relevant` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1<br>cargo test -p jeryu -- secrets exec honeypot admission | Taint Tracking (Detonation Lane) |
| `src/telemetry.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Runner Telemetry subsystem |
| `src/test_intel/cache.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | VTI Test Intelligence subsystem — plan cache |
| `src/test_intel/ci_gen.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | VTI Test Intelligence subsystem — CI pipeline generation |
| `src/test_intel/explain.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | VTI Test Intelligence subsystem — plan explanation |
| `src/test_intel/mod.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | VTI Test Intelligence subsystem (module root) |
| `src/test_intel/nightly.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | VTI Test Intelligence subsystem — nightly oracle |
| `src/test_intel/planner.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | VTI Test Intelligence subsystem — test plan algorithm |
| `src/test_intel/subsystem.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | VTI Test Intelligence subsystem — subsystem ownership graph |
| `src/test_intel/testmap.rs` | `api-change` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib<br>cargo test -p jeryu --test '*' -- --test-threads=1 | VTI Test Intelligence subsystem — testmap.toml parser |
| `src/test_runner.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | CI Test Runner subsystem |
| `src/tui/action_registry.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | TUI action surface and capability action contract. |
| `src/tui/app.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Interactive TUI subsystem — application state and refresh loop |
| `src/tui/events.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Interactive TUI subsystem — event handling stubs |
| `src/tui/flow/builder.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Interactive TUI subsystem — flow graph builder |
| `src/tui/flow/collector.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Interactive TUI subsystem — flow snapshot collector |
| `src/tui/flow/eta.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Interactive TUI subsystem — flow ETA estimation |
| `src/tui/flow/inspector.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Interactive TUI subsystem — flow inspector pane |
| `src/tui/flow/mod.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Interactive TUI subsystem — CI flow view |
| `src/tui/flow/model.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Interactive TUI subsystem — flow data model |
| `src/tui/flow/widget.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Interactive TUI subsystem — flow graph widget |
| `src/tui/graph.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Interactive TUI subsystem — pipeline graph rendering |
| `src/tui/mod.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Interactive TUI subsystem (module root) |
| `src/tui/ui.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Interactive TUI subsystem — rendering logic |
| `src/witness.rs` | `leaf-bugfix` | cargo check -p jeryu --message-format=json<br>cargo nextest run -p jeryu --lib | Build Witness (Cacheability Classification) |

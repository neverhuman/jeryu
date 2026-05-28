# jankurai Standard Agent Bootstrap

Standard version: `0.9.0`

Read `docs/agent-native-standard.md` when policy detail matters. Use `agent/owner-map.json`, `agent/test-map.json`, `agent/generated-zones.toml`, `agent/proof-lanes.toml`, `agent/tool-adoption.toml`, and `agent/boundaries.toml` before editing.

GitLab auth is canonicalized through `~/.jeryu/jeryu.env`. For local GitLab work, use `jeryu init` / `jeryu bootstrap` or the repair helpers in code (`gitlab_auth::resolve_or_repair_default()` / `GitLabClient::from_jeryu_env_or_repair()`), not ad hoc shell probing for `GITLAB_PAT`. When a task can be handled through jeryu's own APIs, prefer those surfaces over talking to GitLab directly.

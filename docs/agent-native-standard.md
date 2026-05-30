# Agent Native Standard

Agents work from local evidence first:

- Read `AGENTS.md`, `agent/owner-map.json`, `agent/test-map.json`, and any local
  `AGENTS.md` in the changed path.
- Use the narrowest mapped proof lane before broader workspace gates.
- Keep all user-visible forge behavior PR/GitHub-compatible and local.
- Do not introduce provider aliases, hidden compatibility shims, or string-only
  repair instructions.

Every mutation should leave a runnable proof command, a receipt or test when the
surface is stateful, and a coordination note in `AGENT_CHAT.md`.

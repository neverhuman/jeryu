// agentTerminal.ts — frontend-local wire contract for the live agent terminal.
//
// The backend streams `AgentTtyEvent`s over the realtime socket on the
// `agent_run.{id}` scope; each `WebEvent.payload` for that scope carries the
// raw output chunk (`AgentTtyFrame`). The control channel is the mirror image:
// the SPA drives the sandboxed TTY by sending `agent_control` client frames
// (`AgentControl`). The active-agents list is provided by
// `GET /api/v1/repos/{id}/agent-runs` (`RepoAgentSummary`).
//
// These shapes (and the URL builder) are frontend-local until the Rust
// read-model exporter owns them via ts-rs — mirroring the W-FE-11 pattern in
// `types.ts`. They live in their own module rather than `types.ts`/`endpoints.ts`
// to keep this self-contained workstream out of those shared files.

/** A single decoded chunk of agent terminal output (the `agent_run.{id}`
 *  WebEvent payload). `bytes_b64` is the raw PTY bytes, base64-encoded so
 *  arbitrary control sequences survive the JSON transport intact. */
export interface AgentTtyFrame {
  /** Monotonic per-run chunk cursor; a gap means we missed bytes and must
   *  clear + resync. */
  chunk_seq: number;
  /** Logical output stream the originating stream of the bytes. */
  stream: 'pty' | 'stdout' | 'stderr' | 'event';
  /** Base64-encoded raw bytes. */
  bytes_b64: string;
}

/** A control command the operator drives into the sandboxed agent TTY. The
 *  union is intentionally small and transport-shaped; the backend maps it onto
 *  its richer `AgentControlCommand` enum. */
export type AgentControl =
  | { kind: 'input'; bytes_b64: string }
  | { kind: 'interrupt' }
  | { kind: 'resize'; cols: number; rows: number }
  | { kind: 'resync' };

/** Client→server control frame carrying an `AgentControl` for a run. Sent via
 *  `JeRyuWsClient.sendControl`. Frontend-local: not part of the generated
 *  `ClientWsMessage` union yet. */
export interface AgentControlClientMessage {
  type: 'agent_control';
  run_id: string;
  control: AgentControl;
}

/** One active (or recently-active) agent run attached to a repository, as
 *  surfaced by `GET /api/v1/repos/{id}/agent-runs`. */
export interface RepoAgentSummary {
  run_id: string;
  /** Working branch the agent operates on. */
  branch: string;
  /** Runner / node id executing the run. */
  runner: string;
  /** Lifecycle state, e.g. `running` | `blocked` | `exited` | `queued`. */
  status: string;
  /** True while the run is streaming live TTY output. */
  tty_live: boolean;
  /** Companion shell run id for split terminal. */
  shell_run_id?: string;
  /** Agent tool id, when known. */
  agent?: string | null;
  /** Workcell id, when known. */
  workcell_id?: string | null;
  /** RFC3339 timestamp of the latest change. */
  updated_at?: string | null;
}

/** Wire shape of `GET /api/v1/repos/{id}/agent-runs`. */
export interface RepoAgentRunsResponse {
  items: RepoAgentSummary[];
}

/** Wire shape of `POST /api/v1/repos/{id}/sessions` — the freshly created,
 *  isolated agent session. `run_id` names the run the SPA then deep-links to
 *  and mounts the live `<AgentTerminal>` on; the remaining fields locate the
 *  session's realtime scope, control channel, and status surfaces. */
export interface CreateSessionResponse {
  /** The created agent run id (scope `agent_run.{run_id}`). */
  run_id: string;
  /** The isolated working branch the session operates on. */
  branch: string;
  /** Commit the branch was cut from. */
  base_oid: string;
  /** Realtime scope the session's activity is published on. */
  ws_scope: string;
  /** Realtime topic carrying the session's live TTY frames. */
  tty_topic: string;
  /** URL the operator drives control intents into. */
  control_url: string;
  /** URL the session's lifecycle status is polled from. */
  status_url: string;
  /** Companion shell run id — ready immediately for the split terminal. */
  shell_run_id?: string;
}

/** Typed URL builder for the per-repo agent-runs collection. */
export function repoAgentRunsPath(repoId: string): string {
  return `/api/v1/repos/${encodeURIComponent(repoId)}/agent-runs`;
}

// agentControlTransport.ts — REST-based control channel for agent runs.
//
// Sends operator commands (input, interrupt, resize) to the backend via
// POST /api/v1/agent-runs/{id}/control. Each command is fire-and-forget;
// the backend acks with { accepted: true } or returns an error.

const CONTROL_URL = (runId: string) =>
  `/api/v1/agent-runs/${encodeURIComponent(runId)}/control`;

interface ControlResponse {
  accepted?: boolean;
  agent_run_id?: string;
  command?: string;
  control_seq?: number;
}

async function sendControl(
  runId: string,
  body: Record<string, unknown>,
): Promise<ControlResponse | null> {
  try {
    const res = await fetch(CONTROL_URL(runId), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (res.ok) {
      return (await res.json()) as ControlResponse;
    }
    return null;
  } catch {
    return null;
  }
}

/** Send raw text input to the agent's PTY stdin. */
export function sendInput(runId: string, text: string): Promise<ControlResponse | null> {
  return sendControl(runId, { kind: 'send_input', text });
}

/** Send SIGINT to the agent process. */
export function sendInterrupt(runId: string): Promise<ControlResponse | null> {
  return sendControl(runId, { kind: 'interrupt' });
}

/** Notify the agent that the terminal viewport has resized. */
export function sendResize(
  runId: string,
  cols: number,
  rows: number,
): Promise<ControlResponse | null> {
  return sendControl(runId, { kind: 'resize_pty', cols, rows });
}

/** Terminate the agent process. */
export function sendTerminate(runId: string): Promise<ControlResponse | null> {
  return sendControl(runId, { kind: 'terminate' });
}

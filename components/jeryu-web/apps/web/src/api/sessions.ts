// sessions.ts — client call for creating an isolated agent session.
//
// `POST /api/v1/repos/{id}/sessions` provisions a fresh, isolated agent run on
// a repository and returns the run id + realtime coordinates the SPA needs to
// deep-link to and mount the live `<AgentTerminal>`. A per-attempt
// `Idempotency-Key` (§35.1.3) is attached so a retried click collapses to a
// single server-side session rather than spawning duplicates.

import { apiSend, type ApiRequestOptions } from './client';
import { endpoints } from './endpoints';
import type { CreateSessionResponse } from './agentTerminal';

function newIdempotencyKey(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `repo-session-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

/**
 * Create a new isolated agent session on `repoId`. Resolves with the created
 * run's coordinates; rejects with an `ApiError` on a non-2xx response.
 */
export function createSession(
  repoId: string,
  agentId: string,
  opts?: ApiRequestOptions
): Promise<CreateSessionResponse> {
  return apiSend<CreateSessionResponse>(
    endpoints.repoSessions(repoId),
    { agent_id: agentId },
    { idempotencyKey: newIdempotencyKey(), ...opts }
  );
}

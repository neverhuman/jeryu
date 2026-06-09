import { afterEach, describe, expect, it, vi } from 'vitest';

import { ApiError } from '../client';
import { createSession } from '../sessions';
import type { CreateSessionResponse } from '../agentTerminal';

function sessionBody(): CreateSessionResponse {
  return {
    run_id: 'run-xyz',
    branch: 'agent/session-xyz',
    base_oid: 'a'.repeat(40),
    ws_scope: 'agent_run.run-xyz',
    tty_topic: 'agent_run.run-xyz.tty',
    control_url: '/api/v1/agent-runs/run-xyz/control',
    status_url: '/api/v1/agent-runs/run-xyz/status',
  };
}

describe('createSession', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('POSTs to /repos/{id}/sessions with an idempotency key and parses the run', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(
        new Response(JSON.stringify(sessionBody()), {
          status: 201,
          headers: { 'content-type': 'application/json' },
        })
      );

    const result = await createSession('repo-1', 'codex');

    expect(result.run_id).toBe('run-xyz');
    expect(result.branch).toBe('agent/session-xyz');

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const [url, init] = fetchSpy.mock.calls[0];
    expect(String(url)).toBe('/api/v1/repos/repo-1/sessions');
    expect(init?.method).toBe('POST');
    expect(JSON.parse(String(init?.body))).toEqual({ agent_id: 'codex' });
    const headers = new Headers(init?.headers);
    expect(headers.get('Idempotency-Key')).toBeTruthy();
    expect(headers.get('Content-Type')).toContain('application/json');
  });

  it('encodes the repo id into the path', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(
        new Response(JSON.stringify(sessionBody()), {
          status: 201,
          headers: { 'content-type': 'application/json' },
        })
      );

    await createSession('group/repo', 'codex');
    expect(String(fetchSpy.mock.calls[0][0])).toBe(
      '/api/v1/repos/group%2Frepo/sessions'
    );
  });

  it('rejects with an ApiError carrying the backend envelope on failure', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          error: { code: 'capacity_exhausted', message: 'No runner slots free.' },
        }),
        { status: 503, headers: { 'content-type': 'application/json' } }
      )
    );

    await expect(createSession('repo-1', 'codex')).rejects.toMatchObject({
      name: 'ApiError',
      status: 503,
      code: 'capacity_exhausted',
    });
    // The thrown value is a real ApiError instance, not a bare object.
    await expect(createSession('repo-1', 'codex')).rejects.toBeInstanceOf(ApiError);
  });
});

import { afterEach, describe, expect, it, vi } from 'vitest';

import { apiGet, apiSend, setCsrfToken } from '../client';

describe('api client CSRF header', () => {
  afterEach(() => {
    setCsrfToken(null);
    vi.restoreAllMocks();
  });

  it('attaches the Jeryu CSRF token to unsafe requests only', async () => {
    const seen: Array<{ method: string; csrf: string | null }> = [];
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (_input, init) => {
      const headers = new Headers(init?.headers);
      seen.push({
        method: init?.method ?? 'GET',
        csrf: headers.get('x-jeryu-csrf'),
      });
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });

    setCsrfToken('csrf-test-token');
    await apiGet('/api/v1/auth/me');
    await apiSend('/api/v1/auth/tokens', { name: 'cli' });

    expect(seen).toEqual([
      { method: 'GET', csrf: null },
      { method: 'POST', csrf: 'csrf-test-token' },
    ]);
  });
});

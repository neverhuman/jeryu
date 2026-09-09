// server.ts — MSW handlers + bootstrap helpers (W-T-05 start).
//
// Tests import `handlers` and combine with `setupServer` (Node) or
// `setupWorker` (browser). Phase 1 ships only `/api/v1/bootstrap` and a
// no-op `/api/v1/ws` so component tests that wait for bootstrap can run.
//
// The browser worker bootstrap is left to W-T-08; here we expose just the
// handler list so consumers compose the runtime they need.

import { http, HttpResponse } from 'msw';

import { makeBootstrapFixture } from './mocks';
import type { WebBootstrap } from '../api/types';

export const handlers = [
  http.get('/api/v1/bootstrap', () =>
    HttpResponse.json(makeBootstrapFixture())
  ),
  // The websocket route returns a 426 so the test environment does not
  // attempt to upgrade. Vitest tests for WS lifecycle inject a fake
  // WebSocket constructor instead.
  http.get('/api/v1/ws', () =>
    HttpResponse.text('Upgrade required', { status: 426 })
  ),
];

export function bootstrapResponse(
  override: Partial<WebBootstrap> = {}
): WebBootstrap {
  return makeBootstrapFixture(override);
}

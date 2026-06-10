// router.test.tsx — route-map registration + negative authorization proof.
//
// The route table is an authorization-relevant surface: it decides which
// component owns a URL. Two contracts under test:
//   1. `repos/family/:family` is registered ABOVE the `repos/:provider/*`
//      catch-all so the static `family` segment wins over the dynamic
//      provider param.
//   2. Driving the REAL registered family route element as a non-owner
//      viewer — the list endpoint answers 403 forbidden — renders the
//      permission-denied state and leaks zero repository data into the DOM.
//      The route element is pulled from the live route table (not re-built
//      by the test), so a routing regression fails here, while the heavy
//      app shell (realtime, keyboard, theme plumbing) stays out of scope.

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { createMemoryRouter, RouterProvider } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { router } from '../router';

const FAMILY_PATH = 'repos/family/:family';
const CATCH_ALL_PATH = 'repos/:provider/*';

function topLevelRoutes(): Array<{ path?: string; element?: unknown }> {
  return router.routes[0]?.children ?? [];
}

describe('router route table', () => {
  it('registers repos/family/:family above the repos/:provider/* catch-all', () => {
    const paths = topLevelRoutes().map((r) => r.path ?? '(index)');
    const familyIdx = paths.indexOf(FAMILY_PATH);
    const catchAllIdx = paths.indexOf(CATCH_ALL_PATH);
    expect(familyIdx).toBeGreaterThan(-1);
    expect(catchAllIdx).toBeGreaterThan(-1);
    expect(familyIdx).toBeLessThan(catchAllIdx);
  });
});

describe('router negative authorization (non-owner viewer)', () => {
  beforeEach(() => {
    // Non-owner viewer: every repo read comes back 403 forbidden.
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input) => {
      const rawUrl = input instanceof Request ? input.url : String(input);
      const { pathname } = new URL(rawUrl, 'http://localhost');
      if (pathname.startsWith('/api/v1/repos')) {
        return new Response(
          JSON.stringify({
            error: {
              code: 'permission_denied',
              message: 'missing repo.read',
            },
          }),
          { status: 403, headers: { 'content-type': 'application/json' } }
        );
      }
      return new Response(
        JSON.stringify({
          error: { code: 'not_found', message: pathname },
        }),
        { status: 404, headers: { 'content-type': 'application/json' } }
      );
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders permission denied on the family route and leaks no repo data', async () => {
    const familyRoute = topLevelRoutes().find(
      (r) => r.path === FAMILY_PATH
    );
    expect(familyRoute?.element).toBeTruthy();

    const memoryRouter = createMemoryRouter(
      [
        {
          path: `/${FAMILY_PATH}`,
          element: familyRoute?.element as JSX.Element,
        },
      ],
      { initialEntries: ['/repos/family/veox-split'] }
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={client}>
        <RouterProvider router={memoryRouter} />
      </QueryClientProvider>
    );

    // The 403 forbidden response surfaces the permission-denied state…
    expect(await screen.findByText('Permission denied')).toBeInTheDocument();
    expect(screen.getByText(/missing: repo\.read/)).toBeInTheDocument();
    // …on the family page (the registered element owns the URL)…
    expect(
      screen.getByTestId('repository-family-page')
    ).toBeInTheDocument();
    // …and the non-owner viewer sees zero repository data.
    expect(document.querySelector('.repo-card')).toBeNull();
    expect(
      screen.queryByRole('region', { name: /repositories/ })
    ).not.toBeInTheDocument();
  });
});

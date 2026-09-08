import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { RepositoryOverviewPage } from '../RepositoryOverviewPage';

describe('RepositoryOverviewPage', () => {
  beforeEach(() => {
    vi.stubGlobal(
      'WebSocket',
      class {
        static OPEN = 1;
        readyState = 1;
        addEventListener(): void {}
        removeEventListener(): void {}
        send(): void {}
        close(): void {}
      }
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('routes split repositories into the family browser', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input) => {
      const parsed = new URL(String(input), 'http://localhost');
      if (parsed.pathname === '/api/v1/repos') {
        return jsonResponse({
          generated_at: '2026-05-26T00:00:00Z',
          total: 1,
          repositories: [repoSummary()],
          facets: {
            hosts: ['jeryu'],
            owners: ['neverhuman'],
            families: ['jeryu-split'],
            languages: [],
          },
        });
      }
      if (parsed.pathname === '/api/v1/repos/repo-1/refs') {
        return jsonResponse([
          { name: 'main', sha: 'abc123', kind: 'branch', protected: true },
        ]);
      }
      if (parsed.pathname === '/api/v1/repos/repo-1/readme') {
        return jsonResponse({
          html: '<p>Portal</p>',
          toc: [],
          links: [],
          renderer_version: 'test',
          sanitizer_version: null,
          rendered_at: '2026-05-26T00:00:00Z',
        });
      }
      return jsonResponse({ error: { code: 'not_found', message: parsed.pathname } }, 404);
    });

    renderPage();

    expect(await screen.findByText('Split family browser')).toBeInTheDocument();
  });
});

function renderPage(): void {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={['/repos/jeryu/neverhuman/jeryu']}>
        <Routes>
          <Route
            path="/repos/:provider/:fullName/*"
            element={<RepositoryOverviewPage />}
          />
          <Route
            path="/repos/family/:family"
            element={<div>Split family browser</div>}
          />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>
  );
}

function repoSummary(): Record<string, unknown> {
  return {
    id: {
      id: 'repo-1',
      host: 'jeryu',
      owner: 'neverhuman',
      name: 'jeryu',
    },
    entity: { kind: 'repository', id: 'repo-1' },
    description: 'Portal repo',
    visibility: 'public',
    default_branch: 'main',
    family: 'jeryu-split',
    repo_role: 'public_portal',
    topics: [],
    language: null,
    health: 'healthy',
    open_pull_requests: 0,
    failing_checks: 0,
    running_jobs: 0,
    active_agents: 0,
    blocked_agents: 0,
    updated_at: '2026-05-26T00:00:00Z',
    clone_http_url: null,
    clone_ssh_url: null,
    available_actions: [],
  };
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

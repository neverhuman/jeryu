import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { RepositoryPullRequestsPage } from '../RepositoryPullRequestsPage';

describe('RepositoryPullRequestsPage', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('resolves the repo and renders repo-scoped pull requests', async () => {
    mockFetch([
      [
        '/api/v1/repos?host=jeryu',
        {
          generated_at: '2026-06-05T00:00:00Z',
          total: 1,
          repositories: [repoSummary()],
          facets: { hosts: ['jeryu'], owners: ['alice'], families: [], languages: [] },
        },
      ],
      [
        '/api/v1/repos/repo-1/pulls',
        {
          total: 1,
          items: [pullSummary()],
        },
      ],
    ]);

    renderPage('/repos/jeryu/alice%2Fjeryu/pulls');

    await waitFor(() => {
      expect(screen.getByTestId('repo-pulls-page')).toBeInTheDocument();
      expect(screen.getByText('Fix Pull Room')).toBeInTheDocument();
    });
    expect(screen.queryByText(/W-FE-11/i)).not.toBeInTheDocument();
  });

  it('renders the required empty state', async () => {
    mockFetch([
      [
        '/api/v1/repos?host=jeryu',
        {
          generated_at: '2026-06-05T00:00:00Z',
          total: 1,
          repositories: [repoSummary()],
          facets: { hosts: ['jeryu'], owners: ['alice'], families: [], languages: [] },
        },
      ],
      ['/api/v1/repos/repo-1/pulls', { total: 0, items: [] }],
    ]);

    renderPage('/repos/jeryu/alice%2Fjeryu/pulls');

    expect(await screen.findByText('No pull requests')).toBeInTheDocument();
  });
});

function renderPage(route: string): void {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[route]}>
        <Routes>
          <Route
            path="/repos/:provider/:fullName/pulls"
            element={<RepositoryPullRequestsPage />}
          />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>
  );
}

function mockFetch(routes: Array<[string, unknown]>): void {
  const byPath = new Map(routes);
  vi.spyOn(globalThis, 'fetch').mockImplementation(async (input) => {
    const rawUrl = input instanceof Request ? input.url : String(input);
    const parsed = new URL(rawUrl, 'http://localhost');
    const path = parsed.pathname + parsed.search;
    const body = byPath.get(path);
    if (body === undefined) {
      return new Response(JSON.stringify({ error: { code: 'not_found', message: path } }), {
        status: 404,
        headers: { 'content-type': 'application/json' },
      });
    }
    return new Response(JSON.stringify(body), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  });
}

function repoSummary(): Record<string, unknown> {
  return {
    id: { id: 'repo-1', host: 'jeryu', owner: 'alice', name: 'jeryu' },
    entity: { kind: 'repository', id: 'repo-1' },
    description: null,
    visibility: 'private',
    default_branch: 'main',
    family: null,
    topics: [],
    language: null,
    health: 'healthy',
    open_pull_requests: 1,
    failing_checks: 0,
    running_jobs: 0,
    active_agents: 0,
    blocked_agents: 0,
    updated_at: '2026-06-05T00:00:00Z',
    clone_http_url: null,
    clone_ssh_url: null,
    available_actions: [],
  };
}

function pullSummary(): Record<string, unknown> {
  return {
    repo: { id: 'repo-1', host: 'jeryu', owner: 'alice', name: 'jeryu' },
    number: 12,
    entity: { kind: 'pull_request', id: 'repo-1#12' },
    title: 'Fix Pull Room',
    author: 'alice',
    head_ref: 'feature',
    base_ref: 'main',
    head_sha: 'abc123456789',
    base_sha: 'base123456789',
    state: 'open',
    draft: false,
    mergeable: {
      level: 'mergeable',
      can_merge: true,
      reason: null,
      exact_head_sha: 'abc123456789',
      required_gate: null,
    },
    review: {
      required_approvals: 1,
      approvals: 1,
      changes_requested: 0,
      unresolved_threads: 0,
      user_review_state: null,
    },
    checks: { total: 1, passing: 1, failing: 0, pending: 0, skipped: 0 },
    agents: {
      active_sessions: 0,
      proposed_patches: 0,
      evidence_packets: 0,
      blockers: 0,
    },
    labels: [],
    updated_at: '2026-06-05T00:00:00Z',
    passport_hash: 'passport-1',
    available_actions: [],
  };
}

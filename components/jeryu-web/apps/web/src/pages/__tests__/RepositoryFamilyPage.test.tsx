// RepositoryFamilyPage.test.tsx — render tests for the family drill-down.
//
// `useRepositories` is mocked so the test focuses on the page rendering:
// success (rollup strip + split browser rail), the empty state, the 403
// permission-denied state (negative authorization proof), and the error state
// with a retry action.

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, within } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiError } from '../../api/client';
import { RepositoryFamilyPage } from '../RepositoryFamilyPage';
import type { RepositorySummary } from '../../api/types';

const useRepositoriesMock = vi.fn();

vi.mock('../../hooks/useRepositories', () => ({
  useRepositories: (query: unknown) => useRepositoriesMock(query),
}));

vi.mock('../../hooks/useBlob', () => ({
  useBlob: () => ({
    isPending: false,
    error: null,
    data: null,
  }),
}));

vi.mock('../../components/browser', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../components/browser')>();
  return {
    ...actual,
    BranchSelector: ({ value }: { value: string }) => (
      <div aria-label="Branch selector">{value}</div>
    ),
    FileTree: () => <div aria-label="Files">file tree</div>,
    ReadmePanel: ({ repoId }: { repoId: string | null }) => (
      <article>README for {repoId}</article>
    ),
    CodeViewer: () => <article>code preview</article>,
  };
});

function repoSummary(
  name: string,
  overrides: Partial<RepositorySummary> = {}
): RepositorySummary {
  return {
    id: { id: `uuid-${name}`, host: 'jeryu', owner: 'veox', name },
    entity: { kind: 'repository', id: `uuid-${name}` },
    description: null,
    visibility: 'internal',
    default_branch: 'main',
    family: 'veox-split',
    repo_role: null,
    topics: [],
    language: null,
    health: 'healthy',
    open_pull_requests: 1,
    failing_checks: 0,
    running_jobs: 0,
    active_agents: 0,
    blocked_agents: 0,
    updated_at: '2026-06-09T08:30:00Z',
    clone_http_url: null,
    clone_ssh_url: null,
    available_actions: [],
    ...overrides,
  };
}

function listResult(repositories: RepositorySummary[]): unknown {
  return {
    data: {
      generated_at: '2026-06-10T00:00:00Z',
      total: repositories.length,
      repositories,
      facets: { hosts: [], owners: [], families: [], languages: [] },
    },
    isPending: false,
    error: null,
    refetch: vi.fn(),
  };
}

function renderPage(family = 'veox-split'): void {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter
        initialEntries={[`/repos/family/${encodeURIComponent(family)}`]}
      >
        <Routes>
          <Route
            path="/repos/family/:family"
            element={<RepositoryFamilyPage />}
          />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>
  );
}

describe('RepositoryFamilyPage', () => {
  beforeEach(() => {
    useRepositoriesMock.mockReset();
  });

  it('renders the rollup strip and split browser rail', () => {
    useRepositoriesMock.mockReturnValue(
      listResult([
        repoSummary('redline', {
          health: 'failing',
          open_pull_requests: 2,
          failing_checks: 3,
          running_jobs: 1,
        }),
        repoSummary('bluebird', {
          open_pull_requests: 5,
          failing_checks: 1,
          running_jobs: 2,
        }),
      ])
    );

    renderPage();

    expect(useRepositoriesMock).toHaveBeenCalledWith({
      family: 'veox-split',
      sort: 'name',
    });
    expect(
      screen.getByRole('heading', { level: 1, name: 'veox' })
    ).toBeInTheDocument();
    // Rollup strip: member count, worst-of health, sums (the summed values
    // differ from every per-card value so the labels are unambiguous).
    const strip = screen.getByLabelText('veox rollup');
    expect(within(strip).getByText('2 repos')).toBeInTheDocument();
    expect(
      within(strip).getByRole('status', { name: /Health: failing/ })
    ).toBeInTheDocument();
    expect(screen.getByLabelText('7 open pull requests')).toBeInTheDocument();
    expect(screen.getByLabelText('4 failing checks')).toBeInTheDocument();
    expect(screen.getByLabelText('3 running jobs')).toBeInTheDocument();
    const browser = screen.getByRole('region', {
      name: 'Split repository browser',
    });
    expect(browser).toHaveClass('split-browser');
    expect(screen.getByRole('button', { name: /redline/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /bluebird/ })).toBeInTheDocument();
    expect(screen.getByText('veox/bluebird')).toBeInTheDocument();
    expect(within(browser).getByText('file tree')).toBeInTheDocument();
    expect(screen.getByText('README for uuid-bluebird')).toBeInTheDocument();
  });

  it('renders the empty state with a back-to-repos action', () => {
    useRepositoriesMock.mockReturnValue(listResult([]));

    renderPage();

    expect(
      screen.getByText('No repositories in this family')
    ).toBeInTheDocument();
    expect(
      screen.getByRole('link', { name: /Back to repositories/ })
    ).toHaveAttribute('href', '/repos');
  });

  it('renders permission denied for a non-owner viewer (403 forbidden) without leaking repos', () => {
    // Negative authorization proof (owner/non-owner): a viewer without
    // repo.read on this family gets 403 forbidden from the list endpoint.
    useRepositoriesMock.mockReturnValue({
      data: undefined,
      isPending: false,
      error: new ApiError(403, {
        code: 'permission_denied',
        message: 'missing repo.read',
      }),
      refetch: vi.fn(),
    });

    renderPage();

    expect(screen.getByText('Permission denied')).toBeInTheDocument();
    expect(screen.getByText(/missing: repo\.read/)).toBeInTheDocument();
    // The non-owner viewer must see zero repository data in the DOM.
    expect(
      screen.queryByRole('region', { name: /repositories/ })
    ).not.toBeInTheDocument();
    expect(document.querySelector('.repo-card')).toBeNull();
  });

  it('renders the error state and retries via refetch', () => {
    const refetch = vi.fn();
    useRepositoriesMock.mockReturnValue({
      data: undefined,
      isPending: false,
      error: new ApiError(500, {
        code: 'internal',
        message: 'upstream exploded',
      }),
      refetch,
    });

    renderPage();

    expect(
      screen.getByText('Could not load family repositories')
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(refetch).toHaveBeenCalledTimes(1);
  });
});

// RepoDangerZone.test.tsx — danger-zone wiring against the removal endpoint.
//
// The zone owns `useDeleteRepository`, so these tests stub `fetch` and prove
// the wire contract end to end:
//   * `DELETE`-method call on /api/v1/repos/{id} with the slash percent-encoded,
//     `Idempotency-Key` header, and `{ confirm_full_name, delete_storage }`
//     JSON body;
//   * success parses the receipt and navigates to /repos;
//   * negative authorization (403 permission_denied for a viewer who may
//     not delete) surfaces the backend message in the dialog and does NOT
//     navigate — the destructive action provably did not happen;
//   * 422 confirm_mismatch (server-side byte-match guard) surfaces too.

import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { RepoDangerZone } from '../RepoDangerZone';
import type {
  DeleteRepositoryReceipt,
  RepositorySummary,
} from '../../../api/types';

const FULL_NAME = 'veox/redline';

function repoFixture(
  overrides: Partial<RepositorySummary> = {}
): RepositorySummary {
  return {
    // owner/name id form: the URL must percent-encode the slash.
    id: { id: FULL_NAME, host: 'jeryu', owner: 'veox', name: 'redline' },
    entity: { kind: 'repository', id: FULL_NAME },
    description: null,
    visibility: 'internal',
    default_branch: 'main',
    family: null,
    repo_role: null,
    topics: [],
    language: null,
    health: 'healthy',
    open_pull_requests: 0,
    failing_checks: 0,
    running_jobs: 0,
    active_agents: 0,
    blocked_agents: 0,
    updated_at: '2026-06-09T08:30:00Z',
    clone_http_url: null,
    clone_ssh_url: null,
    available_actions: [
      {
        action_id: 'repo.delete_registry',
        label: 'Remove from registry',
        risk: 'destructive',
      },
      {
        action_id: 'repo.delete_storage',
        label: 'Purge repository and storage',
        risk: 'destructive',
      },
    ],
    ...overrides,
  };
}

const RECEIPT: DeleteRepositoryReceipt = {
  repo: { id: FULL_NAME, host: 'jeryu', owner: 'veox', name: 'redline' },
  registry_deleted: true,
  deleted_counts: [{ collection: 'web_repositories', removed: 1 }],
  storage_deleted: false,
  storage_path: null,
  audit_id: 'audit-0001',
};

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function renderZone(repo: RepositorySummary): QueryClient {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter initialEntries={['/repos/jeryu/veox/redline']}>
        <Routes>
          <Route
            path="/repos/jeryu/veox/redline"
            element={<RepoDangerZone repo={repo} />}
          />
          <Route path="/repos" element={<div data-testid="repos-index" />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>
  );
  return queryClient;
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('RepoDangerZone', () => {
  it('renders both tiers using the available_actions labels', () => {
    vi.stubGlobal('fetch', vi.fn());
    renderZone(repoFixture());
    const zone = screen.getByTestId('repo-danger-zone');
    expect(zone).toHaveTextContent('Danger zone');
    expect(
      screen.getByRole('button', { name: 'Remove from registry' })
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Purge repository and storage' })
    ).toBeInTheDocument();
  });

  it('renders fallback labels when the backend advertises no removal actions', () => {
    vi.stubGlobal('fetch', vi.fn());
    renderZone(repoFixture({ available_actions: [] }));
    expect(
      screen.getByRole('button', { name: 'Remove from registry' })
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Purge repository and storage' })
    ).toBeInTheDocument();
  });

  it('registry tier posts the confirmation payload + Idempotency-Key and navigates on success', async () => {
    const user = userEvent.setup();
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, RECEIPT));
    vi.stubGlobal('fetch', fetchMock);
    const queryClient = renderZone(repoFixture());
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries');

    await user.click(
      screen.getByRole('button', { name: 'Remove from registry' })
    );
    // Registry tier: no typed confirmation required.
    await user.click(
      screen.getByRole('dialog').querySelector('.action-button--danger')!
    );

    await screen.findByTestId('repos-index');

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/v1/repos/veox%2Fredline');
    expect(init.method).toBe('DELETE');
    const headers = init.headers as Record<string, string>;
    expect(headers['Idempotency-Key']).toBeTruthy();
    expect(headers['Content-Type']).toBe('application/json');
    expect(JSON.parse(String(init.body))).toEqual({
      confirm_full_name: FULL_NAME,
      delete_storage: false,
    });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['repos'] });
  });

  it('purge tier requires the typed name and sends delete_storage: true', async () => {
    const user = userEvent.setup();
    const fetchMock = vi.fn().mockResolvedValue(
      jsonResponse(200, { ...RECEIPT, storage_deleted: true })
    );
    vi.stubGlobal('fetch', fetchMock);
    renderZone(repoFixture());

    await user.click(
      screen.getByRole('button', { name: 'Purge repository and storage' })
    );
    const dialog = screen.getByRole('dialog');
    const confirm = dialog.querySelector(
      '.action-button--danger'
    ) as HTMLButtonElement;
    expect(confirm).toBeDisabled();
    expect(fetchMock).not.toHaveBeenCalled();

    await user.type(screen.getByLabelText(/to confirm/), FULL_NAME);
    expect(confirm).toBeEnabled();
    await user.click(confirm);

    await screen.findByTestId('repos-index');
    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(JSON.parse(String(init.body))).toEqual({
      confirm_full_name: FULL_NAME,
      delete_storage: true,
    });
  });

  it('403 for a viewer without removal rights shows the error and does not navigate', async () => {
    const user = userEvent.setup();
    const fetchMock = vi.fn().mockResolvedValue(
      jsonResponse(403, {
        error: {
          code: 'permission_denied',
          message: 'You are not allowed to remove veox/redline.',
        },
      })
    );
    vi.stubGlobal('fetch', fetchMock);
    renderZone(repoFixture());

    await user.click(
      screen.getByRole('button', { name: 'Remove from registry' })
    );
    await user.click(
      screen.getByRole('dialog').querySelector('.action-button--danger')!
    );

    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent(
      'You are not allowed to remove veox/redline.'
    );
    // The destructive call was rejected: still on the overview route.
    expect(screen.queryByTestId('repos-index')).not.toBeInTheDocument();
    expect(screen.getByRole('dialog')).toBeInTheDocument();
  });

  it('422 confirm_mismatch from the server is surfaced in the dialog', async () => {
    const user = userEvent.setup();
    const fetchMock = vi.fn().mockResolvedValue(
      jsonResponse(422, {
        error: {
          code: 'confirm_mismatch',
          message: 'confirm_full_name does not match the repository.',
        },
      })
    );
    vi.stubGlobal('fetch', fetchMock);
    renderZone(repoFixture());

    await user.click(
      screen.getByRole('button', { name: 'Remove from registry' })
    );
    await user.click(
      screen.getByRole('dialog').querySelector('.action-button--danger')!
    );

    await waitFor(() => {
      expect(screen.getByRole('alert')).toHaveTextContent(
        'confirm_full_name does not match the repository.'
      );
    });
    expect(screen.queryByTestId('repos-index')).not.toBeInTheDocument();
  });
});

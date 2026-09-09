// ToolsPage.test.tsx — render smoke for the /tools control surface.
//
// Drives the page with mocked registry + dashboard + scan endpoints and
// asserts: the left rail ranks tools by total LOC saved, family cards render
// sorted with category chips, expanding a family exposes member clusters with
// exact occurrence spans, the Ignore action POSTs durable feedback to the
// cluster-feedback endpoint, and the Run-scan button POSTs the scan trigger.

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ToolsPage } from '../ToolsPage';
import type {
  ToolFinderCluster,
  ToolFinderDashboard,
  ToolFinderScanStatus,
  ToolRegistrySummary,
} from '../../api/types';

const REGISTRY: ToolRegistrySummary = {
  generated_at: '2026-06-12T00:00:00Z',
  tool_count: 2,
  published_count: 1,
  building_count: 0,
  proposed_count: 1,
  deprecated_count: 0,
  adopting_repo_count: 3,
  candidate_repo_count: 2,
  open_task_count: 1,
  realized_loc_saved: 900,
  anticipated_loc_saved: 450,
  tools: [
    {
      id: 'small-tool',
      name: 'small-tool',
      kind: 'shell-lib',
      status: 'published',
      adopting_repo_count: 1,
      candidate_repo_count: 0,
      loc_saved: 100,
      loc_saved_estimate: 0,
    },
    {
      id: 'big-tool',
      name: 'big-tool',
      kind: 'rust-crate',
      status: 'proposed',
      adopting_repo_count: 2,
      candidate_repo_count: 2,
      loc_saved: 800,
      loc_saved_estimate: 450,
    },
  ],
};

const CLUSTER: ToolFinderCluster = {
  cluster_id: 'toolbuild-abc123',
  category: 'tool-candidate',
  score: BigInt(900),
  occurrence_count: 2,
  repo_count: 2,
  file_count: 2,
  total_lines: 24,
  language: 'rust',
  insight: 'rust normalized window repeats 2 times across 2 file(s)',
  normalized_preview: 'kw:let id op:= call:retry',
  anticipated_loc_saved: 12,
  suggested_name: 'Shared rust helper (retry)',
  suggested_kind: 'rust-crate',
  ignored: false,
  occurrences: [
    {
      repo_id: 'repo-a',
      path: 'src/lib.rs',
      start_line: 10,
      end_line: 21,
      is_test: false,
    },
    {
      repo_id: 'repo-b',
      path: 'src/svc.rs',
      start_line: 4,
      end_line: 15,
      is_test: false,
    },
  ],
};

const DASHBOARD: ToolFinderDashboard = {
  generated_at: '2026-06-12T00:00:00Z',
  scan: { scanned_at: '1781222400000', repos_scanned: 57, files_scanned: 13133 },
  family_count: 2,
  cluster_count: 2,
  candidate_loc_saved: 612,
  families: [
    {
      family_id: 'toolfam-aaa',
      label: 'retry, call_remote',
      category: 'tool-candidate',
      language: 'rust',
      anticipated_loc_saved: 600,
      occurrence_count: 2,
      file_count: 2,
      repos: ['repo-a', 'repo-b'],
      clusters: [CLUSTER],
    },
    {
      family_id: 'toolfam-bbb',
      label: 'json, md, sh',
      category: 'managed-scaffold',
      language: 'shell',
      anticipated_loc_saved: 200,
      occurrence_count: 8,
      file_count: 8,
      repos: ['repo-a', 'repo-b', 'repo-c'],
      clusters: [],
    },
  ],
};

const IDLE_SCAN: ToolFinderScanStatus = {
  running: false,
  scan_id: 0,
  phase: 'idle',
  current_repo: null,
  repos_total: 0,
  repos_done: 0,
  files_scanned: 0,
  files_skipped: 0,
  clusters_found: 0,
  families_found: 0,
  started_at: null,
  finished_at: null,
  error: null,
};

function jsonResponse(payload: unknown): Response {
  return new Response(
    JSON.stringify(payload, (_key, value) =>
      typeof value === 'bigint' ? Number(value) : value
    ),
    { status: 200, headers: { 'content-type': 'application/json' } }
  );
}

describe('ToolsPage', () => {
  const requests: Array<{ method: string; pathname: string; body: string }> =
    [];

  beforeEach(() => {
    requests.length = 0;
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
      const rawUrl = input instanceof Request ? input.url : String(input);
      const { pathname } = new URL(rawUrl, 'http://localhost');
      const method =
        init?.method ?? (input instanceof Request ? input.method : 'GET');
      const body = typeof init?.body === 'string' ? init.body : '';
      requests.push({ method, pathname, body });
      if (pathname === '/api/v1/tools/registry/summary') {
        return jsonResponse(REGISTRY);
      }
      if (pathname === '/api/v1/tool-finder/dashboard') {
        return jsonResponse(DASHBOARD);
      }
      if (pathname === '/api/v1/tool-finder/scan' && method === 'GET') {
        return jsonResponse(IDLE_SCAN);
      }
      if (pathname === '/api/v1/tool-finder/scan' && method === 'POST') {
        return jsonResponse({ ...IDLE_SCAN, running: true, phase: 'discover' });
      }
      if (pathname.endsWith('/feedback') && method === 'POST') {
        return jsonResponse({ cluster_id: 'toolbuild-abc123' });
      }
      return new Response(
        JSON.stringify({ error: { code: 'not_found', message: pathname } }),
        { status: 404, headers: { 'content-type': 'application/json' } }
      );
    });
    vi.spyOn(window, 'prompt').mockReturnValue('fixture noise');
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  function renderPage(): void {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={['/tools']}>
          <ToolsPage />
        </MemoryRouter>
      </QueryClientProvider>
    );
  }

  it('ranks the rail by LOC saved and renders sorted family cards', async () => {
    renderPage();

    // Rail: big-tool (1250 total) above small-tool (100).
    await screen.findByTestId('rail-tool-big-tool');
    const rail = screen.getAllByTestId(/^rail-tool-/);
    expect(rail[0]).toHaveAttribute('data-testid', 'rail-tool-big-tool');
    expect(rail[1]).toHaveAttribute('data-testid', 'rail-tool-small-tool');

    // Families render with labels + category chips, candidates first.
    const families = screen.getAllByTestId(/^family-/);
    expect(families[0]).toHaveAttribute('data-testid', 'family-toolfam-aaa');
    expect(screen.getByText('retry, call_remote')).toBeInTheDocument();
    expect(screen.getByText('managed scaffold')).toBeInTheDocument();

    // Scan metadata header is painted from the dashboard.
    expect(screen.getByText(/2 clusters in 2 families/)).toBeInTheDocument();
  });

  it('expands a family into clusters and posts durable ignore feedback', async () => {
    renderPage();
    fireEvent.click(await screen.findByText('retry, call_remote'));

    // The member cluster paints exact occurrence spans.
    await screen.findByTestId('cluster-toolbuild-abc123');
    expect(screen.getByText('src/lib.rs:10-21')).toBeInTheDocument();
    expect(screen.getByText(/Shared rust helper \(retry\)/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /Ignore/ }));
    await waitFor(() => {
      const feedback = requests.find((r) => r.pathname.endsWith('/feedback'));
      expect(feedback).toBeTruthy();
      expect(feedback?.method).toBe('POST');
      expect(feedback?.body).toContain('fixture noise');
      expect(feedback?.pathname).toContain('toolbuild-abc123');
    });
  });

  it('starts a live scan from the header button', async () => {
    renderPage();
    fireEvent.click(await screen.findByTestId('run-scan-button'));
    await waitFor(() => {
      const post = requests.find(
        (r) => r.pathname === '/api/v1/tool-finder/scan' && r.method === 'POST'
      );
      expect(post).toBeTruthy();
    });
  });
});

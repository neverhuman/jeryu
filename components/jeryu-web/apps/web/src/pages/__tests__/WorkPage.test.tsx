import userEvent from '@testing-library/user-event';
import { screen, waitFor, within } from '@testing-library/react';
import { Route } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { RepoRouter } from '../RepoRouter';
import { WorkDetailPage } from '../WorkDetailPage';
import { WorkPage } from '../WorkPage';
import { mockFetch, renderRoute, repoSummary, workItem } from './workPageTestHelpers';

describe('WorkPage', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders split-wide work items with provider-shaped issue and PR links', async () => {
    mockFetch([
      [
        '/api/v1/work',
        {
          total: 1,
          items: [
            workItem({
              title: 'Fix cache key',
              labels: ['cache'],
              issue: {
                owner: 'alice',
                repo: 'jeryu',
                number: 42,
                url: '/repos/jeryu/alice/jeryu/issues#42',
              },
              pull_requests: [
                {
                  owner: 'alice',
                  repo: 'jeryu',
                  number: 7,
                  url: '/repos/jeryu/alice/jeryu/pulls/7',
                },
              ],
            }),
          ],
        },
      ],
    ]);

    renderRoute('/work', <Route path="/work" element={<WorkPage />} />);

    expect(await screen.findByText('Fix cache key')).toBeInTheDocument();
    expect(screen.getByText('JRY-1')).toBeInTheDocument();
    expect(screen.getAllByText('cache').length).toBeGreaterThan(0);
    expect(screen.getByRole('link', { name: '#42' })).toHaveAttribute(
      'href',
      '/repos/jeryu/alice/jeryu/issues#42'
    );
    expect(screen.getByRole('link', { name: 'PR 7' })).toHaveAttribute(
      'href',
      '/repos/jeryu/alice/jeryu/pulls/7'
    );
  });

  it('submits split-wide create payloads without BigInt serialization', async () => {
    const requests = mockFetch([
      ['/api/v1/work', { total: 0, items: [] }],
      {
        method: 'POST',
        path: '/api/v1/work',
        response: workItem({
          title: 'Investigate runner retry',
          kind: 'bug',
          priority: 'p1',
          status: 'blocked',
        }),
      },
    ]);
    const user = userEvent.setup();

    renderRoute('/work', <Route path="/work" element={<WorkPage />} />);

    const createRegion = await screen.findByRole('region', {
      name: 'Create work item',
    });
    await user.type(
      within(createRegion).getByLabelText('Title'),
      'Investigate runner retry'
    );
    await user.selectOptions(within(createRegion).getByLabelText('Kind'), 'bug');
    await user.selectOptions(
      within(createRegion).getByLabelText('Priority'),
      'p1'
    );
    await user.selectOptions(
      within(createRegion).getByLabelText('Status'),
      'blocked'
    );
    await user.type(
      within(createRegion).getByLabelText('Body'),
      'Breaks on retry'
    );
    await user.type(
      within(createRegion).getByLabelText('Labels'),
      'ci, runner'
    );
    await user.type(
      within(createRegion).getByLabelText('Assignees'),
      'alice, agent:runner'
    );
    await user.click(within(createRegion).getByRole('button', { name: 'Create' }));

    await waitFor(() => {
      expect(requests.some((request) => request.method === 'POST')).toBe(true);
    });
    expect(requests.find((request) => request.method === 'POST')?.body).toEqual({
      repo: null,
      title: 'Investigate runner retry',
      body: 'Breaks on retry',
      status: 'blocked',
      kind: 'bug',
      priority: 'p1',
      labels: ['ci', 'runner'],
      assignees: [
        { kind: 'human', id: 'alice', display_name: null },
        { kind: 'agent', id: 'runner', display_name: null },
      ],
    });
  });

  it('submits repo-scoped create payloads to the repo Work endpoint', async () => {
    const requests = mockFetch([
      [
        '/api/v1/repos?host=jeryu',
        {
          generated_at: '2026-07-02T00:00:00Z',
          total: 1,
          repositories: [repoSummary()],
          facets: { hosts: ['jeryu'], owners: ['alice'], families: [], languages: [] },
        },
      ],
      ['/api/v1/repos/repo-1/work', { total: 0, items: [] }],
      {
        method: 'POST',
        path: '/api/v1/repos/repo-1/work',
        response: workItem({ title: 'Repo local task' }),
      },
    ]);
    const user = userEvent.setup();

    renderRoute(
      '/repos/jeryu/alice/jeryu/work',
      <Route
        path="/repos/:provider/:owner/:name/work"
        element={<WorkPage provider="jeryu" fullName="alice/jeryu" />}
      />
    );

    const createRegion = await screen.findByRole('region', {
      name: 'Create work item',
    });
    expect(screen.queryByLabelText('Repo')).not.toBeInTheDocument();
    await user.type(within(createRegion).getByLabelText('Title'), 'Repo local task');
    await user.click(within(createRegion).getByRole('button', { name: 'Create' }));

    await waitFor(() => {
      expect(
        requests.some(
          (request) =>
            request.method === 'POST' &&
            request.path === '/api/v1/repos/repo-1/work'
        )
      ).toBe(true);
    });
    expect(requests.find((request) => request.method === 'POST')?.body).toMatchObject({
      repo: null,
      title: 'Repo local task',
      status: 'ready',
      kind: 'task',
      priority: 'p2',
    });
  });

  it('routes repo /issues through the Work issue-compatible alias', async () => {
    mockFetch([
      [
        '/api/v1/repos?host=jeryu',
        {
          generated_at: '2026-07-02T00:00:00Z',
          total: 1,
          repositories: [repoSummary()],
          facets: { hosts: ['jeryu'], owners: ['alice'], families: [], languages: [] },
        },
      ],
      [
        '/api/v1/repos/repo-1/work',
        {
          total: 1,
          items: [
            workItem({
              title: 'Triage linked issue',
              issue: {
                owner: 'alice',
                repo: 'jeryu',
                number: 42,
                url: '/repos/jeryu/alice/jeryu/issues#42',
              },
            }),
          ],
        },
      ],
    ]);

    renderRoute(
      '/repos/jeryu/alice/jeryu/issues',
      <Route path="/repos/:provider/*" element={<RepoRouter />} />
    );

    await waitFor(() => {
      expect(screen.getByTestId('work-page')).toBeInTheDocument();
      expect(screen.getByText('Triage linked issue')).toBeInTheDocument();
    });
    expect(screen.getByText('alice/jeryu issue-compatible work items.')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '#42' })).toHaveAttribute(
      'href',
      '/repos/jeryu/alice/jeryu/issues#42'
    );
  });

  it('navigates from repo-scoped Work cards to the Work detail route', async () => {
    const user = userEvent.setup();
    mockFetch([
      [
        '/api/v1/repos?host=jeryu',
        {
          generated_at: '2026-07-02T00:00:00Z',
          total: 1,
          repositories: [repoSummary()],
          facets: { hosts: ['jeryu'], owners: ['alice'], families: [], languages: [] },
        },
      ],
      [
        '/api/v1/repos/repo-1/work',
        {
          total: 1,
          items: [workItem({ title: 'Repo work detail' })],
        },
      ],
      [
        '/api/v1/work/JRY-1',
        {
          item: workItem({ title: 'Linked from repo board' }),
          comments: [],
        },
      ],
    ]);

    renderRoute(
      '/repos/jeryu/alice/jeryu/work',
      <>
        <Route path="/repos/:provider/*" element={<RepoRouter />} />
        <Route path="/work/:key" element={<WorkDetailPage />} />
      </>
    );

    await user.click(await screen.findByRole('link', { name: 'Repo work detail' }));

    expect(await screen.findByTestId('work-detail-page')).toBeInTheDocument();
    expect(screen.getByText('Linked from repo board')).toBeInTheDocument();
  });
});

import userEvent from '@testing-library/user-event';
import { screen, waitFor } from '@testing-library/react';
import { Route } from 'react-router-dom';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { WorkDetailPage } from '../WorkDetailPage';
import { mockFetch, renderRoute, workItem } from './workPageTestHelpers';

describe('WorkDetailPage', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders detail state and submits updates, comments, and PR links', async () => {
    const requests = mockFetch([
      [
        '/api/v1/work/JRY-1',
        {
          item: workItem({
            title: 'Detail item',
            body: 'Initial body',
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
          comments: [
            {
              id: 'comment-1',
              work_key: 'JRY-1',
              author: { kind: 'human', id: 'alice', display_name: null },
              body: 'Existing note',
              created_at: '2026-07-02T00:30:00Z',
            },
          ],
        },
      ],
      {
        method: 'PATCH',
        path: '/api/v1/work/JRY-1',
        response: workItem({ title: 'Detail item updated' }),
      },
      {
        method: 'POST',
        path: '/api/v1/work/JRY-1/comments',
        response: {
          id: 'comment-2',
          work_key: 'JRY-1',
          author: { kind: 'human', id: 'alice', display_name: null },
          body: 'Looks ready',
          created_at: '2026-07-02T01:00:00Z',
        },
      },
      {
        method: 'POST',
        path: '/api/v1/work/JRY-1/links',
        response: workItem({
          title: 'Detail item',
          pull_requests: [
            {
              owner: 'alice',
              repo: 'jeryu',
              number: 18,
              url: '/repos/jeryu/alice/jeryu/pulls/18',
            },
          ],
        }),
      },
    ]);
    const user = userEvent.setup();

    renderRoute('/work/JRY-1', <Route path="/work/:key" element={<WorkDetailPage />} />);

    expect(await screen.findByText('Detail item')).toBeInTheDocument();
    expect(screen.getByRole('link', { name: '#42' })).toHaveAttribute(
      'href',
      '/repos/jeryu/alice/jeryu/issues#42'
    );
    expect(screen.getByRole('link', { name: 'alice/jeryu#7' })).toHaveAttribute(
      'href',
      '/repos/jeryu/alice/jeryu/pulls/7'
    );
    expect(screen.getByText('Existing note')).toBeInTheDocument();

    await user.clear(screen.getByLabelText('Title'));
    await user.type(screen.getByLabelText('Title'), 'Detail item updated');
    await user.selectOptions(screen.getByLabelText('Status'), 'blocked');
    await user.selectOptions(screen.getByLabelText('Kind'), 'bug');
    await user.selectOptions(screen.getByLabelText('Priority'), 'p0');
    await user.clear(screen.getByLabelText('Labels'));
    await user.type(screen.getByLabelText('Labels'), 'release, urgent');
    await user.clear(screen.getByLabelText('Assignees'));
    await user.type(screen.getByLabelText('Assignees'), 'alice, agent:runner');
    await user.clear(screen.getByLabelText('Body'));
    await user.type(screen.getByLabelText('Body'), 'New body');
    await user.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      expect(
        requests.some((request) => request.method === 'PATCH')
      ).toBe(true);
    });
    expect(requests.find((request) => request.method === 'PATCH')?.body).toEqual({
      title: 'Detail item updated',
      body: 'New body',
      status: 'blocked',
      kind: 'bug',
      priority: 'p0',
      labels: ['release', 'urgent'],
      assignees: [
        { kind: 'human', id: 'alice', display_name: null },
        { kind: 'agent', id: 'runner', display_name: null },
      ],
    });

    await user.type(screen.getByLabelText('Comment body'), 'Looks ready');
    await user.click(screen.getByRole('button', { name: 'Comment' }));

    await waitFor(() => {
      expect(
        requests.some(
          (request) =>
            request.method === 'POST' &&
            request.path === '/api/v1/work/JRY-1/comments'
        )
      ).toBe(true);
    });
    expect(
      requests.find((request) => request.path === '/api/v1/work/JRY-1/comments')
        ?.body
    ).toEqual({ body: 'Looks ready', author: null });

    await user.type(screen.getByLabelText('Pull request owner'), 'alice');
    await user.type(screen.getByLabelText('Pull request repo'), 'jeryu');
    await user.type(screen.getByLabelText('Pull request number'), '18');
    await user.click(screen.getByRole('button', { name: 'Link' }));

    await waitFor(() => {
      expect(
        requests.some(
          (request) =>
            request.method === 'POST' &&
            request.path === '/api/v1/work/JRY-1/links'
        )
      ).toBe(true);
    });
    const linkBody = requests.find(
      (request) => request.path === '/api/v1/work/JRY-1/links'
    )?.body as Record<string, Record<string, unknown> | null> | undefined;
    expect(linkBody).toEqual({
      issue: null,
      pull_request: {
        owner: 'alice',
        repo: 'jeryu',
        number: 18,
        url: '/repos/jeryu/alice/jeryu/pulls/18',
      },
    });
    expect(typeof linkBody?.pull_request?.number).toBe('number');
  });
});

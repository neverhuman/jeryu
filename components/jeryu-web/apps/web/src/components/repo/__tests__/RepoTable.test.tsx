import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it } from 'vitest';

import type { RepositorySummary } from '../../../api/types';
import { RepoTable } from '../RepoTable';

const REPO: RepositorySummary = {
  id: {
    id: 'repo-uuid-1',
    host: 'jeryu',
    owner: 'neverhuman',
    name: 'jeryu-core',
  },
  entity: { kind: 'repository', id: 'repo-uuid-1' },
  description: 'Core split',
  visibility: 'public',
  default_branch: 'main',
  family: 'jeryu-split',
  repo_role: 'split_member',
  topics: [],
  language: 'Rust',
  health: 'healthy',
  open_pull_requests: 0,
  failing_checks: 0,
  running_jobs: 0,
  active_agents: 0,
  blocked_agents: 0,
  updated_at: '2026-05-26T12:00:00Z',
  clone_http_url: null,
  clone_ssh_url: null,
  available_actions: [],
};

describe('RepoTable', () => {
  it('renders split-member role badges in rows', () => {
    render(
      <MemoryRouter>
        <RepoTable repos={[REPO]} />
      </MemoryRouter>
    );

    expect(screen.getByText('Split member')).toBeInTheDocument();
  });
});

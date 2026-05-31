// RepoCard.test.tsx — minimal render smoke for W-FE-08.
//
// The card must surface owner/name, description, default branch, language
// and the health pill. We do not assert on relative time so the test is
// independent of the wall clock.

import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it } from 'vitest';

import { RepoCard } from '../RepoCard';
import type { RepositorySummary } from '../../../api/types';

const FIXTURE: RepositorySummary = {
  id: {
    id: 'repo-uuid-1',
    host: 'jeryu',
    owner: 'veox',
    name: 'redline',
  },
  entity: { kind: 'repository', id: 'repo-uuid-1' },
  description: 'Edge router for VEOX',
  visibility: 'internal',
  default_branch: 'main',
  family: 'veox-*',
  topics: ['rust', 'async'],
  language: 'Rust',
  health: 'healthy',
  open_pull_requests: 3,
  failing_checks: 0,
  running_jobs: 0,
  active_agents: 1,
  blocked_agents: 0,
  updated_at: '2026-05-26T12:00:00Z',
  clone_http_url: 'https://jeryu.example/veox/redline.git',
  clone_ssh_url: 'git@jeryu.example:veox/redline.git',
  available_actions: [],
};

describe('RepoCard', () => {
  it('renders owner, name, description, language, and health', () => {
    render(
      <MemoryRouter>
        <RepoCard repo={FIXTURE} />
      </MemoryRouter>
    );
    expect(screen.getByText('redline')).toBeInTheDocument();
    expect(screen.getByText('veox/')).toBeInTheDocument();
    expect(screen.getByText('Edge router for VEOX')).toBeInTheDocument();
    expect(screen.getByText('main')).toBeInTheDocument();
    expect(screen.getByText('Rust')).toBeInTheDocument();
    expect(
      screen.getByRole('link', { name: /Open repository veox\/redline/ })
    ).toHaveAttribute('href', '/repos/jeryu/veox/redline');
    expect(screen.getByRole('status', { name: /Health/ })).toBeInTheDocument();
  });
});

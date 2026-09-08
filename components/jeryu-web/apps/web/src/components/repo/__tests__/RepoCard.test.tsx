// RepoCard.test.tsx — minimal render smoke for W-FE-08.
//
// The card must surface the repo name (without an owner prefix),
// description, default branch, language and the health pill. We do not
// assert on relative time so the test is independent of the wall clock.

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
  repo_role: null,
  topics: ['rust', 'async'],
  language: 'Rust',
  health: 'healthy',
  open_pull_requests: 3,
  failing_checks: 0,
  running_jobs: 0,
  active_agents: 1,
  blocked_agents: 0,
  updated_at: '2026-05-26T12:00:00Z',
  jankurai_score: 92,
  jankurai_decision: 'pass',
  jankurai_scored_at: '2026-05-26T11:00:00Z',
  mirror: {
    configured: true,
    last_attempt_at: '2026-05-26T11:30:00Z',
    last_attempt_ok: true,
    last_attempt_conclusion: 'success',
    last_success_at: '2026-05-26T11:30:00Z',
  },
  clone_http_url: 'https://jeryu.example/veox/redline.git',
  clone_ssh_url: 'git@jeryu.example:veox/redline.git',
  available_actions: [],
};

describe('RepoCard', () => {
  it('renders name, description, language, and health without owner prefix', () => {
    render(
      <MemoryRouter>
        <RepoCard repo={FIXTURE} />
      </MemoryRouter>
    );
    expect(screen.getByText('redline')).toBeInTheDocument();
    expect(screen.queryByText('veox/')).not.toBeInTheDocument();
    expect(screen.queryByText(/veox\/redline/)).not.toBeInTheDocument();
    expect(screen.getByText('Edge router for VEOX')).toBeInTheDocument();
    expect(screen.getByText('main')).toBeInTheDocument();
    expect(screen.getByText('Rust')).toBeInTheDocument();
    expect(
      screen.getByRole('link', { name: 'Open repository redline' })
    ).toHaveAttribute('href', '/repos/jeryu/veox/redline');
    expect(screen.getByRole('status', { name: /Health/ })).toBeInTheDocument();
  });

  it('renders the public portal badge', () => {
    render(
      <MemoryRouter>
        <RepoCard
          repo={{
            ...FIXTURE,
            id: { ...FIXTURE.id, owner: 'neverhuman', name: 'jeryu' },
            family: 'jeryu-split',
            repo_role: 'public_portal',
          }}
        />
      </MemoryRouter>
    );
    expect(screen.getByText('Public portal')).toBeInTheDocument();
  });

  it('renders the jankurai score pill and the mirror posture badge', () => {
    render(
      <MemoryRouter>
        <RepoCard repo={FIXTURE} />
      </MemoryRouter>
    );
    expect(
      screen.getByRole('status', { name: /jankurai score 92/ })
    ).toHaveClass('repo-score-badge--good');
    expect(
      screen.getByRole('status', { name: /Mirror pushed/ })
    ).toBeInTheDocument();
  });

  it('shows the running-jobs indicator only when jobs are running', () => {
    const { rerender } = render(
      <MemoryRouter>
        <RepoCard repo={FIXTURE} />
      </MemoryRouter>
    );
    expect(screen.queryByLabelText(/running jobs/)).not.toBeInTheDocument();
    rerender(
      <MemoryRouter>
        <RepoCard repo={{ ...FIXTURE, running_jobs: 2 }} />
      </MemoryRouter>
    );
    expect(screen.getByLabelText('2 running jobs')).toBeInTheDocument();
  });

  it('omits the mirror badge when no mirror is configured', () => {
    render(
      <MemoryRouter>
        <RepoCard repo={{ ...FIXTURE, mirror: null }} />
      </MemoryRouter>
    );
    expect(
      screen.queryByRole('status', { name: /Mirror/ })
    ).not.toBeInTheDocument();
  });
});

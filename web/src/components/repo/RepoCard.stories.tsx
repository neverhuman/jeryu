// RepoCard.stories.tsx — health-state matrix (W-T-07).
//
// Covers the five archetypes the plan calls out: healthy / warning /
// critical / archived / private. Each story passes a `RepositorySummary`
// fixture that toggles the relevant facets so the addon-a11y panel can
// scan the chrome in every shape.

import type { Meta, StoryObj } from '@storybook/react-vite';

import type { RepositorySummary } from '../../api/types';

import { RepoCard } from './RepoCard';

function baseRepo(overrides: Partial<RepositorySummary> = {}): RepositorySummary {
  return {
    id: { id: 'repo-uuid', host: 'jeryu', owner: 'veox', name: 'redline' },
    entity: { kind: 'repository', id: 'repo-uuid' },
    description: 'Edge router for VEOX.',
    visibility: 'internal',
    default_branch: 'main',
    family: 'veox-*',
    topics: ['rust', 'async'],
    language: 'Rust',
    health: 'healthy',
    open_pull_requests: 3,
    failing_checks: 0,
    running_jobs: 1,
    active_agents: 1,
    blocked_agents: 0,
    updated_at: '2026-05-26T12:00:00Z',
    clone_http_url: 'https://jeryu.example/veox/redline.git',
    clone_ssh_url: 'git@jeryu.example:veox/redline.git',
    available_actions: [],
    ...overrides,
  };
}

const meta: Meta<typeof RepoCard> = {
  title: 'repo/RepoCard',
  component: RepoCard,
};
export default meta;

type Story = StoryObj<typeof RepoCard>;

export const Healthy: Story = {
  args: {
    repo: baseRepo({ health: 'healthy' }),
  },
};

export const Warning: Story = {
  args: {
    repo: baseRepo({
      health: 'degraded',
      open_pull_requests: 12,
      failing_checks: 1,
    }),
  },
};

export const Critical: Story = {
  args: {
    repo: baseRepo({
      health: 'failing',
      failing_checks: 7,
      blocked_agents: 2,
      description:
        'Outage triage — three production rollbacks queued, agents blocked on policy.',
    }),
  },
};

export const Archived: Story = {
  args: {
    repo: baseRepo({
      health: 'archived',
      open_pull_requests: 0,
      failing_checks: 0,
      active_agents: 0,
      description: 'Archived in 2025. Read-only mirror.',
    }),
  },
};

export const Private: Story = {
  args: {
    repo: baseRepo({
      visibility: 'private',
      description: 'Private vault. Access requires repo.read on this slug.',
      topics: ['private', 'security'],
    }),
  },
};

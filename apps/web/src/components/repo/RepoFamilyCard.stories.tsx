// RepoFamilyCard.stories.tsx — rollup-state matrix for the family tile.
//
// Covers the worst-of health ramp (healthy / degraded / failing) plus a
// single-member family so the count pill's singular form is reviewable.
// Each story passes a `FamilyRollup` fixture; the addon-a11y panel scans
// the tile chrome in every shape.

import type { Meta, StoryObj } from '@storybook/react-vite';

import type { FamilyRollup } from './familyRollup';

import { RepoFamilyCard } from './RepoFamilyCard';

function baseRollup(overrides: Partial<FamilyRollup> = {}): FamilyRollup {
  return {
    name: 'veox-split',
    repos: [],
    memberCount: 4,
    health: 'healthy',
    openPullRequests: 6,
    failingChecks: 0,
    runningJobs: 2,
    activeAgents: 3,
    updatedAt: '2026-05-26T12:00:00Z',
    worstScore: 91,
    ...overrides,
  };
}

const meta: Meta<typeof RepoFamilyCard> = {
  title: 'repo/RepoFamilyCard',
  component: RepoFamilyCard,
};
export default meta;

type Story = StoryObj<typeof RepoFamilyCard>;

export const Healthy: Story = {
  args: {
    family: baseRollup(),
  },
};

export const Degraded: Story = {
  args: {
    family: baseRollup({
      health: 'degraded',
      openPullRequests: 12,
      failingChecks: 2,
    }),
  },
};

export const Failing: Story = {
  args: {
    family: baseRollup({
      name: 'jmcp-split',
      health: 'failing',
      failingChecks: 7,
      runningJobs: 0,
      worstScore: 58,
    }),
  },
};

export const NoMemberScores: Story = {
  args: {
    family: baseRollup({ worstScore: null }),
  },
};

export const SingleMember: Story = {
  args: {
    family: baseRollup({
      memberCount: 1,
      openPullRequests: 1,
      runningJobs: 0,
    }),
  },
};

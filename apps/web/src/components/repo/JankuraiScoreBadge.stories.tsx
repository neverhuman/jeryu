// JankuraiScoreBadge.stories.tsx — audit-score pill state matrix.
//
// Covers the four states: good (>= 85), warn (< 85), "audit failed"
// (no score but a decision, e.g. tool-failed), and neutral "no score"
// (no audit ingested yet). The addon-a11y panel scans each colour ramp.

import type { Meta, StoryObj } from '@storybook/react-vite';

import { JankuraiScoreBadge } from './JankuraiScoreBadge';

const meta: Meta<typeof JankuraiScoreBadge> = {
  title: 'repo/JankuraiScoreBadge',
  component: JankuraiScoreBadge,
};
export default meta;

type Story = StoryObj<typeof JankuraiScoreBadge>;

export const Good: Story = {
  args: {
    score: 92,
    decision: 'pass',
    scoredAt: '2026-06-09T08:30:00Z',
  },
};

export const Warn: Story = {
  args: {
    score: 64,
    decision: 'fail',
    scoredAt: '2026-06-09T08:30:00Z',
  },
};

export const ExactlyAtThreshold: Story = {
  args: {
    score: 85,
    decision: 'pass',
    scoredAt: '2026-06-09T08:30:00Z',
  },
};

export const AuditFailed: Story = {
  args: {
    score: null,
    decision: 'tool-failed',
    scoredAt: '2026-06-09T08:30:00Z',
  },
};

export const NoScore: Story = {
  args: {
    score: null,
    decision: null,
    scoredAt: null,
  },
};

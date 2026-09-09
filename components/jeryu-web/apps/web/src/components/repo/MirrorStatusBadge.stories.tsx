// MirrorStatusBadge.stories.tsx — offsite mirror posture state matrix.
//
// Healthy push, configured-but-never-pushed, failed push (danger ramp),
// and the not-configured shape (renders nothing — story shows a fallback
// caption so the a11y scan has content to anchor on).

import type { Meta, StoryObj } from '@storybook/react-vite';

import { MirrorStatusBadge } from './MirrorStatusBadge';

const meta: Meta<typeof MirrorStatusBadge> = {
  title: 'repo/MirrorStatusBadge',
  component: MirrorStatusBadge,
};
export default meta;

type Story = StoryObj<typeof MirrorStatusBadge>;

export const Healthy: Story = {
  args: {
    mirror: {
      configured: true,
      last_attempt_at: '2026-06-09T09:00:00Z',
      last_attempt_ok: true,
      last_attempt_conclusion: 'success',
      last_success_at: '2026-06-09T09:00:00Z',
    },
  },
};

export const NeverPushed: Story = {
  args: {
    mirror: {
      configured: true,
      last_attempt_at: null,
      last_attempt_ok: true,
      last_attempt_conclusion: null,
      last_success_at: null,
    },
  },
};

export const FailedPush: Story = {
  args: {
    mirror: {
      configured: true,
      last_attempt_at: '2026-06-09T11:00:00Z',
      last_attempt_ok: false,
      last_attempt_conclusion: 'failure',
      last_success_at: '2026-06-07T10:00:00Z',
    },
  },
};

export const NotConfigured: Story = {
  args: {
    mirror: null,
  },
  render: (args) => (
    <>
      <MirrorStatusBadge {...args} />
      <p className="text-muted">
        (no badge — repository has no offsite mirror)
      </p>
    </>
  ),
};

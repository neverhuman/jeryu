// MergeGatePanel.stories.tsx — Merge Passport verdict surface (W-T-07).
//
// Covers the five archetypes the plan calls out: pass / blocked /
// drifted-SHA / approval-required / agent-evidence.

import type { Meta, StoryObj } from '@storybook/react-vite';

import type { MergePassport } from '../../api/types';

import { MergeGatePanel } from './MergeGatePanel';

const meta: Meta<typeof MergeGatePanel> = {
  title: 'merge/MergeGatePanel',
  component: MergeGatePanel,
};
export default meta;

type Story = StoryObj<typeof MergeGatePanel>;

const HEAD_SHA = 'abcdef1234567890abcdef1234567890abcdef12';

function passport(overrides: Partial<MergePassport> = {}): MergePassport {
  return {
    status: 'pass',
    head_sha: HEAD_SHA,
    blockers: [],
    evaluated_at: '2026-05-26T12:00:00Z',
    ...overrides,
  };
}

export const Pass: Story = {
  args: {
    passport: passport(),
  },
};

export const Blocked: Story = {
  args: {
    passport: passport({
      status: 'blocked',
      blockers: [
        {
          code: 'passport_blocked_checks',
          message: 'Required checks failing.',
          details: '3 of 8 checks failing.',
        },
        {
          code: 'passport_blocked_threads',
          message: 'Unresolved threads.',
          details: '2 threads pending resolution.',
        },
      ],
    }),
  },
};

export const DriftedSha: Story = {
  name: 'Drifted SHA',
  args: {
    passport: passport({
      status: 'blocked',
      blockers: [
        {
          code: 'passport_blocked_passport_sha',
          message: 'Passport SHA drift.',
          details: 'Head moved to ab12cd34; refresh required.',
        },
      ],
    }),
  },
};

export const ApprovalRequired: Story = {
  name: 'Approval required',
  args: {
    passport: passport({
      status: 'blocked',
      blockers: [
        {
          code: 'passport_blocked_approvals',
          message: 'Required approver count not satisfied.',
          details: '1 of 2 approvals on this head.',
        },
        {
          code: 'passport_blocked_codeowners',
          message: 'CODEOWNERS approval required.',
          details: '@team-security has not approved touched files.',
        },
      ],
    }),
  },
};

export const AgentEvidence: Story = {
  name: 'Agent evidence required',
  args: {
    passport: passport({
      status: 'blocked',
      blockers: [
        {
          code: 'passport_blocked_agent_evidence',
          message: 'Agent evidence packet required.',
          details:
            'Patches authored by `agent:rustfmt-bot` must include a signed evidence pack.',
        },
      ],
    }),
  },
};

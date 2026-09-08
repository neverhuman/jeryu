// RiskBadge.stories.tsx — risk-tier states (W-T-07).
//
// All four canonical risk tiers — low / medium / high / critical —
// rendered so the addon-a11y panel can verify each colour ramp meets
// contrast and the badge advertises its tier to screen readers.

import type { Meta, StoryObj } from '@storybook/react-vite';

import { RiskBadge } from './RiskBadge';

const meta: Meta<typeof RiskBadge> = {
  title: 'action/RiskBadge',
  component: RiskBadge,
};
export default meta;

type Story = StoryObj<typeof RiskBadge>;

export const Low: Story = { args: { tier: 'low' } };
export const Medium: Story = { args: { tier: 'medium' } };
export const High: Story = { args: { tier: 'high' } };
export const Critical: Story = { args: { tier: 'critical' } };

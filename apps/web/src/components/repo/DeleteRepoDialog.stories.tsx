// DeleteRepoDialog.stories.tsx — two-tier removal confirmation states.
//
// Registry tier (high risk, storage kept), purge tier (critical risk,
// typed-name confirmation gates the confirm button), busy and error
// shapes. The dialog renders into a fixed-position backdrop; Storybook's
// padded layout keeps the panel scannable for the a11y addon.

import type { Meta, StoryObj } from '@storybook/react-vite';

import { DeleteRepoDialog } from './DeleteRepoDialog';

const meta: Meta<typeof DeleteRepoDialog> = {
  title: 'repo/DeleteRepoDialog',
  component: DeleteRepoDialog,
  args: {
    open: true,
    fullName: 'veox/redline',
    onConfirm: () => {},
    onCancel: () => {},
  },
};
export default meta;

type Story = StoryObj<typeof DeleteRepoDialog>;

export const RegistryTier: Story = {
  args: {
    tier: 'registry',
    confirmLabel: 'Remove from registry',
  },
};

export const PurgeTier: Story = {
  args: {
    tier: 'purge',
    confirmLabel: 'Purge repository and storage',
  },
};

export const PurgeBusy: Story = {
  args: {
    tier: 'purge',
    busy: true,
  },
};

export const WithError: Story = {
  args: {
    tier: 'registry',
    errorMessage:
      'confirm_full_name does not match the repository (expected veox/redline).',
  },
};

// SettingsDiffPreview.stories.tsx — settings preview surface (W-T-07).
//
// Covers the four archetypes the plan calls out: safe / reversible /
// irreversible / production-impact.

import type { Meta, StoryObj } from '@storybook/react-vite';

import type {
  RepositoryId,
  SettingsDiffPreview as SettingsDiffPreviewWire,
} from '../../api/types';

import { SettingsDiffPreview } from './SettingsDiffPreview';

const meta: Meta<typeof SettingsDiffPreview> = {
  title: 'settings/SettingsDiffPreview',
  component: SettingsDiffPreview,
};
export default meta;

type Story = StoryObj<typeof SettingsDiffPreview>;

const REPO: RepositoryId = {
  id: 'repo-uuid',
  host: 'jeryu',
  owner: 'veox',
  name: 'redline',
};

function preview(
  overrides: Partial<SettingsDiffPreviewWire> = {}
): SettingsDiffPreviewWire {
  return {
    repo: REPO,
    current_hash: 'sha256:1111',
    diffs: [],
    side_effects: [],
    warnings: [],
    reversible: true,
    ...overrides,
  };
}

export const Safe: Story = {
  args: {
    preview: preview({
      diffs: [
        { field: 'description', before: 'Original blurb', after: 'Updated blurb' },
      ],
      reversible: true,
    }),
  },
};

export const Reversible: Story = {
  args: {
    preview: preview({
      diffs: [
        { field: 'topics', before: 'rust', after: 'rust, async' },
        { field: 'language', before: 'Rust', after: 'Rust' },
      ],
      side_effects: ['Search index refresh queued.'],
      reversible: true,
    }),
  },
};

export const Irreversible: Story = {
  args: {
    preview: preview({
      diffs: [
        { field: 'default_branch', before: 'main', after: 'master' },
      ],
      side_effects: ['12 open PRs will be retargeted.'],
      warnings: [
        'Default branch swap rewrites CI references; reverting requires a separate change.',
      ],
      reversible: false,
    }),
  },
};

export const ProductionImpact: Story = {
  name: 'Production impact',
  args: {
    preview: preview({
      diffs: [
        { field: 'visibility', before: 'public', after: 'private' },
        { field: 'feature_pages', before: 'true', after: 'false' },
      ],
      side_effects: [
        'Public clones disabled.',
        '3 deploy keys will be revoked.',
        'External webhook endpoints will lose access.',
      ],
      warnings: [
        'External clients lose read access immediately.',
        'Search index drops this repo from public results.',
      ],
      reversible: false,
    }),
  },
};

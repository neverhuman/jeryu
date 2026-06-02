// SettingsDiffPreview.test.tsx — Phase 3 (W-FE-12).
//
// Pins the rendering contract for the preview surface:
//   1. Each field-change row shows the before + after value.
//   2. Empty (`""`) values render as "(empty)" so reviewers don't see a
//      blank cell.
//   3. `null` renders as ∅ to distinguish from empty-string.
//   4. Reversible flag swaps between "Reversible" / "Not reversible".
//   5. Side effects + warnings sections only render when populated.
//   6. The "no changes" path renders when the preview returned an empty
//      diff list.

import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { SettingsDiffPreview as SettingsDiffPreviewWire } from '../../../api/types';
import { SettingsDiffPreview } from '../SettingsDiffPreview';

const BASE: SettingsDiffPreviewWire = {
  repo: { id: 'repo-uuid-1', host: 'jeryu', owner: 'veox', name: 'redline' },
  current_hash: 'hash-1',
  diffs: [],
  side_effects: [],
  warnings: [],
  reversible: true,
};

describe('SettingsDiffPreview', () => {
  it('renders the loading state when isLoading is set', () => {
    render(<SettingsDiffPreview preview={null} isLoading />);
    expect(screen.getByText(/Computing preview/)).toBeInTheDocument();
  });

  it('renders the empty hint when no preview is set', () => {
    render(<SettingsDiffPreview preview={null} />);
    expect(
      screen.getByText(/Change a field and press Preview/)
    ).toBeInTheDocument();
  });

  it('renders per-field before/after values', () => {
    const preview: SettingsDiffPreviewWire = {
      ...BASE,
      diffs: [
        { field: 'description', before: 'previous desc', after: 'updated desc' },
        { field: 'visibility', before: 'public', after: 'private' },
      ],
    };
    render(<SettingsDiffPreview preview={preview} />);
    expect(screen.getByText('description')).toBeInTheDocument();
    expect(screen.getByText('previous desc')).toBeInTheDocument();
    expect(screen.getByText('updated desc')).toBeInTheDocument();
    expect(screen.getByText('visibility')).toBeInTheDocument();
    expect(screen.getByText('public')).toBeInTheDocument();
    expect(screen.getByText('private')).toBeInTheDocument();
  });

  it('renders ∅ for null and "(empty)" for empty-string values', () => {
    const preview: SettingsDiffPreviewWire = {
      ...BASE,
      diffs: [
        { field: 'homepage', before: null, after: '' },
        { field: 'default_branch', before: 'main', after: null },
      ],
    };
    render(<SettingsDiffPreview preview={preview} />);
    // The component renders the `(empty)` marker exactly once for the
    // empty string and the null markers for the explicit-null cells.
    expect(screen.getByText('(empty)')).toBeInTheDocument();
    expect(screen.getAllByText('∅').length).toBeGreaterThanOrEqual(2);
  });

  it('surfaces affected entities and warnings when set', () => {
    const preview: SettingsDiffPreviewWire = {
      ...BASE,
      reversible: false,
      diffs: [
        { field: 'visibility', before: 'internal', after: 'public' },
      ],
      side_effects: [
        '3 protected branches affected',
        '12 open PRs would be re-evaluated',
      ],
      warnings: [
        'Tightening visibility may cut off external collaborators.',
      ],
    };
    render(<SettingsDiffPreview preview={preview} />);
    expect(screen.getByText('Affected entities')).toBeInTheDocument();
    expect(
      screen.getByText('3 protected branches affected')
    ).toBeInTheDocument();
    expect(
      screen.getByText('12 open PRs would be re-evaluated')
    ).toBeInTheDocument();
    expect(screen.getByText(/Warnings/)).toBeInTheDocument();
    expect(
      screen.getByText(/Tightening visibility may cut off/)
    ).toBeInTheDocument();
    expect(screen.getByText(/Not reversible/)).toBeInTheDocument();
  });

  it('shows "no changes" when the diff list is empty', () => {
    render(<SettingsDiffPreview preview={BASE} />);
    expect(
      screen.getByText(/produced no changes against the current snapshot/)
    ).toBeInTheDocument();
  });

  it('renders the reversible pill with positive treatment by default', () => {
    render(<SettingsDiffPreview preview={BASE} />);
    expect(screen.getByText('Reversible')).toBeInTheDocument();
  });
});

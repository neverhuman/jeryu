// RepoFamilyCard.test.tsx — render smoke for the family tile.
//
// The tile must surface the family name, member count, rollup health, and
// link to the family drill-down page (URL-encoding the family name).

import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it } from 'vitest';

import { RepoFamilyCard } from '../RepoFamilyCard';
import type { FamilyRollup } from '../familyRollup';

const ROLLUP: FamilyRollup = {
  name: 'veox-split',
  repos: [],
  memberCount: 3,
  health: 'degraded',
  openPullRequests: 5,
  failingChecks: 2,
  runningJobs: 1,
  activeAgents: 4,
  updatedAt: '2026-06-09T08:30:00Z',
  worstScore: 88,
};

describe('RepoFamilyCard', () => {
  it('renders name, count, health, and the family href', () => {
    render(
      <MemoryRouter>
        <RepoFamilyCard family={ROLLUP} />
      </MemoryRouter>
    );
    expect(screen.getByText('veox-split')).toBeInTheDocument();
    expect(screen.getByText('3 repos')).toBeInTheDocument();
    expect(
      screen.getByRole('status', { name: /Health: degraded/ })
    ).toBeInTheDocument();
    expect(
      screen.getByRole('link', {
        name: 'Open family veox-split (3 repositories)',
      })
    ).toHaveAttribute('href', '/repos/family/veox-split');
    expect(
      screen.getByLabelText('5 open pull requests')
    ).toBeInTheDocument();
    expect(screen.getByLabelText('2 failing checks')).toBeInTheDocument();
    expect(screen.getByLabelText('1 running jobs')).toBeInTheDocument();
  });

  it('URL-encodes the family name in the href', () => {
    render(
      <MemoryRouter>
        <RepoFamilyCard family={{ ...ROLLUP, name: 'a b/c' }} />
      </MemoryRouter>
    );
    expect(screen.getByRole('link')).toHaveAttribute(
      'href',
      '/repos/family/a%20b%2Fc'
    );
  });
});

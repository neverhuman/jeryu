// MirrorStatusBadge.test.tsx — mirror posture states.
//
// Absent / unconfigured mirrors render nothing; a configured mirror shows
// the last successful push age ("never pushed" before the first success);
// a failed last attempt flips to the danger ramp with an explanatory title.

import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { MirrorStatusBadge } from '../MirrorStatusBadge';
import type { RepositoryMirrorStatus } from '../../../api/types';

function mirror(
  overrides: Partial<RepositoryMirrorStatus> = {}
): RepositoryMirrorStatus {
  return {
    configured: true,
    last_attempt_at: '2026-06-09T09:00:00Z',
    last_attempt_ok: true,
    last_attempt_conclusion: 'success',
    last_success_at: '2026-06-09T09:00:00Z',
    ...overrides,
  };
}

describe('MirrorStatusBadge', () => {
  it('renders nothing when the mirror field is absent', () => {
    const { container } = render(<MirrorStatusBadge />);
    expect(container).toBeEmptyDOMElement();
  });

  it('renders nothing when the mirror is null', () => {
    const { container } = render(<MirrorStatusBadge mirror={null} />);
    expect(container).toBeEmptyDOMElement();
  });

  it('renders nothing when the mirror is not configured', () => {
    const { container } = render(
      <MirrorStatusBadge mirror={mirror({ configured: false })} />
    );
    expect(container).toBeEmptyDOMElement();
  });

  it('shows the last successful push age when healthy', () => {
    const recent = new Date(Date.now() - 120_000).toISOString();
    render(<MirrorStatusBadge mirror={mirror({ last_success_at: recent })} />);
    const badge = screen.getByRole('status');
    expect(badge).not.toHaveClass('repo-mirror-badge--danger');
    expect(badge.getAttribute('title')).toMatch(/^Mirror pushed /);
  });

  it('shows "never pushed" when no success has been recorded', () => {
    render(
      <MirrorStatusBadge
        mirror={mirror({ last_attempt_at: null, last_success_at: null })}
      />
    );
    const badge = screen.getByRole('status');
    expect(badge).toHaveTextContent('never pushed');
    expect(badge).not.toHaveClass('repo-mirror-badge--danger');
  });

  it('uses danger styling and a failure title when the last attempt failed', () => {
    render(
      <MirrorStatusBadge
        mirror={mirror({
          last_attempt_ok: false,
          last_attempt_conclusion: 'failure',
          last_success_at: '2026-06-07T10:00:00Z',
        })}
      />
    );
    const badge = screen.getByRole('status');
    expect(badge).toHaveClass('repo-mirror-badge--danger');
    expect(badge.getAttribute('title')).toMatch(
      /^Last mirror push failed · last success /
    );
  });

  it('reports "last success never" when a failing mirror has never pushed', () => {
    render(
      <MirrorStatusBadge
        mirror={mirror({ last_attempt_ok: false, last_success_at: null })}
      />
    );
    expect(screen.getByRole('status').getAttribute('title')).toBe(
      'Last mirror push failed · last success never'
    );
  });
});

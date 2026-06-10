// JankuraiScoreBadge.test.tsx — threshold + null-state coverage.
//
// The pill has four shapes: good (score >= 85), warn (score < 85),
// "audit failed" (no score but a decision), and neutral "no score"
// (no audit ingested). Relative-time output depends on the wall clock,
// so assertions match on the stable "scored" prefix instead of exact text.

import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { JankuraiScoreBadge } from '../JankuraiScoreBadge';

describe('JankuraiScoreBadge', () => {
  it('renders a good pill at the 85 threshold', () => {
    render(<JankuraiScoreBadge score={85} scoredAt="2026-06-09T08:30:00Z" />);
    const badge = screen.getByRole('status');
    expect(badge).toHaveTextContent('85');
    expect(badge).toHaveClass('repo-score-badge--good');
  });

  it('renders a good pill above the threshold', () => {
    render(<JankuraiScoreBadge score={92} decision="pass" />);
    expect(screen.getByRole('status')).toHaveClass('repo-score-badge--good');
  });

  it('renders a warn pill below the threshold', () => {
    render(<JankuraiScoreBadge score={84} decision="fail" />);
    const badge = screen.getByRole('status');
    expect(badge).toHaveTextContent('84');
    expect(badge).toHaveClass('repo-score-badge--warn');
  });

  it('renders "audit failed" when the tool could not score the tree', () => {
    render(
      <JankuraiScoreBadge
        score={null}
        decision="tool-failed"
        scoredAt="2026-06-09T08:30:00Z"
      />
    );
    const badge = screen.getByRole('status');
    expect(badge).toHaveTextContent('audit failed');
    expect(badge).toHaveClass('repo-score-badge--warn');
    expect(badge.getAttribute('aria-label')).toContain('tool-failed');
  });

  it('renders neutral "no score" when no audit exists', () => {
    render(<JankuraiScoreBadge />);
    const badge = screen.getByRole('status');
    expect(badge).toHaveTextContent('no score');
    expect(badge).toHaveClass('repo-score-badge--neutral');
  });

  it('carries the score and scored-at time in title and aria-label', () => {
    const recent = new Date(Date.now() - 60_000).toISOString();
    render(<JankuraiScoreBadge score={91} scoredAt={recent} />);
    const badge = screen.getByRole('status');
    const label = badge.getAttribute('aria-label') ?? '';
    expect(label).toContain('91');
    expect(label).toContain('scored');
    // The relative timestamp is appended after "scored ".
    expect(label).toMatch(/scored .+/);
    expect(badge.getAttribute('title')).toBe(label);
  });

  it('omits the scored-at suffix when no timestamp is available', () => {
    render(<JankuraiScoreBadge score={91} />);
    const label = screen
      .getByRole('status')
      .getAttribute('aria-label');
    expect(label).toBe('jankurai score 91');
  });
});

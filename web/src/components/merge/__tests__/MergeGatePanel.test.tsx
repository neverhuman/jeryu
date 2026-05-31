// MergeGatePanel.test.tsx — Phase 3 (W-FE-11).
//
// Pins the contract that:
//   1. A PASS verdict renders the "All 12 gates passed" line + no blockers.
//   2. A BLOCKED verdict renders each blocker, translates known codes to
//      human titles, and includes the raw `code` for debugging.
//   3. An unknown blocker code falls back to the server-supplied message.
//   4. A null passport renders the "No verdict yet" pending state.

import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { MergePassport } from '../../../api/types';
import { MergeGatePanel } from '../MergeGatePanel';

const HEAD_SHA = 'abcdef1234567890abcdef1234567890abcdef12';

describe('MergeGatePanel', () => {
  it('renders PASS state with a green tick line', () => {
    const passport: MergePassport = {
      status: 'pass',
      head_sha: HEAD_SHA,
      blockers: [],
      evaluated_at: '2026-05-26T12:00:00Z',
    };
    render(<MergeGatePanel passport={passport} />);
    expect(screen.getByText(/Merge Passport: PASS/)).toBeInTheDocument();
    expect(screen.getByText(/All 12 gates passed/)).toBeInTheDocument();
    expect(screen.queryByRole('list', { name: /Passport blockers/i })).toBeNull();
  });

  it('renders BLOCKED state with translated blocker titles', () => {
    const passport: MergePassport = {
      status: 'blocked',
      head_sha: HEAD_SHA,
      blockers: [
        {
          code: 'passport_blocked_approvals',
          message: 'Required approver count not satisfied.',
          details: '2 of 3 approvals on this head.',
        },
        {
          code: 'passport_blocked_checks',
          message: 'Required checks failing.',
          details: null,
        },
      ],
      evaluated_at: '2026-05-26T12:00:00Z',
    };
    render(<MergeGatePanel passport={passport} />);
    expect(screen.getByText(/Merge Passport: BLOCKED/)).toBeInTheDocument();
    // Translated human titles.
    expect(screen.getByText('Approvals not met')).toBeInTheDocument();
    expect(screen.getByText('Required checks failing')).toBeInTheDocument();
    // Raw codes still rendered for debugging.
    expect(screen.getByText('passport_blocked_approvals')).toBeInTheDocument();
    expect(screen.getByText('passport_blocked_checks')).toBeInTheDocument();
    // Details surfaced for the first blocker.
    expect(
      screen.getByText('2 of 3 approvals on this head.')
    ).toBeInTheDocument();
  });

  it('falls back to the server message for unknown codes', () => {
    const passport: MergePassport = {
      status: 'blocked',
      head_sha: HEAD_SHA,
      blockers: [
        {
          code: 'passport_blocked_custom_xyz',
          message: 'Custom-server-side rule failed.',
          details: null,
        },
      ],
      evaluated_at: '2026-05-26T12:00:00Z',
    };
    const { container } = render(<MergeGatePanel passport={passport} />);
    // Title falls back to the raw code; we expect at least one element
    // (title row + the code tag for debugging).
    const matches = screen.getAllByText('passport_blocked_custom_xyz');
    expect(matches.length).toBeGreaterThanOrEqual(1);
    // The first occurrence sits inside the blocker title element so the
    // human-readable spot uses it.
    expect(
      container.querySelector('.merge-gate__blocker-title')?.textContent
    ).toBe('passport_blocked_custom_xyz');
    // Hint shows the server message.
    expect(
      screen.getByText('Custom-server-side rule failed.')
    ).toBeInTheDocument();
  });

  it('renders the pending state when no passport is available', () => {
    render(<MergeGatePanel passport={null} />);
    expect(screen.getByText(/Merge Passport/)).toBeInTheDocument();
    expect(
      screen.getByText(/No verdict yet for this head SHA/)
    ).toBeInTheDocument();
  });

  it('renders the loading state when computing', () => {
    render(<MergeGatePanel passport={null} isLoading />);
    expect(screen.getByText(/Computing verdict/)).toBeInTheDocument();
  });
});

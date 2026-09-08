// ReviewSidebar.test.tsx — review-action sidebar render + interaction (Phase G).
//
// The ReviewSidebar is the operator's mutation surface on the PR cockpit. It
// must:
//   1. Always carry the EXACT head SHA the reviewer saw into `onApprove`
//      (§35.1.7) — the button label embeds the short SHA and the callback
//      receives the full 40-char SHA.
//   2. Gate the Merge CTA on the Passport verdict AND `mergeable.can_merge`:
//      a blocked passport (or `can_merge:false`) shows the "Merge blocked by
//      the Passport" notice instead of the merge buttons.
//   3. Surface the squash/rebase method buttons only when merge is allowed,
//      each routing the chosen method into `onMerge`.
//   4. Disable every action while a mutation is in-flight (`isBusy`).
//   5. Toggle the request-changes composer and submit a trimmed body.

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import type { PullRequestDetail } from '../../../api/types';
import { ReviewSidebar } from '../ReviewSidebar';

const HEAD_SHA = 'abcdef1234567890abcdef1234567890abcdef12';

function makeDetail(
  over: {
    passport?: 'pass' | 'blocked';
    can_merge?: boolean;
    approvals?: number;
    required_approvals?: number;
    changes_requested?: number;
    unresolved_threads?: number;
    user_review_state?: string | null;
    passport_hash?: string | null;
  } = {}
): PullRequestDetail {
  const passport = over.passport ?? 'blocked';
  const canMerge = over.can_merge ?? passport === 'pass';
  return {
    summary: {
      repo: { id: 'r1', host: 'jeryu', owner: 'neverhuman', name: 'jeryu' },
      number: 7,
      entity: { kind: 'pull_request', id: 'r1#7' },
      title: 'A PR',
      author: '@author',
      head_ref: 'feature/x',
      base_ref: 'main',
      head_sha: HEAD_SHA,
      base_sha: 'base000000000000000000000000000000000000',
      state: 'open',
      draft: false,
      mergeable: {
        level: canMerge ? 'mergeable' : 'blocked',
        can_merge: canMerge,
        reason: canMerge ? null : 'Passport blocked',
        exact_head_sha: HEAD_SHA,
        required_gate: canMerge ? null : 'passport',
      },
      review: {
        required_approvals: over.required_approvals ?? 2,
        approvals: over.approvals ?? 0,
        changes_requested: over.changes_requested ?? 0,
        unresolved_threads: over.unresolved_threads ?? 0,
        user_review_state: over.user_review_state ?? null,
      },
      checks: { total: 1, passing: 1, failing: 0, pending: 0, skipped: 0 },
      agents: {
        active_sessions: 0,
        proposed_patches: 0,
        evidence_packets: 0,
        blockers: 0,
      },
      labels: [],
      updated_at: '2026-05-26T00:00:00Z',
      passport_hash: over.passport_hash ?? 'hash-1',
      available_actions: [],
    },
    description: null,
    merge_passport: {
      status: passport,
      head_sha: HEAD_SHA,
      blockers:
        passport === 'blocked'
          ? [
              {
                code: 'passport_blocked_approvals',
                message: 'Approvals not met.',
                details: null,
              },
            ]
          : [],
      evaluated_at: '2026-05-26T00:00:00Z',
    },
    passport_hash: over.passport_hash ?? 'hash-1',
  };
}

describe('ReviewSidebar', () => {
  it('renders the approval posture and the exact-SHA approve label', () => {
    render(
      <ReviewSidebar
        detail={makeDetail({ approvals: 1, required_approvals: 2 })}
        onApprove={vi.fn()}
        onMerge={vi.fn()}
      />
    );
    expect(screen.getByText('1/2 approvals')).toBeInTheDocument();
    expect(
      screen.getByRole('button', {
        name: new RegExp(`Approve exact SHA ${HEAD_SHA.slice(0, 7)}`),
      })
    ).toBeInTheDocument();
  });

  it('passes the FULL head SHA to onApprove when Approve is clicked', async () => {
    const onApprove = vi.fn();
    const user = userEvent.setup();
    render(
      <ReviewSidebar detail={makeDetail()} onApprove={onApprove} onMerge={vi.fn()} />
    );
    await user.click(
      screen.getByRole('button', { name: /Approve exact SHA/ })
    );
    expect(onApprove).toHaveBeenCalledTimes(1);
    expect(onApprove).toHaveBeenCalledWith(HEAD_SHA);
  });

  it('blocks merge when the Passport is blocked', () => {
    render(
      <ReviewSidebar
        detail={makeDetail({ passport: 'blocked' })}
        onApprove={vi.fn()}
        onMerge={vi.fn()}
      />
    );
    expect(
      screen.getByText(/Merge blocked by the Passport/i)
    ).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /^Merge$/ })
    ).not.toBeInTheDocument();
  });

  it('blocks merge when the Passport passes but mergeable.can_merge is false', () => {
    render(
      <ReviewSidebar
        detail={makeDetail({ passport: 'pass', can_merge: false })}
        onApprove={vi.fn()}
        onMerge={vi.fn()}
      />
    );
    expect(
      screen.getByText(/Merge blocked by the Passport/i)
    ).toBeInTheDocument();
  });

  it('enables Merge + squash/rebase methods when the Passport passes', async () => {
    const onMerge = vi.fn();
    const user = userEvent.setup();
    render(
      <ReviewSidebar
        detail={makeDetail({ passport: 'pass', can_merge: true })}
        onApprove={vi.fn()}
        onMerge={onMerge}
      />
    );
    expect(screen.getByText(/ready to merge/i)).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /^Merge$/ }));
    expect(onMerge).toHaveBeenLastCalledWith({
      expectedHeadSha: HEAD_SHA,
      expectedPassportHash: 'hash-1',
      method: 'merge',
    });

    await user.click(screen.getByRole('button', { name: /Squash/ }));
    expect(onMerge).toHaveBeenLastCalledWith(
      expect.objectContaining({ method: 'squash' })
    );

    await user.click(screen.getByRole('button', { name: /Rebase/ }));
    expect(onMerge).toHaveBeenLastCalledWith(
      expect.objectContaining({ method: 'rebase' })
    );
  });

  it('disables all actions while a mutation is in-flight (isBusy)', () => {
    render(
      <ReviewSidebar
        detail={makeDetail({ passport: 'pass', can_merge: true })}
        onApprove={vi.fn()}
        onMerge={vi.fn()}
        onRequestChanges={vi.fn()}
        isBusy
      />
    );
    expect(
      screen.getByRole('button', { name: /Approve exact SHA/ })
    ).toBeDisabled();
    expect(screen.getByRole('button', { name: /^Merge$/ })).toBeDisabled();
  });

  it('toggles the request-changes composer and submits a trimmed body', async () => {
    const onRequestChanges = vi.fn();
    const user = userEvent.setup();
    render(
      <ReviewSidebar
        detail={makeDetail()}
        onApprove={vi.fn()}
        onMerge={vi.fn()}
        onRequestChanges={onRequestChanges}
      />
    );

    // The composer is hidden until "Request changes" is clicked.
    expect(
      screen.queryByPlaceholderText(/What needs to change/i)
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /Request changes/i }));
    const textarea = screen.getByLabelText(/Requested changes/i);
    await user.type(textarea, '   please rename the variable   ');

    await user.click(screen.getByRole('button', { name: /^Submit$/ }));
    expect(onRequestChanges).toHaveBeenCalledWith(
      HEAD_SHA,
      'please rename the variable'
    );
  });

  it('shows the viewer review state when present', () => {
    render(
      <ReviewSidebar
        detail={makeDetail({ user_review_state: 'approved' })}
        onApprove={vi.fn()}
        onMerge={vi.fn()}
      />
    );
    expect(screen.getByText(/Your review:/i)).toBeInTheDocument();
    expect(screen.getByText('approved')).toBeInTheDocument();
  });
});

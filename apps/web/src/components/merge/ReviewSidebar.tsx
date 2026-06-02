// ReviewSidebar.tsx — actions sidebar: approve / request changes / merge
// (W-FE-11).
//
// The Merge button is gated on the Passport verdict (§35.2.4). The Approve
// button always carries the SHA the reviewer saw (`expected_head_sha`);
// when the server rejects with `merge_sha_stale` the parent surfaces the
// recovery banner.

import { Check, GitMerge, ShieldAlert, XCircle } from 'lucide-react';
import { useState } from 'react';

import { ActionButton } from '../action/ActionButton';
import type { PullRequestDetail } from '../../api/types';

import './merge.css';

export interface ReviewSidebarProps {
  detail: PullRequestDetail;
  /** Called when the reviewer clicks Approve. */
  onApprove: (expectedHeadSha: string) => Promise<void> | void;
  /** Called when the reviewer clicks Request changes. */
  onRequestChanges?: (expectedHeadSha: string, body: string) => Promise<void> | void;
  /** Called when the reviewer clicks Merge. */
  onMerge: (params: {
    expectedHeadSha: string;
    expectedPassportHash: string | null;
    method: 'merge' | 'squash' | 'rebase';
  }) => Promise<void> | void;
  /** When true, mutations are disabled (in-flight). */
  isBusy?: boolean;
  className?: string;
}

export function ReviewSidebar({
  detail,
  onApprove,
  onRequestChanges,
  onMerge,
  isBusy = false,
  className,
}: ReviewSidebarProps): JSX.Element {
  const review = detail.summary.review;
  const passport = detail.merge_passport;
  const headSha = detail.summary.head_sha;
  const mergeAllowed = passport.status === 'pass' && detail.summary.mergeable.can_merge;
  const reviewState = review.user_review_state ?? null;

  const [requestChangesOpen, setRequestChangesOpen] = useState(false);
  const [requestChangesBody, setRequestChangesBody] = useState('');

  const handleApprove = (): void => {
    void onApprove(headSha);
  };

  const handleMerge = (method: 'merge' | 'squash' | 'rebase'): void => {
    void onMerge({
      expectedHeadSha: headSha,
      expectedPassportHash: detail.passport_hash ?? null,
      method,
    });
  };

  const handleRequestChangesSubmit = (): void => {
    if (!onRequestChanges) return;
    const body = requestChangesBody.trim();
    if (!body) return;
    void onRequestChanges(headSha, body);
    setRequestChangesOpen(false);
    setRequestChangesBody('');
  };

  return (
    <section
      className={`review-sidebar ${className ?? ''}`.trim()}
      aria-label="Review actions"
    >
      <header className="review-sidebar__header">
        <h3 className="review-sidebar__title">Review</h3>
        <p className="review-sidebar__posture">
          <span className="review-sidebar__approvals">
            {review.approvals}/{review.required_approvals} approvals
          </span>
          {review.changes_requested > 0 ? (
            <span className="review-sidebar__changes">
              · {review.changes_requested} changes requested
            </span>
          ) : null}
          {review.unresolved_threads > 0 ? (
            <span className="review-sidebar__threads">
              · {review.unresolved_threads} unresolved
            </span>
          ) : null}
        </p>
        {reviewState ? (
          <p className="review-sidebar__user-state">
            Your review: <strong>{reviewState}</strong>
          </p>
        ) : null}
      </header>

      <div className="review-sidebar__actions">
        <ActionButton
          variant="primary"
          icon={<Check aria-hidden="true" size={12} />}
          onClick={handleApprove}
          disabled={isBusy}
          actionId="pull.approve"
        >
          Approve exact SHA {headSha.slice(0, 7)}
        </ActionButton>
        <ActionButton
          variant="default"
          icon={<XCircle aria-hidden="true" size={12} />}
          onClick={() => setRequestChangesOpen((v) => !v)}
          disabled={isBusy || !onRequestChanges}
          actionId="pull.request_changes"
        >
          Request changes
        </ActionButton>
      </div>

      {requestChangesOpen ? (
        <div className="review-sidebar__changes-form">
          <label htmlFor="request-changes-body" className="sr-only">
            Changes requested body
          </label>
          <textarea
            id="request-changes-body"
            className="review-sidebar__textarea"
            placeholder="What needs to change?"
            value={requestChangesBody}
            onChange={(e) => setRequestChangesBody(e.target.value)}
            rows={3}
          />
          <div className="review-sidebar__changes-actions">
            <ActionButton
              variant="ghost"
              onClick={() => setRequestChangesOpen(false)}
              disabled={isBusy}
            >
              Cancel
            </ActionButton>
            <ActionButton
              variant="primary"
              onClick={handleRequestChangesSubmit}
              disabled={isBusy || requestChangesBody.trim().length === 0}
            >
              Submit
            </ActionButton>
          </div>
        </div>
      ) : null}

      <div className="review-sidebar__merge">
        {mergeAllowed ? (
          <>
            <p className="review-sidebar__merge-hint">
              All gates green — ready to merge.
            </p>
            <ActionButton
              variant="primary"
              icon={<GitMerge aria-hidden="true" size={12} />}
              onClick={() => handleMerge('merge')}
              disabled={isBusy}
              actionId="pull.merge"
            >
              Merge
            </ActionButton>
            <div className="review-sidebar__merge-methods">
              <button
                type="button"
                className="review-sidebar__method"
                onClick={() => handleMerge('squash')}
                disabled={isBusy}
              >
                Squash
              </button>
              <button
                type="button"
                className="review-sidebar__method"
                onClick={() => handleMerge('rebase')}
                disabled={isBusy}
              >
                Rebase
              </button>
            </div>
          </>
        ) : (
          <div className="review-sidebar__merge-blocked">
            <ShieldAlert aria-hidden="true" size={14} />
            <span>
              Merge blocked by the Passport. Address blockers below to enable.
            </span>
          </div>
        )}
      </div>
    </section>
  );
}

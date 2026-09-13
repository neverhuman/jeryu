import type { PullReviewChallenge } from '../../api/types';
import { ActionButton } from '../action/ActionButton';

interface PreparedReviewProps {
  challenge: PullReviewChallenge;
  busy: boolean;
  problem?: string;
  onSubmit: () => void;
  onCancel: () => void;
}

export function PreparedReview({
  challenge,
  busy,
  problem,
  onSubmit,
  onCancel,
}: PreparedReviewProps): JSX.Element {
  const { git, reviewer } = challenge.snapshot;
  return (
    <section className="review-sidebar" aria-label="Prepared review">
      <h2 className="review-sidebar__title">Review prepared for {reviewer.login}</h2>
      <p>
        Approve commit <code>{git.source.commit_sha}</code> from{' '}
        <code>{git.source.reference}</code> against{' '}
        <code>{git.destination.reference}</code> at{' '}
        <code>{git.destination.commit_sha}</code>.
      </p>
      <p>
        Review the changes before submitting. This records your approval;
        the required merge checks still apply.
      </p>
      {problem ? <p role="alert">{problem}</p> : null}
      <div className="review-sidebar__actions">
        <ActionButton
          variant="primary"
          onClick={onSubmit}
          disabled={busy || !!problem}
        >
          {busy ? 'Submitting approval…' : 'Submit approval'}
        </ActionButton>
        <ActionButton variant="ghost" onClick={onCancel} disabled={busy}>
          Cancel review
        </ActionButton>
      </div>
    </section>
  );
}

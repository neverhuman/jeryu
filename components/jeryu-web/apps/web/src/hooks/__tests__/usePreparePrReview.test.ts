import { describe, expect, it } from 'vitest';
import { ApiError } from '../../api/client';
import type { PullReviewChallenge } from '../../api/types';
import { parsePreparedReview, preparationProblem } from '../usePreparePrReview';

const head = 'a'.repeat(40);
function challenge(): PullReviewChallenge {
  return {
    id: 'a59382b0-e8d1-4e18-a5a6-90f04f38ff4c',
    nonce: 'private-fixture-nonce',
    expires_at: new Date(Date.now() + 600_000).toISOString(),
    snapshot_sha256: 'b'.repeat(64),
    merge_qualified: false,
    blockers: ['Required merge authority is unavailable'],
    snapshot: {
      repository_id: 'repository', pull_number: 1,
      policy_revision: 'c'.repeat(64), reviewer: { login: 'reviewer' },
      git: {
        source: { reference: 'refs/heads/topic', commit_sha: head, tree_sha: 'd'.repeat(40) },
        destination: { reference: 'refs/heads/main', commit_sha: 'e'.repeat(40), tree_sha: 'f'.repeat(40) },
      },
    },
  };
}

describe('server review preparation', () => {
  it('retains the same challenge for submission and uncertain-response retries', () => {
    const input = challenge();
    const parsed = parsePreparedReview(input, 'repository', '1', head);
    expect(parsed).toEqual(input);
    expect(preparationProblem(parsed, 'repository', '1', head)).toBeUndefined();
    expect(parsed.merge_qualified).toBe(false);
  });

  it('refuses malformed or missing identities without exposing nonce bytes', () => {
    const valid = challenge();
    for (const input of [null, [], {}, { ...valid, id: 'bad' }, { ...valid, nonce: null },
      { ...valid, expires_at: 'invalid' }, { ...valid, snapshot: null },
      { ...valid, snapshot_sha256: '' }, { ...valid, snapshot: { ...valid.snapshot, pull_number: 1.5 } },
      { ...valid, snapshot: { ...valid.snapshot, git: { ...valid.snapshot.git, source: { ...valid.snapshot.git.source, commit_sha: 'abc' } } } },
    ]) {
      expect(() => parsePreparedReview(input, 'repository', '1', head)).toThrow(ApiError);
      try {
        parsePreparedReview(input, 'repository', '1', head);
      } catch (error) {
        expect(String(error)).not.toContain(valid.nonce);
      }
    }
  });

  it('refuses another repository, pull, head or an expired preparation', () => {
    for (const input of [
      { ...challenge(), snapshot: { ...challenge().snapshot, repository_id: 'other' } },
      { ...challenge(), snapshot: { ...challenge().snapshot, pull_number: 2 } },
      { ...challenge(), expires_at: new Date(Date.now() - 1).toISOString() },
    ]) {
      expect(() => parsePreparedReview(input, 'repository', '1', head)).toThrow(ApiError);
    }
    expect(() => parsePreparedReview(challenge(), 'repository', '1', '1'.repeat(40))).toThrow(ApiError);
  });

  it('checks expiry and route or head movement again at submission', () => {
    const input = challenge();
    const parsed = parsePreparedReview(input, 'repository', '1', head);
    const expires = Date.parse(input.expires_at);
    expect(preparationProblem(parsed, 'repository', '1', head, expires - 1)).toBeUndefined();
    expect(preparationProblem(parsed, 'repository', '1', head, expires)?.code).toBe('review_snapshot_expired');
    expect(preparationProblem(parsed, 'other', '1', head)?.code).toBe('review_snapshot_changed');
    expect(preparationProblem(parsed, 'repository', '2', head)?.code).toBe('review_snapshot_changed');
    expect(preparationProblem(parsed, 'repository', '1', '2'.repeat(40))?.code).toBe('review_snapshot_changed');
  });
});

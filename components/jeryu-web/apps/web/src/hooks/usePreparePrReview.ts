import { useMutation, type UseMutationResult } from '@tanstack/react-query';
import { z } from 'zod';

import { apiSend, ApiError } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { PreparePullReviewRequest, PullReviewChallenge } from '../api/types';

const oid = z.string().regex(/^(?:[0-9a-f]{40}|[0-9a-f]{64})$/);
const digest = z.string().regex(/^[0-9a-f]{64}$/);
const observedRef = z.object({
  reference: z.string().startsWith('refs/heads/'),
  commit_sha: oid,
  tree_sha: oid,
});
const challengeSchema: z.ZodType<PullReviewChallenge> = z.object({
  id: z.string().uuid(),
  nonce: z.string().min(1).max(512),
  expires_at: z.string().refine((value) => Number.isFinite(Date.parse(value))),
  snapshot_sha256: digest,
  snapshot: z.object({
    repository_id: z.string().min(1),
    pull_number: z.number().int().positive().max(Number.MAX_SAFE_INTEGER),
    git: z.object({ source: observedRef, destination: observedRef }),
    policy_revision: digest,
    reviewer: z.object({ login: z.string().min(1) }),
  }),
  merge_qualified: z.boolean(),
  blockers: z.array(z.string()),
});

export function preparationProblem(
  challenge: PullReviewChallenge,
  repoId: string | null,
  prNumber: string | null,
  headSha: string | undefined,
  now = Date.now()
): ApiError | undefined {
  if (
    challenge.snapshot.repository_id !== repoId ||
    String(challenge.snapshot.pull_number) !== prNumber ||
    challenge.snapshot.git.source.commit_sha !== headSha
  ) {
    return new ApiError(409, {
      code: 'review_snapshot_changed',
      message: 'The prepared review does not match this pull request and commit. Refresh before reviewing.',
    });
  }
  if (Date.parse(challenge.expires_at) <= now) {
    return new ApiError(409, {
      code: 'review_snapshot_expired',
      message: 'This review preparation expired. Refresh and review the current source before submitting.',
    });
  }
  return undefined;
}

export function parsePreparedReview(
  input: unknown,
  repoId: string,
  prNumber: string,
  headSha: string
): PullReviewChallenge {
  const parsed = challengeSchema.safeParse(input);
  if (!parsed.success) {
    throw new ApiError(502, {
      code: 'invalid_review_challenge',
      message: 'The server returned an incomplete review snapshot. No approval was submitted.',
    });
  }
  const problem = preparationProblem(parsed.data, repoId, prNumber, headSha);
  if (problem) throw problem;
  return parsed.data;
}

export function usePreparePrReview(
  repoId: string | null,
  prNumber: string | null
): UseMutationResult<PullReviewChallenge, ApiError, PreparePullReviewRequest> {
  return useMutation({
    gcTime: 0,
    retry: false,
    mutationFn: async (body: PreparePullReviewRequest) => {
      if (!repoId || !prNumber) {
        throw new ApiError(0, {
          code: 'invalid_state',
          message: 'Repository or pull request not resolved yet.',
        });
      }
      const response = await apiSend<unknown>(
        endpoints.pullReviewChallenge(repoId, prNumber),
        body
      );
      return parsePreparedReview(response, repoId, prNumber, body.expected_head_sha);
    },
  });
}

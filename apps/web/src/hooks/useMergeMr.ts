// useMergeMr.ts — `POST /merge-requests/{iid}/merge` mutation (W-FE-11).
//
// Symmetric to `useApproveMr`. The merge body must carry the Passport hash
// the UI saw when the button was rendered so the backend can reject if the
// gate snapshot drifted (§35.2.4).

import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from '@tanstack/react-query';

import { apiSend, ApiError } from '../api/client';
import { endpoints } from '../api/endpoints';
import type {
  MergeMergeRequest,
  MergeRequestDetail,
} from '../api/types';

import { mergeRequestQueryKey } from './useMergeRequest';

function newIdempotencyKey(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `mr-merge-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function useMergeMr(
  repoId: string | null,
  iid: string | null
): UseMutationResult<MergeRequestDetail, ApiError, MergeMergeRequest> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (body: MergeMergeRequest) => {
      if (!repoId || !iid) {
        throw new ApiError(0, {
          code: 'invalid_state',
          message: 'Repository or merge request not resolved yet.',
        });
      }
      return apiSend<MergeRequestDetail>(
        endpoints.mergeRequestMerge(repoId, iid),
        body,
        { idempotencyKey: newIdempotencyKey() }
      );
    },
    onSuccess: (data) => {
      queryClient.setQueryData(
        mergeRequestQueryKey(repoId, iid),
        data
      );
    },
  });
}

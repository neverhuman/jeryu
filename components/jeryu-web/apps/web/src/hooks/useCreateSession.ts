// useCreateSession.ts — `POST /repos/{id}/sessions` mutation.
//
// Backs the Agents lens "New Session" button: creates a fresh isolated agent
// run on the repository, then the caller deep-links to `.../agents/{run_id}`
// and mounts the live `<AgentTerminal>`. On success the per-repo agent-runs
// list is invalidated so the new run appears without a manual refresh.

import {
  useMutation,
  useQueryClient,
  type UseMutationResult,
} from '@tanstack/react-query';

import { ApiError } from '../api/client';
import { createSession } from '../api/sessions';
import type { CreateSessionResponse } from '../api/types';

export function useCreateSession(
  repoId: string | null
): UseMutationResult<CreateSessionResponse, ApiError, string> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (agentId: string) => {
      if (!repoId) {
        throw new ApiError(0, {
          code: 'invalid_state',
          message: 'Repository not resolved yet.',
        });
      }
      return createSession(repoId, agentId);
    },
    onSuccess: () => {
      if (repoId) {
        queryClient.invalidateQueries({
          queryKey: ['repo-agent-runs', repoId],
        });
      }
    },
  });
}

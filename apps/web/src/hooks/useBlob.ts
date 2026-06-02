// useBlob.ts — React Query hook for `GET /api/v1/repos/{id}/blob` (W-FE-10).
//
// Markdown files (`*.md` / `*.markdown`) get the `render=html` query param so
// the server returns sanitized HTML alongside the raw text. Non-markdown
// blobs always request just the raw bytes (the viewer renders syntax
// highlighting client-side via Monaco).

import { useQuery, type UseQueryResult } from '@tanstack/react-query';

import { apiGet } from '../api/client';
import { endpoints } from '../api/endpoints';
import type { BlobResponse } from '../api/types';

export function blobQueryKey(
  repoId: string | null,
  ref: string,
  path: string,
  render: 'html' | undefined
): readonly unknown[] {
  return ['repo', repoId, 'blob', ref, path, render ?? 'raw'] as const;
}

export function isMarkdownPath(path: string): boolean {
  const lower = path.toLowerCase();
  return lower.endsWith('.md') || lower.endsWith('.markdown');
}

export function useBlob(
  repoId: string | null,
  ref: string,
  path: string
): UseQueryResult<BlobResponse, Error> {
  const render: 'html' | undefined = isMarkdownPath(path) ? 'html' : undefined;
  return useQuery({
    queryKey: blobQueryKey(repoId, ref, path, render),
    queryFn: ({ signal }) =>
      apiGet<BlobResponse>(
        endpoints.blob(repoId as string, { ref, path, render }),
        { signal }
      ),
    enabled:
      typeof repoId === 'string' &&
      repoId.length > 0 &&
      ref.length > 0 &&
      path.length > 0,
    staleTime: 30_000,
  });
}

// ReadmePanel.tsx — README rendering surface (W-FE-09).
//
// Pulls the rendered Markdown via `useMarkdown(repoId, ref)` and lights up
// the appropriate UX-QA state. A 404 from the README endpoint resolves to
// the empty state ("No README found") because that is the expected outcome
// when a repository has not initialized one, not a real error.

import { FileText } from 'lucide-react';

import { ApiError } from '../../api/client';
import { useMarkdown } from '../../hooks/useMarkdown';
import {
  EmptyState,
  ErrorState,
  LoadingState,
  PermissionDeniedState,
} from '../state';

import { MarkdownRenderer } from './MarkdownRenderer';

export interface ReadmePanelProps {
  repoId: string | null;
  ref?: string;
}

export function ReadmePanel({ repoId, ref }: ReadmePanelProps): JSX.Element {
  const query = useMarkdown(repoId, ref);

  if (query.isPending) {
    return <LoadingState title="Loading README…" rows={6} />;
  }

  if (query.isError) {
    const err = query.error;
    if (err instanceof ApiError) {
      if (err.status === 404 || err.code === 'not_found') {
        return (
          <EmptyState
            title="No README found"
            description="This repository does not include a README at the selected ref."
            icon={FileText}
          />
        );
      }
      if (err.status === 403 || err.code === 'permission_denied') {
        return (
          <PermissionDeniedState
            description="You do not have permission to view this README."
            missingPermission="repo.read"
          />
        );
      }
    }
    return (
      <ErrorState
        title="Could not render the README."
        error={query.error}
      />
    );
  }

  const data = query.data;
  if (!data || !data.html) {
    return (
      <EmptyState
        title="No README found"
        description="This repository does not include a README at the selected ref."
        icon={FileText}
      />
    );
  }

  return <MarkdownRenderer html={data.html} />;
}

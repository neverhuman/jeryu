// PullRequestCockpit.tsx — three-pane body for the PR review page (W-FE-11).
//
// Renders the Files / Diff / Review panes once the PR detail has resolved.
// Kept separate from `PullRequestPage` so the page module stays focused on
// data fetching, mutation wiring, and the loading/error/recovery guards.

import {
  ChecksPanel,
  DiffFileTree,
  DiffViewer,
  MergeGatePanel,
  ReviewSidebar,
  ThreadList,
  type DiffViewerMode,
} from '../components/merge';
import { ErrorState, LoadingState } from '../components/state';
import type { usePrChecks } from '../hooks/usePrChecks';
import type { usePrDiff } from '../hooks/usePrDiff';
import type { usePrThreads } from '../hooks/usePrThreads';
import type { usePullRequest } from '../hooks/usePullRequest';
import type { PullRequestDiffFile } from '../api/types';

type PrDetail = NonNullable<ReturnType<typeof usePullRequest>['data']>;

export interface PullRequestCockpitProps {
  data: PrDetail;
  diff: ReturnType<typeof usePrDiff>;
  checks: ReturnType<typeof usePrChecks>;
  threads: ReturnType<typeof usePrThreads>;
  activeFilePath: string | null;
  activeFile: PullRequestDiffFile | null;
  viewedPaths: Set<string>;
  diffMode: string;
  isBusy: boolean;
  onSelectFile: (path: string) => void;
  onToggleViewed: (path: string, viewed: boolean) => void;
  onDiffModeChange: (mode: DiffViewerMode) => void;
  onApprove: (expectedHeadSha: string) => Promise<void>;
  onMerge: (input: {
    expectedHeadSha: string;
    expectedPassportHash: string | null;
    method: 'merge' | 'squash' | 'rebase';
  }) => Promise<void>;
}

export function PullRequestCockpit({
  data,
  diff,
  checks,
  threads,
  activeFilePath,
  activeFile,
  viewedPaths,
  diffMode,
  isBusy,
  onSelectFile,
  onToggleViewed,
  onDiffModeChange,
  onApprove,
  onMerge,
}: PullRequestCockpitProps): JSX.Element {
  return (
    <div className="pr-cockpit">
      <aside className="pr-cockpit__pane pr-cockpit__pane--files" tabIndex={0}>
        <h2 className="pr-cockpit__pane-title">Files</h2>
        {diff.isPending ? (
          <LoadingState title="Loading diff…" variant="skeleton" rows={6} />
        ) : diff.error ? (
          <ErrorState title="Diff failed" error={diff.error} />
        ) : (
          <DiffFileTree
            files={diff.data?.files ?? []}
            activePath={activeFilePath}
            viewedPaths={viewedPaths}
            onSelect={onSelectFile}
            onToggleViewed={onToggleViewed}
          />
        )}
      </aside>

      <main className="pr-cockpit__pane pr-cockpit__pane--diff">
        <h2 className="pr-cockpit__pane-title">Diff</h2>
        {!activeFile ? (
          <LoadingState
            title={
              diff.isPending
                ? 'Loading diff…'
                : 'No file selected. Choose a file from the left.'
            }
            variant="message"
          />
        ) : (
          <DiffViewer
            file={activeFile}
            mode={diffMode as DiffViewerMode}
            onModeChange={onDiffModeChange}
          />
        )}
      </main>

      <aside className="pr-cockpit__pane pr-cockpit__pane--review" tabIndex={0}>
        <h2 className="pr-cockpit__pane-title">Review</h2>
        <ReviewSidebar
          detail={data}
          onApprove={onApprove}
          onMerge={onMerge}
          isBusy={isBusy}
        />
        <MergeGatePanel passport={data.merge_passport} />
        <ChecksPanel checks={checks.data ?? null} isLoading={checks.isPending} />
        <ThreadList threads={threads.data?.threads ?? []} />
      </aside>
    </div>
  );
}

// MergeRequestPage.tsx — Phase 3 merge cockpit (W-FE-11).
//
// Three-pane layout per FINAL §4.6:
//   ┌────────────────────────────────────────────────────────────────────┐
//   │ MR #42: title  head abc123  target main  Passport: BLOCKED        │
//   ├──────────────┬──────────────────────────────────┬──────────────────┤
//   │ Files        │ Diff/File Viewer                 │ Review Panel     │
//   │ filters      │ inline comments                  │ Passport         │
//   │ risk badges  │ syntax highlighted               │ Checks           │
//   │ viewed       │ virtualized                      │ Threads          │
//   └──────────────┴──────────────────────────────────┴──────────────────┘
//
// On approve mutation 409 with `merge_sha_stale`, the page shows a recovery
// banner with the old/new SHA and a Refresh button that re-runs the detail
// query. The banner also appears for `merge_passport_stale` / `concurrency
// _conflict` so reviewers see all known drift cases.

import { GitBranch, GitMerge, RefreshCcw, ShieldAlert } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useParams } from 'react-router-dom';

import { ApiError } from '../api/client';
import { ActionButton } from '../components/action/ActionButton';
import {
  ChecksPanel,
  DiffFileTree,
  DiffViewer,
  MergeGatePanel,
  ReviewSidebar,
  ThreadList,
  type DiffViewerMode,
} from '../components/merge';
import {
  ErrorState,
  LoadingState,
  PermissionDeniedState,
} from '../components/state';
import { useApproveMr } from '../hooks/useApproveMr';
import { useMergeMr } from '../hooks/useMergeMr';
import { useMergeRequest } from '../hooks/useMergeRequest';
import { useMrChecks } from '../hooks/useMrChecks';
import { useMrDiff } from '../hooks/useMrDiff';
import { useMrThreads } from '../hooks/useMrThreads';
import { useRealtime } from '../hooks/useRealtime';
import { useResolveRepo } from '../hooks/useResolveRepo';
import { usePreferencesStore } from '../stores/preferencesStore';
import { useSelectionStore } from '../stores/selectionStore';

import './page.css';

function fullNameFromParams(params: Record<string, string | undefined>): string {
  return params.fullName ?? '';
}

interface StaleHeadInfo {
  /** SHA the user saw when they pressed the action. */
  expected: string;
  /** SHA the backend reported is current. */
  current: string;
  /** Last error code so we can word the banner accurately. */
  code: 'merge_sha_stale' | 'merge_passport_stale' | 'concurrency_conflict';
}

function extractStale(error: ApiError): StaleHeadInfo | null {
  const code = error.code;
  if (
    code !== 'merge_sha_stale' &&
    code !== 'merge_passport_stale' &&
    code !== 'concurrency_conflict'
  ) {
    return null;
  }
  const details = error.details ?? {};
  const expected =
    (details.expected_head_sha as string | undefined) ??
    (details.expected_sha as string | undefined) ??
    '';
  const current =
    (details.current_head_sha as string | undefined) ??
    (details.current_sha as string | undefined) ??
    (details.head_sha as string | undefined) ??
    '';
  return { expected, current, code };
}

export function MergeRequestPage(): JSX.Element {
  const params = useParams();
  const provider = params.provider ?? 'unknown';
  const fullName = fullNameFromParams(params);
  const iid = params.iid ?? null;

  const resolved = useResolveRepo(provider, fullName);
  const repoId = resolved.data?.id ?? null;
  const setMr = useSelectionStore((s) => s.setCurrentMr);
  const setRepo = useSelectionStore((s) => s.setCurrentRepo);

  useEffect(() => {
    setRepo(repoId);
    return () => setRepo(null);
  }, [repoId, setRepo]);

  useEffect(() => {
    if (!iid) return undefined;
    setMr(iid);
    return () => setMr(null);
  }, [iid, setMr]);

  useRealtime(iid ? [`mr.${iid}`] : []);

  const detail = useMergeRequest(repoId, iid);
  const diff = useMrDiff(repoId, iid);
  const checks = useMrChecks(repoId, iid);
  const threads = useMrThreads(repoId, iid);

  const approve = useApproveMr(repoId, iid);
  const mergeMutation = useMergeMr(repoId, iid);

  // Diff viewer state.
  const [activeFilePath, setActiveFilePath] = useState<string | null>(null);
  const [viewedPaths, setViewedPaths] = useState<Set<string>>(() => new Set());
  const diffMode = usePreferencesStore((s) => s.diffMode);
  const setDiffMode = usePreferencesStore((s) => s.setDiffMode);

  // Default to the first file once the diff arrives.
  useEffect(() => {
    if (!activeFilePath && diff.data && diff.data.files.length > 0) {
      setActiveFilePath(diff.data.files[0]?.path ?? null);
    }
  }, [activeFilePath, diff.data]);

  const activeFile = useMemo(() => {
    if (!diff.data || !activeFilePath) return null;
    return diff.data.files.find((f) => f.path === activeFilePath) ?? null;
  }, [diff.data, activeFilePath]);

  const handleToggleViewed = useCallback((path: string, viewed: boolean) => {
    setViewedPaths((prev) => {
      const next = new Set(prev);
      if (viewed) next.add(path);
      else next.delete(path);
      return next;
    });
  }, []);

  const handleApprove = useCallback(
    async (expectedHeadSha: string) => {
      approve.reset();
      await approve.mutateAsync({ expected_head_sha: expectedHeadSha });
    },
    [approve]
  );

  const handleMerge = useCallback(
    async (input: {
      expectedHeadSha: string;
      expectedPassportHash: string | null;
      method: 'merge' | 'squash' | 'rebase';
    }) => {
      mergeMutation.reset();
      await mergeMutation.mutateAsync({
        expected_head_sha: input.expectedHeadSha,
        expected_passport_hash: input.expectedPassportHash,
        merge_method: input.method,
      });
    },
    [mergeMutation]
  );

  // Aggregate the stale-head signal from either mutation.
  const staleHead = useMemo<StaleHeadInfo | null>(() => {
    const approveErr = approve.error;
    const mergeErr = mergeMutation.error;
    if (approveErr instanceof ApiError) {
      const info = extractStale(approveErr);
      if (info) return info;
    }
    if (mergeErr instanceof ApiError) {
      const info = extractStale(mergeErr);
      if (info) return info;
    }
    return null;
  }, [approve.error, mergeMutation.error]);

  const handleRefresh = useCallback(() => {
    approve.reset();
    mergeMutation.reset();
    void detail.refetch();
    void diff.refetch();
    void checks.refetch();
    void threads.refetch();
  }, [approve, mergeMutation, detail, diff, checks, threads]);

  // ── Loading + error guards. ────────────────────────────────────────
  if (resolved.isPending) {
    return (
      <div className="page">
        <LoadingState
          title={`Loading MR !${iid}…`}
          variant="message"
          description="Resolving the repository."
        />
      </div>
    );
  }

  if (resolved.error || !resolved.data) {
    if (resolved.error instanceof ApiError && resolved.error.status === 403) {
      return (
        <div className="page">
          <PermissionDeniedState
            description="You do not have permission to view this merge request."
            missingPermission="repo.read"
          />
        </div>
      );
    }
    return (
      <div className="page">
        <ErrorState
          title="Repository not found"
          description={resolved.error?.message ?? `No repository ${fullName}.`}
        />
      </div>
    );
  }

  if (detail.isPending) {
    return (
      <div className="page">
        <LoadingState title="Loading merge request…" variant="message" />
      </div>
    );
  }

  if (detail.error || !detail.data) {
    if (detail.error instanceof ApiError && detail.error.status === 403) {
      return (
        <div className="page">
          <PermissionDeniedState
            description="You do not have permission to view this merge request."
            missingPermission="mr.read"
          />
        </div>
      );
    }
    return (
      <div className="page">
        <ErrorState
          title="Could not load merge request"
          error={detail.error}
        />
      </div>
    );
  }

  const data = detail.data;
  const summary = data.summary;
  const passport = data.merge_passport;
  const passportTone: 'pass' | 'blocked' | 'pending' =
    passport?.status ?? 'pending';

  return (
    <div className="page page--full">
      <div className="mr-cockpit__header">
        <h1 className="mr-cockpit__title">
          MR !{summary.iid}: {summary.title}
        </h1>
        <span className="mr-cockpit__meta">
          <GitBranch aria-hidden="true" size={12} />
          <code>{summary.source_branch}</code>
          <span aria-hidden="true">→</span>
          <code>{summary.target_branch}</code>
        </span>
        <span className="mr-cockpit__meta">
          <code title={summary.head_sha}>{summary.head_sha.slice(0, 7)}</code>
        </span>
        <span
          className={`mr-cockpit__passport-pill mr-cockpit__passport-pill--${passportTone}`}
        >
          <GitMerge aria-hidden="true" size={12} />
          Passport: {passportTone.toUpperCase()}
        </span>
      </div>

      {staleHead ? (
        <div className="mr-cockpit__recovery" role="alert">
          <div className="mr-cockpit__recovery-title">
            <ShieldAlert aria-hidden="true" size={14} />
            {staleHead.code === 'merge_passport_stale'
              ? 'Merge Passport recomputed since you opened this view.'
              : staleHead.code === 'concurrency_conflict'
              ? 'Another reviewer touched this merge request.'
              : 'Head SHA changed since you opened this view.'}
          </div>
          {staleHead.expected && staleHead.current ? (
            <div className="mr-cockpit__recovery-shas">
              Head changed from <code>{staleHead.expected.slice(0, 7)}</code>
              {' '}→ <code>{staleHead.current.slice(0, 7)}</code>. Refresh to
              re-review.
            </div>
          ) : (
            <div className="mr-cockpit__recovery-shas">
              Refresh to load the latest snapshot before re-reviewing.
            </div>
          )}
          <ActionButton
            variant="primary"
            icon={<RefreshCcw aria-hidden="true" size={12} />}
            onClick={handleRefresh}
          >
            Refresh
          </ActionButton>
        </div>
      ) : null}

      <div className="mr-cockpit">
        <aside className="mr-cockpit__pane mr-cockpit__pane--files">
          <h2 className="mr-cockpit__pane-title">Files</h2>
          {diff.isPending ? (
            <LoadingState title="Loading diff…" variant="skeleton" rows={6} />
          ) : diff.error ? (
            <ErrorState title="Diff failed" error={diff.error} />
          ) : (
            <DiffFileTree
              files={diff.data?.files ?? []}
              activePath={activeFilePath}
              viewedPaths={viewedPaths}
              onSelect={setActiveFilePath}
              onToggleViewed={handleToggleViewed}
            />
          )}
        </aside>

        <main className="mr-cockpit__pane mr-cockpit__pane--diff">
          <h2 className="mr-cockpit__pane-title">Diff</h2>
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
              onModeChange={(m) => setDiffMode(m)}
            />
          )}
        </main>

        <aside className="mr-cockpit__pane mr-cockpit__pane--review">
          <h2 className="mr-cockpit__pane-title">Review</h2>
          <ReviewSidebar
            detail={data}
            onApprove={handleApprove}
            onMerge={handleMerge}
            isBusy={approve.isPending || mergeMutation.isPending}
          />
          <MergeGatePanel passport={passport} />
          <ChecksPanel
            checks={checks.data ?? null}
            isLoading={checks.isPending}
          />
          <ThreadList threads={threads.data?.threads ?? []} />
        </aside>
      </div>
    </div>
  );
}

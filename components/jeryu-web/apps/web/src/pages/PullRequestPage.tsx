// PullRequestPage.tsx — Phase 3 PR review cockpit (W-FE-11).
//
// Three-pane layout per FINAL §4.6:
//   ┌────────────────────────────────────────────────────────────────────┐
//   │ PR #42: title  head abc123  base main  Passport: BLOCKED          │
//   ├──────────────┬──────────────────────────────────┬──────────────────┤
//   │ Files        │ Diff/File Viewer                 │ Review Panel     │
//   │ filters      │ inline comments                  │ Passport         │
//   │ risk badges  │ syntax highlighted               │ Checks           │
//   │ viewed       │ virtualized                      │ Threads          │
//   └──────────────┴──────────────────────────────────┴──────────────────┘
//
// On approve mutation 409 with `merge_sha_stale`, the page shows a recovery
// banner with the previous/current SHA and a Refresh button that re-runs the
// detail query. The banner also appears for `merge_passport_stale` /
// `concurrency_conflict` so reviewers see all known drift cases.

import { GitBranch, GitMerge, RefreshCcw, ShieldAlert } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useParams } from 'react-router-dom';

import { ApiError } from '../api/client';
import { ActionButton } from '../components/action/ActionButton';
import type { DiffViewerMode } from '../components/merge';
import {
  ErrorState,
  LoadingState,
  PermissionDeniedState,
} from '../components/state';
import { useApprovePr } from '../hooks/useApprovePr';
import { useMergePr } from '../hooks/useMergePr';
import { usePullRequest } from '../hooks/usePullRequest';
import { usePrChecks } from '../hooks/usePrChecks';
import { usePrDiff } from '../hooks/usePrDiff';
import { usePrThreads } from '../hooks/usePrThreads';
import { useRealtime } from '../hooks/useRealtime';
import { useResolveRepo } from '../hooks/useResolveRepo';
import { usePreferencesStore } from '../stores/preferencesStore';
import { useSelectionStore } from '../stores/selectionStore';
import { PullRequestCockpit } from './PullRequestCockpit';
import {
  extractDrift,
  type HeadDriftInfo,
} from './pullRequestDrift';

import './page.css';

function fullNameFromParams(params: Record<string, string | undefined>): string {
  return params.fullName ?? '';
}

export interface PullRequestPageProps {
  provider?: string;
  fullName?: string;
  prNumber?: string;
}

export function PullRequestPage(props: PullRequestPageProps = {}): JSX.Element {
  const params = useParams();
  const provider = props.provider ?? params.provider ?? 'unknown';
  const fullName = props.fullName ?? fullNameFromParams(params);
  const prNumber = props.prNumber ?? params.number ?? null;

  const resolved = useResolveRepo(provider, fullName);
  const repoId = resolved.data?.id ?? null;
  const setPr = useSelectionStore((s) => s.setCurrentPr);
  const setRepo = useSelectionStore((s) => s.setCurrentRepo);

  useEffect(() => {
    setRepo(repoId);
    return () => setRepo(null);
  }, [repoId, setRepo]);

  useEffect(() => {
    if (!prNumber) return () => {};
    setPr(prNumber);
    return () => setPr(null);
  }, [prNumber, setPr]);

  useRealtime(prNumber ? [`pull.${prNumber}`] : []);

  const detail = usePullRequest(repoId, prNumber);
  const diff = usePrDiff(repoId, prNumber);
  const checks = usePrChecks(repoId, prNumber);
  const threads = usePrThreads(repoId, prNumber);

  const approve = useApprovePr(repoId, prNumber);
  const mergeMutation = useMergePr(repoId, prNumber);

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
    if (!diff.data || !activeFilePath) return;
    return diff.data.files.find((f) => f.path === activeFilePath);
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

  // Aggregate the head-drift signal from either mutation.
  const headDrift = useMemo<HeadDriftInfo | undefined>(() => {
    const approveErr = approve.error;
    const mergeErr = mergeMutation.error;
    if (approveErr instanceof ApiError) {
      const info = extractDrift(approveErr);
      if (info) return info;
    }
    if (mergeErr instanceof ApiError) {
      const info = extractDrift(mergeErr);
      if (info) return info;
    }
    return;
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
          title={`Loading PR #${prNumber}…`}
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
            description="You do not have permission to view this pull request."
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
        <LoadingState title="Loading pull request…" variant="message" />
      </div>
    );
  }

  if (detail.error || !detail.data) {
    if (detail.error instanceof ApiError && detail.error.status === 403) {
      return (
        <div className="page">
          <PermissionDeniedState
            description="You do not have permission to view this pull request."
            missingPermission="pr.read"
          />
        </div>
      );
    }
    return (
      <div className="page">
        <ErrorState
          title="Could not load pull request"
          error={detail.error}
        />
      </div>
    );
  }

  const data = detail.data;
  const summary = data.summary;
  const passportTone: 'pass' | 'blocked' | 'pending' =
    data.merge_passport?.status ?? 'pending';

  return (
    <div className="page page--full">
      <div className="pr-cockpit__header">
        <h1 className="pr-cockpit__title">
          PR #{summary.number}: {summary.title}
        </h1>
        <span className="pr-cockpit__meta">
          <GitBranch aria-hidden="true" size={12} />
          <code>{summary.head_ref}</code>
          <span aria-hidden="true">→</span>
          <code>{summary.base_ref}</code>
        </span>
        <span className="pr-cockpit__meta">
          <code title={summary.head_sha}>{summary.head_sha.slice(0, 7)}</code>
        </span>
        <span
          className={`pr-cockpit__passport-pill pr-cockpit__passport-pill--${passportTone}`}
        >
          <GitMerge aria-hidden="true" size={12} />
          Passport: {passportTone.toUpperCase()}
        </span>
      </div>

      {headDrift ? (
        <div className="pr-cockpit__recovery" role="alert">
          <div className="pr-cockpit__recovery-title">
            <ShieldAlert aria-hidden="true" size={14} />
            {headDrift.code === 'merge_passport_stale'
              ? 'Merge Passport recomputed since you opened this view.'
              : headDrift.code === 'concurrency_conflict'
              ? 'Another reviewer touched this pull request.'
              : 'Head SHA changed since you opened this view.'}
          </div>
          {headDrift.expected && headDrift.current ? (
            <div className="pr-cockpit__recovery-shas">
              Head changed from <code>{headDrift.expected.slice(0, 7)}</code>
              {' '}→ <code>{headDrift.current.slice(0, 7)}</code>. Refresh to
              re-review.
            </div>
          ) : (
            <div className="pr-cockpit__recovery-shas">
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

      <PullRequestCockpit
        data={data}
        diff={diff}
        checks={checks}
        threads={threads}
        activeFilePath={activeFilePath}
        activeFile={activeFile}
        viewedPaths={viewedPaths}
        diffMode={diffMode}
        isBusy={approve.isPending || mergeMutation.isPending}
        onSelectFile={setActiveFilePath}
        onToggleViewed={handleToggleViewed}
        onDiffModeChange={(m: DiffViewerMode) => setDiffMode(m)}
        onApprove={handleApprove}
        onMerge={handleMerge}
      />
    </div>
  );
}

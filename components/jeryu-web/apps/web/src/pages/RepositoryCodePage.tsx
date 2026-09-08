// RepositoryCodePage.tsx — Phase 2 implementation (W-FE-10).
//
// Three-pane layout: BranchSelector + Breadcrumbs on top, FileTree on the
// left, content area on the right. The content area is intentionally empty
// for this page; selecting a file in the tree navigates to
// `/repos/:provider/*fullName/blob/:ref/*path`. A fuzzy file finder
// (`cmdk`-based) opens on the `t` keyboard shortcut, mirroring GitHub's
// behaviour.

import { Command } from 'cmdk';
import { useEffect, useMemo, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';

import { ApiError } from '../api/client';
import { ActionButton } from '../components/action/ActionButton';
import {
  BranchSelector,
  Breadcrumbs,
  FileTree,
} from '../components/browser';
import {
  EmptyState,
  ErrorState,
  LoadingState,
  PermissionDeniedState,
} from '../components/state';
import { useKeyboardShortcut } from '../hooks/useKeyboard';
import { useRepoTree } from '../hooks/useRepoTree';
import { useResolveRepo } from '../hooks/useResolveRepo';
import type { TreeEntry } from '../api/types';

import '../components/browser/browser.css';
import './page.css';

function fullNameFromParams(params: Record<string, string | undefined>): string {
  const main = params.fullName ?? '';
  const tail = params['*'] ?? '';
  if (tail) {
    return `${main}/${tail}`.replace(/\/+$/, '');
  }
  return main;
}

export interface RepositoryCodePageProps {
  provider?: string;
  fullName?: string;
}

export function RepositoryCodePage(props: RepositoryCodePageProps = {}): JSX.Element {
  const params = useParams();
  const navigate = useNavigate();
  const provider = props.provider ?? params.provider ?? 'unknown';
  const fullName = props.fullName ?? fullNameFromParams(params);
  const resolved = useResolveRepo(provider, fullName);

  const repoId = resolved.data?.id ?? null;
  const defaultBranch = resolved.data?.summary.default_branch ?? '';
  const [activeRef, setActiveRef] = useState<string>('');
  const [finderOpen, setFinderOpen] = useState(false);

  useEffect(() => {
    if (defaultBranch && !activeRef) setActiveRef(defaultBranch);
  }, [defaultBranch, activeRef]);

  useKeyboardShortcut(
    't',
    () => {
      if (repoId) setFinderOpen(true);
    },
    { label: 'Open file finder', group: 'Navigation' }
  );

  const navigateToFile = (entry: TreeEntry): void => {
    navigate(
      `/repos/${encodeURIComponent(provider)}/${fullName}/blob/${encodeURIComponent(activeRef || defaultBranch)}/${entry.path}`
    );
  };

  if (resolved.isPending) {
    return (
      <div className="page" data-testid="repo-code-page">
        <LoadingState title="Loading repository…" variant="message" />
      </div>
    );
  }

  if (resolved.error) {
    if (resolved.error instanceof ApiError && resolved.error.status === 403) {
      return (
        <div className="page" data-testid="repo-code-page">
          <PermissionDeniedState
            description="You do not have permission to view this repository."
            missingPermission="repo.read"
          />
        </div>
      );
    }
    return (
      <div className="page" data-testid="repo-code-page">
        <ErrorState
          title="Could not load repository"
          error={resolved.error}
        />
      </div>
    );
  }

  if (!resolved.data) {
    return (
      <div className="page" data-testid="repo-code-page">
        <ErrorState
          title="Repository not found"
          description={`No repository named ${fullName} on ${provider}.`}
        />
      </div>
    );
  }

  return (
    <div className="page" data-testid="repo-code-page">
      <div className="code-browser-layout__top">
        <BranchSelector
          repoId={repoId}
          value={activeRef}
          onSelect={setActiveRef}
        />
        <Breadcrumbs
          segments={[
            { label: 'Repos', to: '/repos' },
            { label: provider },
            {
              label: `${resolved.data.summary.id.owner}/${resolved.data.summary.id.name}`,
              to: `/repos/${encodeURIComponent(provider)}/${fullName}`,
            },
            { label: 'code' },
          ]}
        />
        <ActionButton
          variant="ghost"
          onClick={() => setFinderOpen(true)}
          aria-label="Find files"
        >
          Find files (t)
        </ActionButton>
      </div>
      <section className="code-browser-layout">
        <aside className="code-browser-layout__sidebar" aria-label="File tree">
          <FileTree
            repoId={repoId}
            refName={activeRef || defaultBranch}
            onSelectFile={navigateToFile}
          />
        </aside>
        <div className="code-browser-layout__main">
          <EmptyState
            title="Pick a file"
            description="Choose a file in the tree to view its contents, or press 't' to fuzzy-find."
          />
        </div>
      </section>
      <FileFinder
        open={finderOpen}
        onClose={() => setFinderOpen(false)}
        repoId={repoId}
        refName={activeRef || defaultBranch}
        onPick={(entry) => {
          navigateToFile(entry);
          setFinderOpen(false);
        }}
      />
    </div>
  );
}

interface FileFinderProps {
  open: boolean;
  onClose: () => void;
  repoId: string | null;
  refName: string;
  onPick: (entry: TreeEntry) => void;
}

function FileFinder({
  open,
  onClose,
  repoId,
  refName,
  onPick,
}: FileFinderProps): JSX.Element | null {
  const [query, setQuery] = useState('');
  // The finder enumerates the root tree listing and fuzzy-filters it. The
  // backend serves a single-level tree per request, so the cmdk panel
  // filters against the entries returned for the current path.
  const tree = useRepoTree(repoId, refName, '');

  useEffect(() => {
    if (!open) return () => {};
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  const items = useMemo<TreeEntry[]>(
    () => (tree.data ?? []).filter((entry) => entry.kind === 'file'),
    [tree.data]
  );

  if (!open) return <></>;

  return (
    <div
      className="file-finder__backdrop"
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <Command className="file-finder__panel" label="Find files" loop>
        <Command.Input
          className="file-finder__input"
          aria-label="Filter files"
          value={query}
          onValueChange={setQuery}
          autoFocus
        />
        <Command.List className="file-finder__list">
          {tree.isPending ? (
            <div className="file-tree__loading">Loading files…</div>
          ) : tree.isError ? (
            <div className="file-tree__error">
              Could not load files.
            </div>
          ) : items.length === 0 ? (
            <Command.Empty className="branch-selector__empty">
              No files at this ref.
            </Command.Empty>
          ) : (
            <>
              <Command.Empty className="branch-selector__empty">
                No matches.
              </Command.Empty>
              {items.map((entry) => (
                <Command.Item
                  key={entry.path}
                  value={entry.path}
                  className="file-finder__item"
                  onSelect={() => onPick(entry)}
                >
                  {entry.path}
                </Command.Item>
              ))}
            </>
          )}
        </Command.List>
        <p className="file-finder__hint">Esc to close · ↑↓ to navigate · ↵ to open</p>
      </Command>
    </div>
  );
}

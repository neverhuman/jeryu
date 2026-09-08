// RepositoryFamilyPage.tsx — split-family browser.

import { Boxes, FileText, GitMerge, Play, ShieldAlert } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { Link, useParams } from 'react-router-dom';

import { ApiError } from '../api/client';
import type { RepositorySummary, TreeEntry } from '../api/types';
import { ActionButton } from '../components/action/ActionButton';
import {
  BranchSelector,
  Breadcrumbs,
  CodeViewer,
  FileTree,
  ReadmePanel,
} from '../components/browser';
import {
  aggregateFamily,
  formatFamilyName,
} from '../components/repo';
import { RepoHealthPill } from '../components/repo/RepoHealthPill';
import {
  EmptyState,
  ErrorState,
  LoadingState,
  PermissionDeniedState,
} from '../components/state';
import { useBlob } from '../hooks/useBlob';
import { useRepositories } from '../hooks/useRepositories';

import '../components/browser/browser.css';
import '../components/repo/repo.css';
import './RepositoryFamilyPage.css';
import './page.css';

export function RepositoryFamilyPage(): JSX.Element {
  const params = useParams();
  const family = params.family ?? '';
  const list = useRepositories({ family, sort: 'name' });

  const repos = useMemo(
    () => sortSplitRepos(list.data?.repositories ?? []),
    [list.data?.repositories]
  );
  const rollup = aggregateFamily(family, repos);

  let body: JSX.Element;
  if (list.isPending) {
    body = <LoadingState title="Loading family repositories..." rows={6} />;
  } else if (list.error) {
    const err = list.error;
    if (
      err instanceof ApiError &&
      (err.status === 403 || err.code === 'permission_denied')
    ) {
      body = (
        <PermissionDeniedState
          description="You do not have permission to view repositories."
          missingPermission="repo.read"
        />
      );
    } else {
      body = (
        <ErrorState
          title="Could not load family repositories"
          error={err}
          action={
            <ActionButton variant="primary" onClick={() => list.refetch()}>
              Retry
            </ActionButton>
          }
        />
      );
    }
  } else if (repos.length === 0) {
    body = (
      <EmptyState
        title="No repositories in this family"
        description={`No repositories carry the family label "${formatFamilyName(family)}".`}
        icon={Boxes}
        action={
          <Link to="/repos">
            <ActionButton variant="primary">Back to repositories</ActionButton>
          </Link>
        }
      />
    );
  } else {
    body = <SplitFamilyBrowser repos={repos} />;
  }

  return (
    <div className="page page--wide split-family-page" data-testid="repository-family-page">
      <Breadcrumbs
        segments={[{ label: 'Repos', to: '/repos' }, { label: family }]}
      />

      <header className="page__header split-family-page__header">
        <div className="page__welcome">
          <h1 className="page__title">{formatFamilyName(family)}</h1>
        </div>
        {!list.isPending && !list.error && repos.length > 0 ? (
          <div
            className="repo-family-strip"
            aria-label={`${formatFamilyName(family)} rollup`}
          >
            <span className="repo-family-strip__item">
              {rollup.memberCount} repo{rollup.memberCount === 1 ? '' : 's'}
            </span>
            <RepoHealthPill health={rollup.health} />
            <span
              className="repo-family-strip__item"
              title="Open pull requests"
              aria-label={`${rollup.openPullRequests} open pull requests`}
            >
              <GitMerge size={12} aria-hidden="true" />{' '}
              {rollup.openPullRequests} open
            </span>
            <span
              className="repo-family-strip__item"
              title="Failing checks"
              aria-label={`${rollup.failingChecks} failing checks`}
            >
              <ShieldAlert size={12} aria-hidden="true" />{' '}
              {rollup.failingChecks} failing
            </span>
            <span
              className="repo-family-strip__item"
              title="Running jobs"
              aria-label={`${rollup.runningJobs} running jobs`}
            >
              <Play size={12} aria-hidden="true" /> {rollup.runningJobs}{' '}
              running
            </span>
          </div>
        ) : null}
      </header>

      {body}
    </div>
  );
}

function SplitFamilyBrowser({
  repos,
}: {
  repos: RepositorySummary[];
}): JSX.Element {
  const [selectedId, setSelectedId] = useState<string>(repos[0]?.id.id ?? '');
  const selected = repos.find((repo) => repo.id.id === selectedId) ?? repos[0];
  const [activeRef, setActiveRef] = useState(selected?.default_branch ?? '');
  const [selectedFile, setSelectedFile] = useState<TreeEntry | null>(null);
  const blob = useBlob(selected?.id.id ?? null, activeRef, selectedFile?.path ?? '');

  useEffect(() => {
    if (!repos.some((repo) => repo.id.id === selectedId)) {
      setSelectedId(repos[0]?.id.id ?? '');
    }
  }, [repos, selectedId]);

  useEffect(() => {
    setActiveRef(selected?.default_branch ?? '');
    setSelectedFile(null);
  }, [selected?.id.id, selected?.default_branch]);

  if (!selected) {
    return <EmptyState title="No repository selected" icon={Boxes} />;
  }

  return (
    <section className="split-browser" aria-label="Split repository browser">
      <aside className="split-browser__rail" aria-label="Split repositories">
        {repos.map((repo) => (
          <button
            key={repo.id.id}
            type="button"
            className="split-browser__repo"
            aria-pressed={repo.id.id === selected.id.id}
            onClick={() => setSelectedId(repo.id.id)}
          >
            <span className="split-browser__repo-name">{repo.id.name}</span>
            <span className="split-browser__repo-owner">{repo.id.owner}</span>
          </button>
        ))}
      </aside>

      <div className="split-browser__main">
        <div className="split-browser__toolbar">
          <div className="split-browser__title">
            <strong>{selected.id.owner}/{selected.id.name}</strong>
            {selected.description ? <span>{selected.description}</span> : null}
          </div>
          <BranchSelector
            repoId={selected.id.id}
            value={activeRef}
            onSelect={(refName) => {
              setActiveRef(refName);
              setSelectedFile(null);
            }}
          />
        </div>

        <div className="split-browser__panes">
          <div className="split-browser__tree" aria-label="Files">
            <FileTree
              repoId={selected.id.id}
              refName={activeRef || selected.default_branch}
              selectedPath={selectedFile?.path}
              onSelectFile={setSelectedFile}
            />
          </div>
          <div className="split-browser__preview" aria-label="Preview">
            {selectedFile ? (
              blob.isPending ? (
                <LoadingState title={`Loading ${selectedFile.path}...`} variant="message" />
              ) : blob.error ? (
                <ErrorState title="Could not load file" error={blob.error} />
              ) : blob.data ? (
                <CodeViewer
                  path={selectedFile.path}
                  text={blob.data.text}
                  renderedHtml={blob.data.rendered_markdown?.html ?? null}
                  mime={blob.data.mime}
                  isBinary={blob.data.is_binary}
                />
              ) : (
                <EmptyState title="File is empty" icon={FileText} />
              )
            ) : (
              <ReadmePanel
                repoId={selected.id.id}
                ref={activeRef || selected.default_branch}
              />
            )}
          </div>
        </div>
      </div>
    </section>
  );
}

function sortSplitRepos(repos: RepositorySummary[]): RepositorySummary[] {
  return [...repos].sort((a, b) => {
    if (a.id.name === 'jeryu') return -1;
    if (b.id.name === 'jeryu') return 1;
    return a.id.name.localeCompare(b.id.name);
  });
}

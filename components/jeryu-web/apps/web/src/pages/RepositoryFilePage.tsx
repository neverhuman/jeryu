// RepositoryFilePage.tsx — Phase 2 implementation (W-FE-10).
//
// Route: `/repos/:provider/:fullName/blob/*`. The splat carries
// `${ref}/${path}` so we parse the first segment as the ref and treat the
// rest as the file path. Calls `useBlob(repoId, ref, path)` and renders the
// `<CodeViewer>` with raw + rendered tabs.
//
// Toolbar buttons: Raw (link to `/api/v1/repos/{id}/raw`), Download (blob ->
// object URL), Copy permalink (current SHA-pinned URL).

import { Check, Copy, Download, ExternalLink } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useParams } from 'react-router-dom';

import { ApiError } from '../api/client';
import { endpoints } from '../api/endpoints';
import { ActionButton } from '../components/action/ActionButton';
import {
  Breadcrumbs,
  CodeViewer,
} from '../components/browser';
import {
  EmptyState,
  ErrorState,
  LoadingState,
  PermissionDeniedState,
} from '../components/state';
import { useBlob } from '../hooks/useBlob';
import { useResolveRepo } from '../hooks/useResolveRepo';

import '../components/browser/browser.css';
import './page.css';

function fullNameFromParams(params: Record<string, string | undefined>): string {
  return params.fullName ?? '';
}

function parseRefAndPath(splat: string): { ref: string; path: string } {
  if (!splat) return { ref: '', path: '' };
  const slash = splat.indexOf('/');
  if (slash === -1) return { ref: splat, path: '' };
  return { ref: splat.slice(0, slash), path: splat.slice(slash + 1) };
}

export interface RepositoryFilePageProps {
  provider?: string;
  fullName?: string;
  blobPath?: string;
}

export function RepositoryFilePage(props: RepositoryFilePageProps = {}): JSX.Element {
  const params = useParams();
  const provider = props.provider ?? params.provider ?? 'unknown';
  const fullName = props.fullName ?? fullNameFromParams(params);
  const splat = props.blobPath ?? params['*'] ?? '';
  const { ref, path } = parseRefAndPath(splat);

  const resolved = useResolveRepo(provider, fullName);
  const repoId = resolved.data?.id ?? null;
  const blob = useBlob(repoId, ref, path);
  const [copied, setCopied] = useState(false);

  const pathSegments = useMemo(() => {
    const parts = path.split('/').filter(Boolean);
    const segs = [
      { label: 'Repos', to: '/repos' },
      { label: provider },
      {
        label: fullName,
        to: `/repos/${encodeURIComponent(provider)}/${fullName}`,
      },
      {
        label: ref,
        to: `/repos/${encodeURIComponent(provider)}/${fullName}/code`,
      },
    ];
    let accum = '';
    for (let i = 0; i < parts.length; i += 1) {
      accum = accum ? `${accum}/${parts[i]}` : parts[i] ?? '';
      const isLast = i === parts.length - 1;
      segs.push({
        label: parts[i] ?? '',
        to: isLast
          ? `/repos/${encodeURIComponent(provider)}/${fullName}/blob/${ref}/${accum}`
          : `/repos/${encodeURIComponent(provider)}/${fullName}/blob/${ref}/${accum}`,
      });
    }
    return segs;
  }, [provider, fullName, ref, path]);

  if (resolved.isPending) {
    return (
      <div className="page">
        <LoadingState title="Loading repository…" variant="message" />
      </div>
    );
  }
  if (resolved.error || !resolved.data) {
    if (
      resolved.error instanceof ApiError &&
      resolved.error.status === 403
    ) {
      return (
        <div className="page">
          <PermissionDeniedState
            description="You do not have permission to view this repository."
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

  if (!path) {
    return (
      <div className="page">
        <Breadcrumbs segments={pathSegments} />
        <EmptyState
          title="No file selected"
          description="Open a file from the code browser."
        />
      </div>
    );
  }

  if (blob.isPending) {
    return (
      <div className="page">
        <Breadcrumbs segments={pathSegments} />
        <LoadingState title={`Loading ${path}…`} variant="message" />
      </div>
    );
  }

  if (blob.error) {
    if (blob.error instanceof ApiError && blob.error.status === 403) {
      return (
        <div className="page">
          <Breadcrumbs segments={pathSegments} />
          <PermissionDeniedState
            description="You do not have permission to view this file."
            missingPermission="repo.read"
          />
        </div>
      );
    }
    return (
      <div className="page">
        <Breadcrumbs segments={pathSegments} />
        <ErrorState
          title="Could not load the file."
          error={blob.error}
        />
      </div>
    );
  }

  if (!blob.data) {
    return (
      <div className="page">
        <Breadcrumbs segments={pathSegments} />
        <EmptyState
          title="File is empty"
          description="The file at this ref has no content."
        />
      </div>
    );
  }

  const rawUrl = endpoints.raw(repoId as string, { ref, path });
  const permalink = `${window.location.origin}/repos/${encodeURIComponent(provider)}/${fullName}/blob/${blob.data.sha}/${path}`;

  const handleCopyPermalink = async (): Promise<void> => {
    try {
      await navigator.clipboard?.writeText(permalink);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // No-op; the URL is still derivable from the address bar.
    }
  };

  return (
    <div className="page">
      <Breadcrumbs segments={pathSegments} />
      <div className="code-browser-layout__top">
        <a
          href={rawUrl}
          target="_blank"
          rel="noopener noreferrer"
          aria-label="View raw file"
        >
          <ActionButton
            variant="default"
            icon={<ExternalLink size={12} aria-hidden="true" />}
          >
            Raw
          </ActionButton>
        </a>
        <a
          href={rawUrl}
          download={path.split('/').pop() ?? path}
          aria-label="Download file"
        >
          <ActionButton
            variant="default"
            icon={<Download size={12} aria-hidden="true" />}
          >
            Download
          </ActionButton>
        </a>
        <ActionButton
          variant="default"
          onClick={() => void handleCopyPermalink()}
          aria-label="Copy permalink"
          icon={
            copied ? (
              <Check size={12} aria-hidden="true" />
            ) : (
              <Copy size={12} aria-hidden="true" />
            )
          }
        >
          {copied ? 'Copied' : 'Copy permalink'}
        </ActionButton>
      </div>
      <CodeViewer
        path={path}
        text={blob.data.text}
        renderedHtml={blob.data.rendered_markdown?.html ?? null}
        mime={blob.data.mime}
        isBinary={blob.data.is_binary}
      />
    </div>
  );
}

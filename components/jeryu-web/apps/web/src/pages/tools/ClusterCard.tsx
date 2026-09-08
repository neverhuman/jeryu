// tools/ClusterCard.tsx — one member cluster of a pattern family: preview,
// occurrences with exact file spans, and the Ignore (durable feedback) /
// Propose (files a registry proposal + build task) actions.

import { useState } from 'react';
import { EyeOff, Hammer } from 'lucide-react';

import { useIgnoreCluster, useProposeCluster } from '../../hooks/useToolFinder';
import type { ToolFinderCluster } from '../../api/types';
import { CATEGORY_LABELS, categoryPillClass, formatCount } from './toolsHelpers';

/** One member cluster: preview, occurrences, and the Ignore/Propose actions. */
export function ClusterCard({
  cluster,
}: {
  cluster: ToolFinderCluster;
}): JSX.Element {
  const ignore = useIgnoreCluster();
  const propose = useProposeCluster();
  const [receipt, setReceipt] = useState<string | null>(null);

  return (
    <div className="tools-cluster" data-testid={`cluster-${cluster.cluster_id}`}>
      <div className="tools-cluster__head">
        <code className="tools-cluster__id">{cluster.cluster_id}</code>
        <span className="page__pill">{cluster.language}</span>
        <span className={categoryPillClass(cluster.category)}>
          {CATEGORY_LABELS[cluster.category] ?? cluster.category}
        </span>
        <span className="tools-cluster__loc">
          ~{formatCount(cluster.anticipated_loc_saved)} LOC saved
        </span>
        <span className="tools-cluster__spans">
          {cluster.repo_count} repos · {cluster.occurrence_count} occurrences ·{' '}
          {cluster.file_count} files
        </span>
      </div>
      <p className="tools-cluster__insight">{cluster.insight}</p>
      <ul className="tools-cluster__occurrences">
        {cluster.occurrences.map((occ) => (
          <li key={`${occ.repo_id}:${occ.path}:${occ.start_line}`}>
            <span className="tools-cluster__occ-repo">{occ.repo_id}</span>{' '}
            <code>
              {occ.path}:{occ.start_line}-{occ.end_line}
            </code>
            {occ.is_test ? <span className="page__pill">test</span> : null}
          </li>
        ))}
      </ul>
      <details className="tools-cluster__preview">
        <summary>Normalized window</summary>
        <pre>{cluster.normalized_preview}</pre>
      </details>
      <div className="tools-cluster__actions">
        <button
          type="button"
          className="tools-button"
          disabled={propose.isPending}
          onClick={() => {
            propose.mutate(
              { clusterId: cluster.cluster_id },
              {
                onSuccess: (result) => setReceipt(result.message),
                onError: (error) => setReceipt(error.message),
              }
            );
          }}
        >
          <Hammer size={13} aria-hidden="true" /> Propose tool
        </button>
        <button
          type="button"
          className="tools-button tools-button--quiet"
          disabled={ignore.isPending || cluster.ignored}
          onClick={() => {
            const reason = window.prompt(
              'Why is this cluster not a tool-build lead?'
            );
            if (reason && reason.trim()) {
              ignore.mutate({ clusterId: cluster.cluster_id, reason: reason.trim() });
            }
          }}
        >
          <EyeOff size={13} aria-hidden="true" /> Ignore
        </button>
        <span className="tools-cluster__suggestion">
          → {cluster.suggested_kind}: {cluster.suggested_name}
        </span>
      </div>
      {receipt ? <p className="tools-cluster__receipt">{receipt}</p> : null}
    </div>
  );
}

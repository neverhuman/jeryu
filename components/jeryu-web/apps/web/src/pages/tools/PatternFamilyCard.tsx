// tools/PatternFamilyCard.tsx — the pattern-family dashboard's top-level unit:
// an expandable card over the persisted cross-repo scan, revealing its member
// clusters with exact file spans.

import { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';

import type { ToolFinderPatternFamily } from '../../api/types';
import { CATEGORY_LABELS, categoryPillClass, formatCount } from './toolsHelpers';
import { ClusterCard } from './ClusterCard';

/** A pattern family card: the dashboard's top-level unit. */
export function PatternFamilyCard({
  family,
}: {
  family: ToolFinderPatternFamily;
}): JSX.Element {
  const [expanded, setExpanded] = useState(false);
  return (
    <article
      className="page__card tools-family"
      data-testid={`family-${family.family_id}`}
    >
      <button
        type="button"
        className="tools-family__toggle"
        aria-expanded={expanded}
        onClick={() => setExpanded((value) => !value)}
      >
        {expanded ? (
          <ChevronDown size={14} aria-hidden="true" />
        ) : (
          <ChevronRight size={14} aria-hidden="true" />
        )}
        <span className="tools-family__label">{family.label}</span>
        <span className={categoryPillClass(family.category)}>
          {CATEGORY_LABELS[family.category] ?? family.category}
        </span>
        <span className="page__pill">{family.language}</span>
        <span className="tools-family__loc">
          ~{formatCount(family.anticipated_loc_saved)} LOC saved
        </span>
      </button>
      <p className="tools-family__meta">
        {family.clusters.length} cluster
        {family.clusters.length === 1 ? '' : 's'} · {family.repos.length} repos ·{' '}
        {family.occurrence_count} occurrences · {family.file_count} files
      </p>
      <p className="tools-family__repos">{family.repos.join(', ')}</p>
      {expanded ? (
        <div className="tools-family__clusters">
          {family.clusters.map((cluster) => (
            <ClusterCard key={cluster.cluster_id} cluster={cluster} />
          ))}
        </div>
      ) : null}
    </article>
  );
}

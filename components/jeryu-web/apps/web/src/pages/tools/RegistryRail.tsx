// tools/RegistryRail.tsx — the left rail: every registry tool ranked by LOC
// saved (realized + anticipated), from the same summary endpoint the golden box
// reads.

import { useMemo } from 'react';
import { Wrench } from 'lucide-react';

import { EmptyState, ErrorState, LoadingState } from '../../components/state';
import { useToolRegistry } from '../../hooks/useToolRegistry';
import type { ToolRegistryEntry } from '../../api/types';
import { formatCount } from './toolsHelpers';

/** One registry tool in the left rail, ranked by total LOC saved. */
function RailTool({ tool }: { tool: ToolRegistryEntry }): JSX.Element {
  const total = tool.loc_saved + tool.loc_saved_estimate;
  return (
    <li className="tools-rail__tool" data-testid={`rail-tool-${tool.id}`}>
      <div className="tools-rail__tool-head">
        <span className="tools-rail__tool-name">{tool.name}</span>
        <span
          className="tools-rail__tool-loc"
          title={`${formatCount(tool.loc_saved)} realized + ${formatCount(tool.loc_saved_estimate)} anticipated LOC saved`}
        >
          {formatCount(total)} LOC
        </span>
      </div>
      <div className="tools-rail__tool-meta">
        <span className="page__pill">{tool.kind}</span>
        <span className={`page__pill tools-rail__status--${tool.status}`}>
          {tool.status}
        </span>
      </div>
    </li>
  );
}

/** Ranked registry rail (all tools, by realized + anticipated LOC saved). */
export function RegistryRail(): JSX.Element {
  const { data, isPending, isError, error } = useToolRegistry();
  const ranked = useMemo(() => {
    const tools = data?.tools ?? [];
    return [...tools].sort(
      (a, b) =>
        b.loc_saved + b.loc_saved_estimate - (a.loc_saved + a.loc_saved_estimate)
    );
  }, [data]);

  return (
    <aside className="tools-rail" aria-label="Registry tools by LOC saved">
      <h2 className="page__section-title">
        <Wrench size={13} aria-hidden="true" /> Registry tools
        {data ? ` · ${formatCount(data.tool_count)}` : ''}
      </h2>
      {data ? (
        <p className="tools-rail__totals">
          {formatCount(data.realized_loc_saved)} LOC saved
          {data.anticipated_loc_saved > 0
            ? ` · +${formatCount(data.anticipated_loc_saved)} anticipated`
            : ''}
        </p>
      ) : null}
      {isPending ? (
        <LoadingState title="Loading registry…" variant="message" />
      ) : isError ? (
        <ErrorState title="Could not load the registry." error={error} />
      ) : ranked.length === 0 ? (
        <EmptyState
          icon={Wrench}
          title="No registered tools yet."
          description="Propose a cluster from the dashboard to file the first one."
        />
      ) : (
        <ul className="tools-rail__list">
          {ranked.map((tool) => (
            <RailTool key={tool.id} tool={tool} />
          ))}
        </ul>
      )}
    </aside>
  );
}
